//! The modern Kumir IDE, built with egui/eframe.

use std::collections::{BTreeSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::CreationContext;
use egui::text::{CCursor, CCursorRange, LayoutJob, LayoutSection, TextFormat};
use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Frame, Id, Key, Label, Layout, Margin, Modifiers,
    Rect, RichText, ScrollArea, Sense, Stroke, TextEdit, Ui, Vec2,
};

use kumir_core::actors::{ActorKind, ActorState, RobotField};
use kumir_core::ast::{BaseType, Program, TypeRef};
use kumir_core::diag::Diagnostic;
use kumir_core::highlight::{highlight, HighlightKind, HighlightSpan};
use kumir_core::kumfile::KumFile;
use kumir_core::value::Value;
use kumir_core::{load_program, parse_source};

use crate::completion;
use crate::file_dialog::{DialogOutcome, FileDialog, Mode};
use crate::icons::Icon;
use crate::runner::{self, RunEvent, RunHandle};
use crate::settings::Settings;
use crate::theme::{self, Palette, Scheme};
use crate::ui;

fn is_touch() -> bool {
    cfg!(target_os = "android")
}

const DEFAULT_PROGRAM: &str = "\u{feff}алг Моя программа\nнач\n    | Напишите здесь свою программу\n    вывод \"Привет, мир!\", нс\nкон\n";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BottomTab {
    Output,
    Errors,
    References,
    Reference,
}

#[derive(Clone, Debug)]
struct RenameState {
    from: String,
    to: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActorFileAction {
    OpenVod,
    SaveVod,
    OpenKz,
    SaveKz,
    OpenPng,
    SavePng,
    SaveDrawing,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum EditorCommand {
    GoToDefinition,
    FindReferences,
    Rename,
    Help,
}

#[derive(Default)]
struct EditorResult {
    toggled_breakpoint: Option<u32>,
    command: Option<EditorCommand>,
    action: Option<Action>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SettingsTab {
    Appearance,
    Editor,
    Run,
    Scheme,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RightTab {
    Structure,
    Actor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FieldTool {
    Color,
    Wall,
    Mark,
    Robot,
    Pan,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditTargetKind {
    Color,
    Wall(u8),
    Mark,
    Robot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EditTarget {
    x: i32,
    y: i32,
    kind: EditTargetKind,
}

#[derive(Clone, Debug)]
struct ErrorEntry {
    text: String,
    line: Option<u32>,
}

struct FindState {
    open: bool,
    query: String,
    replacement: String,
    matches: Vec<(usize, usize)>,
    current: Option<usize>,
}

impl FindState {
    fn new() -> Self {
        Self {
            open: false,
            query: String::new(),
            replacement: String::new(),
            matches: Vec::new(),
            current: None,
        }
    }

    fn refresh(&mut self, text: &str) {
        self.matches.clear();
        if !self.query.is_empty() {
            self.matches = text
                .match_indices(&self.query)
                .map(|(start, m)| (start, start + m.len()))
                .collect();
        }
        if let Some(c) = self.current {
            if c >= self.matches.len() {
                self.current = None;
            }
        }
    }

    fn next_match(&mut self, text: &str) -> Option<(usize, usize)> {
        self.refresh(text);
        if self.matches.is_empty() {
            return None;
        }
        let next = match self.current {
            Some(c) => (c + 1) % self.matches.len(),
            None => 0,
        };
        self.current = Some(next);
        Some(self.matches[next])
    }

    fn prev_match(&mut self, text: &str) -> Option<(usize, usize)> {
        self.refresh(text);
        if self.matches.is_empty() {
            return None;
        }
        let prev = match self.current {
            Some(0) | None => self.matches.len() - 1,
            Some(c) => c - 1,
        };
        self.current = Some(prev);
        Some(self.matches[prev])
    }
}

struct CompletionState {
    open: bool,
    items: Vec<completion::Item>,
    selected: usize,
    /// Character range of the word being replaced.
    replace: (usize, usize),
    prefix: String,
    manual: bool,
}

impl CompletionState {
    fn new() -> Self {
        Self {
            open: false,
            items: Vec::new(),
            selected: 0,
            replace: (0, 0),
            prefix: String::new(),
            manual: false,
        }
    }

    fn close(&mut self) {
        self.open = false;
        self.items.clear();
        self.selected = 0;
        self.manual = false;
    }
}

struct EditorTab {
    id: u64,
    path: Option<PathBuf>,
    title: String,
    text: String,
    file: KumFile,
    dirty: bool,
    highlights: Vec<HighlightSpan>,
    highlight_hash: u64,
    find: FindState,
    cursor_line: u32,
    cursor_col: u32,
    last_text: String,
    undo: Vec<String>,
    redo: Vec<String>,
    pending_select: Option<(usize, usize)>,
    jump_to_line: Option<u32>,
    outline: Option<Program>,
    outline_errors: Vec<Diagnostic>,
    outline_dirty: bool,
    pending_focus: bool,
    completion: CompletionState,
    /// Last known selection as character offsets (primary, secondary).
    sel: (usize, usize),
    /// Position of the context menu, if it is open.
    context_menu: Option<egui::Pos2>,
    /// Word under the cursor when the context menu was opened.
    menu_word: String,
    /// Ghost suffix shown after the cursor (inline suggestion).
    inline_hint: Option<String>,
}

impl EditorTab {
    fn new(id: u64, path: Option<PathBuf>, text: String, file: KumFile) -> Self {
        let title = match &path {
            Some(p) => p
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "Без имени".to_string()),
            None => "Без имени".to_string(),
        };
        let mut tab = Self {
            id,
            path,
            title,
            text,
            file,
            dirty: false,
            highlights: Vec::new(),
            highlight_hash: 0,
            find: FindState::new(),
            cursor_line: 1,
            cursor_col: 1,
            last_text: String::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            pending_select: None,
            jump_to_line: None,
            outline: None,
            outline_errors: Vec::new(),
            outline_dirty: true,
            pending_focus: false,
            completion: CompletionState::new(),
            sel: (0, 0),
            context_menu: None,
            menu_word: String::new(),
            inline_hint: None,
        };
        tab.last_text = tab.text.clone();
        tab.recompute_highlights();
        tab
    }

    fn recompute_highlights(&mut self) {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.text.hash(&mut hasher);
        let h = hasher.finish();
        if h != self.highlight_hash {
            self.highlights = highlight(&self.text);
            self.highlight_hash = h;
        }
        self.find.refresh(&self.text);
    }

    fn on_text_changed(&mut self) {
        if self.last_text != self.text {
            self.undo.push(self.last_text.clone());
            if self.undo.len() > 300 {
                self.undo.remove(0);
            }
            self.redo.clear();
            self.last_text = self.text.clone();
            self.dirty = true;
            self.highlight_hash = 0;
            self.outline_dirty = true;
        }
        self.recompute_highlights();
    }

    fn undo(&mut self) {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(self.text.clone());
            self.text = prev;
            self.last_text = self.text.clone();
            self.dirty = true;
            self.highlight_hash = 0;
            self.outline_dirty = true;
            self.recompute_highlights();
        }
    }

    fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(self.text.clone());
            self.text = next;
            self.last_text = self.text.clone();
            self.dirty = true;
            self.highlight_hash = 0;
            self.outline_dirty = true;
            self.recompute_highlights();
        }
    }

    fn toggle_comment(&mut self, tab_width: usize) {
        let (text, sel) = crate::editing::toggle_comment(&self.text, self.sel, tab_width);
        if text != self.text {
            self.undo.push(self.text.clone());
            if self.undo.len() > 300 {
                self.undo.remove(0);
            }
            self.redo.clear();
            self.text = text;
            self.last_text = self.text.clone();
            self.dirty = true;
            self.highlight_hash = 0;
            self.outline_dirty = true;
            self.sel = sel;
            self.pending_select = Some(sel);
            self.recompute_highlights();
        }
    }

    fn format_document(&mut self, tab_width: usize) {
        let text = crate::editing::format_source(&self.text, tab_width);
        if text != self.text {
            self.undo.push(self.text.clone());
            if self.undo.len() > 300 {
                self.undo.remove(0);
            }
            self.redo.clear();
            let cursor = self.sel.0.min(text.chars().count());
            self.text = text;
            self.last_text = self.text.clone();
            self.dirty = true;
            self.highlight_hash = 0;
            self.outline_dirty = true;
            self.sel = (cursor, cursor);
            self.pending_select = Some((cursor, cursor));
            self.recompute_highlights();
        }
    }

    fn compilation_source(&self) -> String {
        let mut file = self.file.clone();
        file.visible = self.text.clone();
        file.source_for_compilation()
    }

    fn source_dir(&self, fallback: &Path) -> PathBuf {
        self.path
            .as_ref()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| fallback.to_path_buf())
    }
}

#[derive(Clone, Debug)]
enum Action {
    New,
    OpenDialog,
    OpenPath(PathBuf),
    LoadField(PathBuf),
    Save,
    SaveAs,
    CloseTab,
    Quit,
    Run,
    Evaluate,
    Stop,
    Pause,
    Step,
    Check,
    ToggleBreakpoint,
    Find,
    Undo,
    Redo,
    Copy,
    Cut,
    Paste,
    SelectAll,
    GoToDefinition,
    GoToLine,
    QuickOpen,
    ToggleComment,
    FormatDocument,
    SetScheme(Scheme),
    ShowShortcuts,
    NextError,
    PrevError,
    ZoomIn,
    ZoomOut,
    ToggleLeft,
    ToggleRight,
    ToggleBottom,
    ShowHelp,
    ShowAbout,
    ShowSettings,
    ShowActor,
    OpenField,
}

pub struct KumirApp {
    settings: Settings,
    palette: Palette,
    dark: bool,
    font_size: f32,
    show_line_numbers: bool,
    /// `pixels_per_point` reported by the OS; the UI scale is relative to it.
    base_ppp: f32,
    /// Whether the window was narrow on the previous frame (responsive layout).
    last_narrow: bool,
    toast: Option<(String, std::time::Instant)>,

    tabs: Vec<EditorTab>,
    active: usize,
    next_id: u64,

    output: String,
    errors: Vec<ErrorEntry>,
    bottom_tab: BottomTab,
    show_bottom: bool,
    show_left: bool,
    show_right: bool,
    right_tab: RightTab,

    explorer_dir: PathBuf,
    explorer_entries: Vec<PathBuf>,
    explorer_dirty: bool,
    recent: Vec<PathBuf>,

    file_dialog: Option<FileDialog>,
    field_dialog: Option<FileDialog>,
    field_path: Option<PathBuf>,
    field_size: (i32, i32),
    actor: Arc<Mutex<ActorState>>,
    actor_zoom: f32,
    actor_pan: Vec2,
    actor_animate: bool,
    actor_delay_ms: u64,
    actor_edit: bool,
    /// Actor currently shown in the panel (several actors may be connected).
    actor_view: ActorKind,
    painter_texture: Option<(u64, egui::TextureHandle)>,
    pult_a: f64,
    pult_b: f64,
    pult_text: String,
    vod_edit: [i64; 7],
    gh_edit: (i64, i64, i64, bool, i64, i64, String),
    actor_dialog: Option<(FileDialog, ActorFileAction)>,
    field_tool: FieldTool,
    field_undo: Vec<RobotField>,
    field_redo: Vec<RobotField>,
    field_initial: Option<RobotField>,
    field_before_run: Option<RobotField>,
    field_path_before_run: Option<PathBuf>,
    field_dragging_robot: bool,
    field_last_target: Option<EditTarget>,
    field_save_dialog: Option<FileDialog>,
    output_dialog: Option<FileDialog>,
    save_after_dialog: Option<usize>,
    close_request: Option<usize>,
    quit_request: bool,
    allow_close: bool,

    run: Option<RunHandle>,
    run_line: u32,
    run_path: Option<PathBuf>,
    breakpoints: BTreeSet<u32>,
    input_line: String,
    input_focus: bool,
    awaiting_input: bool,
    status: String,
    variables: Vec<(String, String, String)>,
    references: Vec<(u32, String)>,
    rename: Option<RenameState>,

    show_settings: bool,
    show_about: bool,
    show_help: bool,
    show_shortcuts: bool,
    help_focus: Option<String>,
    help_topic: &'static str,
    help_query: String,
    goto_line: Option<String>,
    quick_open: Option<String>,
    settings_tab: SettingsTab,
    scheme_text: String,
    scheme_message: Option<String>,

    zoom_accum: f32,
    field_rect: Option<Rect>,
    pinch_field: bool,

    actions: Vec<Action>,
}

impl KumirApp {
    pub fn new(cc: &CreationContext) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        // Android: IME-текст доставляем сами (winit его не транслирует).
        #[cfg(target_os = "android")]
        {
            cc.egui_ctx.on_begin_pass(
                "kumir-ime",
                std::sync::Arc::new(move |ui| {
                    crate::android_platform::inject_text_input(ui.ctx());
                }),
            );
            // В landscape клавиатура сильно уменьшает окно. Если в этот
            // момент редактор выпадает из видимой области, egui перестаёт
            // отдавать ime-выход → winit прячет клавиатуру → окно растёт
            // обратно → клавиатура снова показывается: бесконечное мигание.
            // Пока фокус в тексте, удерживаем последнюю позицию IME.
            cc.egui_ctx.on_end_pass(
                "kumir-ime-hold",
                std::sync::Arc::new(move |ui| {
                    let ctx = ui.ctx();
                    if !ctx.egui_wants_keyboard_input() {
                        return;
                    }
                    static LAST_IME: std::sync::Mutex<Option<egui::output::IMEOutput>> =
                        std::sync::Mutex::new(None);
                    match ctx.output(|o| o.ime) {
                        Some(v) => *LAST_IME.lock().unwrap() = Some(v),
                        None => {
                            if let Some(v) = *LAST_IME.lock().unwrap() {
                                ctx.output_mut(|o| o.ime = Some(v));
                            }
                        }
                    }
                }),
            );
        }
        let settings = Settings::load();
        let palette = settings.palette();
        let base_ppp = cc.egui_ctx.pixels_per_point();
        if (settings.ui_scale - 1.0).abs() > 0.001 {
            cc.egui_ctx
                .set_pixels_per_point(base_ppp * settings.ui_scale.clamp(0.55, 3.0));
        }
        // На Android cwd = «/», поэтому по умолчанию берём доступный каталог
        // (Документы при наличии разрешения, иначе приватный каталог приложения).
        let cwd = settings
            .last_dir
            .clone()
            .filter(|p| p.is_dir())
            .or_else(crate::android_platform::storage_dir)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        let mut app = Self {
            settings,
            palette,
            dark: palette.dark,
            font_size: 14.0,
            show_line_numbers: true,
            base_ppp,
            last_narrow: false,
            toast: None,
            tabs: Vec::new(),
            active: 0,
            next_id: 1,
            output: String::new(),
            errors: Vec::new(),
            bottom_tab: BottomTab::Output,
            show_bottom: !is_touch(),
            show_left: !is_touch(),
            show_right: !is_touch(),
            right_tab: RightTab::Structure,
            explorer_dir: cwd.clone(),
            explorer_entries: Vec::new(),
            explorer_dirty: true,
            recent: Vec::new(),
            file_dialog: None,
            field_dialog: None,
            field_path: None,
            field_size: (7, 7),
            actor: Arc::new(Mutex::new(ActorState::default())),
            actor_zoom: 1.0,
            actor_pan: Vec2::ZERO,
            actor_animate: true,
            actor_delay_ms: 30,
            actor_edit: true,
            actor_view: ActorKind::None,
            painter_texture: None,
            pult_a: 10.0,
            pult_b: 0.0,
            pult_text: String::new(),
            vod_edit: [3, 5, 7, 0, 0, 0, 1],
            gh_edit: (3, 5, 0, false, -5, 5, String::new()),
            actor_dialog: None,
            field_tool: FieldTool::Color,
            field_undo: Vec::new(),
            field_redo: Vec::new(),
            field_initial: None,
            field_before_run: None,
            field_path_before_run: None,
            field_dragging_robot: false,
            field_last_target: None,
            field_save_dialog: None,
            output_dialog: None,
            save_after_dialog: None,
            close_request: None,
            quit_request: false,
            allow_close: false,
            run: None,
            run_line: 0,
            run_path: None,
            breakpoints: BTreeSet::new(),
            input_line: String::new(),
            input_focus: false,
            awaiting_input: false,
            status: "Готово".to_string(),
            variables: Vec::new(),
            references: Vec::new(),
            rename: None,
            show_settings: false,
            show_about: false,
            show_help: false,
            show_shortcuts: false,
            help_focus: None,
            help_topic: crate::help::default_topic(),
            help_query: String::new(),
            goto_line: None,
            quick_open: None,
            settings_tab: SettingsTab::Appearance,
            scheme_text: String::new(),
            scheme_message: None,
            zoom_accum: 0.0,
            field_rect: None,
            pinch_field: false,
            actions: Vec::new(),
        };
        app.font_size = app.settings.font_size;
        app.show_line_numbers = app.settings.show_line_numbers;
        app.actor_animate = app.settings.animate_actors;
        app.actor_delay_ms = app.settings.actor_delay_ms;
        app.recent = app.settings.recent.clone();
        theme::apply_style(&cc.egui_ctx, &app.palette, app.font_size, is_touch());
        let mut opened = false;
        let session = app.settings.session.clone();
        if !session.is_empty() {
            for path in session {
                if path.is_file() {
                    app.open_path(&path);
                    opened = true;
                }
            }
        }
        if let Some(arg) = std::env::args().nth(1) {
            let p = PathBuf::from(arg);
            if p.is_file() {
                app.open_path(&p);
                opened = true;
            }
        }
        if !opened {
            app.new_tab();
        } else if app.active >= app.tabs.len() {
            app.active = 0;
        }
        app.apply_turtle_bounds();
        app
    }

    /// Save the current UI state into the settings file.
    pub fn persist_settings(&mut self) {
        self.settings.font_size = self.font_size;
        self.settings.editor_font_size = self.font_size;
        self.settings.show_line_numbers = self.show_line_numbers;
        self.settings.animate_actors = self.actor_animate;
        self.settings.actor_delay_ms = self.actor_delay_ms;
        self.settings.recent = self.recent.clone();
        self.settings.session = self.tabs.iter().filter_map(|t| t.path.clone()).collect();
        if let Ok(actor) = self.actor.lock() {
            self.settings.turtle_bounds = actor.turtle.bounds;
        }
        if let Err(e) = self.settings.save() {
            self.toast = Some((e, std::time::Instant::now()));
        }
    }

    fn apply_turtle_bounds(&mut self) {
        if let Ok(mut actor) = self.actor.lock() {
            actor.turtle.bounds = self.settings.turtle_bounds;
        }
    }

    fn set_scheme(&mut self, ctx: &egui::Context, scheme: Scheme) {
        self.settings.set_scheme(scheme);
        self.palette = self.settings.palette();
        self.dark = self.palette.dark;
        theme::apply_style(ctx, &self.palette, self.font_size, is_touch());
    }

    fn apply_ui_scale(&self, ctx: &egui::Context) {
        let scale = self.settings.ui_scale.clamp(0.55, 3.0);
        ctx.set_pixels_per_point(self.base_ppp * scale);
    }

    // ------------------------------------------------------------------
    // Files and tabs
    // ------------------------------------------------------------------

    #[allow(clippy::field_reassign_with_default)]
    fn new_tab(&mut self) {
        let id = self.next_id;
        self.next_id += 1;
        let mut file = KumFile::default();
        file.visible = DEFAULT_PROGRAM.trim_start_matches('\u{feff}').to_string();
        let text = DEFAULT_PROGRAM.trim_start_matches('\u{feff}').to_string();
        file.visible = text.clone();
        self.tabs.push(EditorTab::new(id, None, text, file));
        self.active = self.tabs.len() - 1;
        self.status = "Новая программа".to_string();
    }

    fn open_field_dialog(&mut self) {
        let start = self
            .field_path
            .as_ref()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| self.explorer_dir.clone());
        self.field_dialog = Some(FileDialog::new(Mode::Open, start, ""));
    }

    fn open_dialog(&mut self, mode: Mode) {
        // На Android при первом сохранении просим доступ к общей памяти.
        #[cfg(target_os = "android")]
        {
            if mode == Mode::Save && !crate::android_platform::has_file_access() {
                static ASKED: std::sync::atomic::AtomicBool =
                    std::sync::atomic::AtomicBool::new(false);
                if !ASKED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                    crate::android_platform::request_file_access();
                }
            }
        }
        let start = self
            .active_tab()
            .and_then(|t| t.path.as_ref())
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| self.explorer_dir.clone());
        let name = self
            .active_tab()
            .map(|t| t.title.clone())
            .unwrap_or_else(|| "программа.kum".to_string());
        self.file_dialog = Some(FileDialog::new(mode, start, name));
    }

    fn ensure_actor_for_program(&mut self, modules: &[String]) {
        let mut desired = ActorState::for_modules(modules);
        if desired.kind() == ActorKind::None {
            return;
        }
        let mut keep_field_path = false;
        if desired.kind() == ActorKind::Robot {
            let loaded = self
                .field_path
                .as_ref()
                .and_then(|p| desired.load_field_file(p).ok());
            if loaded.is_some() {
                self.field_size = (desired.robot.width, desired.robot.height);
                keep_field_path = true;
            } else {
                desired.robot = RobotField::empty(self.field_size.0, self.field_size.1);
                desired.status = format!("Поле {}×{}", self.field_size.0, self.field_size.1);
            }
            self.field_initial = Some(desired.robot.clone());
            self.field_before_run = None;
            self.field_path_before_run = None;
        }
        self.actor_view = desired.kind();
        self.actor = Arc::new(Mutex::new(desired));
        if !keep_field_path {
            self.field_path = None;
        }
        self.field_undo.clear();
        self.field_redo.clear();
        self.field_last_target = None;
        self.field_dragging_robot = false;
        self.actor_zoom = 1.0;
        self.actor_pan = Vec2::ZERO;
        self.actor_edit = true;
        self.show_right = true;
        self.right_tab = RightTab::Actor;
    }

    /// Load `<program>.vod` / `<program>.kz` / `<program>.png` next to the
    /// opened program, like the automatic `.fil` field pick-up.
    fn load_sibling_task(&mut self, program: &Path, modules: &[String]) {
        let has = |needle: &str| modules.iter().any(|m| m.to_lowercase().contains(needle));
        let mut loaded = None;
        if has("водолей") || has("vodoley") {
            let path = program.with_extension("vod");
            if path.is_file() {
                if let Ok(v) = kumir_core::actors::VodoleyState::load_vod(&path) {
                    self.vod_edit = [
                        v.capacity[0],
                        v.capacity[1],
                        v.capacity[2],
                        v.initial[0],
                        v.initial[1],
                        v.initial[2],
                        v.target,
                    ];
                    if let Ok(mut actor) = self.actor.lock() {
                        actor.vodoley = v;
                    }
                    loaded = Some(path);
                }
            }
        }
        if has("кузнечик") || has("grasshopper") {
            let path = program.with_extension("kz");
            if path.is_file() {
                if let Ok(g) = kumir_core::actors::GrasshopperState::load_kz(&path) {
                    self.gh_edit = (
                        g.forward,
                        g.backward,
                        g.start,
                        g.borders.is_some(),
                        g.borders.map(|b| b.0).unwrap_or(-5),
                        g.borders.map(|b| b.1).unwrap_or(5),
                        g.flags
                            .iter()
                            .map(|f| f.to_string())
                            .collect::<Vec<_>>()
                            .join(" "),
                    );
                    if let Ok(mut actor) = self.actor.lock() {
                        actor.grasshopper = g;
                    }
                    loaded = Some(path);
                }
            }
        }
        if loaded.is_some() {
            self.show_right = true;
            self.right_tab = RightTab::Actor;
        }
    }

    fn load_field_path(&mut self, path: &Path) {
        if self.run.is_some() {
            self.status = "Нельзя менять поле во время выполнения".to_string();
            return;
        }
        let mut actor = match self.actor.lock() {
            Ok(a) => a,
            Err(_) => return,
        };
        match actor.load_field_file(path) {
            Ok(()) => {
                self.actor_view = ActorKind::Robot;
                self.field_path = Some(path.to_path_buf());
                self.field_size = (actor.robot.width, actor.robot.height);
                self.field_initial = Some(actor.robot.clone());
                self.field_before_run = None;
                self.field_path_before_run = None;
                self.actor_zoom = 1.0;
                self.actor_pan = Vec2::ZERO;
                self.actor_edit = true;
                self.field_undo.clear();
                self.field_redo.clear();
                self.field_last_target = None;
                self.show_right = true;
                self.right_tab = RightTab::Actor;
                self.status = format!(
                    "Поле загружено: {} ({}×{})",
                    path.display(),
                    actor.robot.width,
                    actor.robot.height
                );
            }
            Err(e) => {
                self.errors.push(ErrorEntry {
                    text: e,
                    line: None,
                });
                self.bottom_tab = BottomTab::Errors;
                self.show_bottom = true;
            }
        }
    }

    fn open_path(&mut self, path: &Path) {
        if path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("kod"))
            .unwrap_or(false)
        {
            self.errors.push(ErrorEntry {
                text: "Файлы .kod — это скомпилированный байткод старой версии КуМир.
Откройте исходный файл .kum (он поставляется вместе с .kod)."
                    .to_string(),
                line: None,
            });
            self.bottom_tab = BottomTab::Errors;
            self.show_bottom = true;
            self.status = "Формат .kod не поддерживается".to_string();
            return;
        }
        let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if let Some(i) = self
            .tabs
            .iter()
            .position(|t| t.path.as_ref() == Some(&canon))
        {
            self.active = i;
            self.activate_tab_actor();
            self.status = format!("Открыт {}", canon.display());
            return;
        }
        match load_program(&canon) {
            Ok(loaded) => {
                let id = self.next_id;
                self.next_id += 1;
                let file = loaded.file;
                let text = file.visible.clone();
                let modules = loaded.program.modules.clone();
                let uses_robot = modules.iter().any(|m| {
                    let m = m.to_lowercase();
                    m.contains("робот") || m == "robot"
                });
                let mut tab = EditorTab::new(id, Some(canon.clone()), text, file);
                tab.outline = Some(loaded.program);
                tab.outline_dirty = false;
                self.tabs.push(tab);
                self.ensure_actor_for_program(&modules);
                let sibling_field = canon.with_extension("fil");
                if uses_robot && sibling_field.is_file() {
                    self.load_field_path(&sibling_field);
                }
                self.load_sibling_task(&canon, &modules);
                self.active = self.tabs.len() - 1;
                self.activate_tab_actor();
                self.explorer_dir = canon
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| self.explorer_dir.clone());
                self.explorer_dirty = true;
                self.status = format!("Открыт {}", canon.display());
                self.settings.remember_recent(&canon);
                self.recent = self.settings.recent.clone();
                self.persist_settings();
            }
            Err(errors) => {
                self.errors = errors
                    .into_iter()
                    .map(|e| ErrorEntry {
                        text: e.to_string(),
                        line: Some(e.line),
                    })
                    .collect();
                self.bottom_tab = BottomTab::Errors;
                self.show_bottom = true;
                self.status = "Ошибки при открытии файла".to_string();
            }
        }
    }

    fn save_active(&mut self, save_as: bool) {
        let Some(tab) = self.active_tab() else {
            return;
        };
        if tab.path.is_none() || save_as {
            self.open_dialog(Mode::Save);
        } else if let Some(path) = tab.path.clone() {
            let idx = self.active;
            self.save_tab_to(idx, &path);
        }
    }

    fn save_tab_to(&mut self, idx: usize, path: &Path) -> bool {
        if idx >= self.tabs.len() {
            return false;
        }
        let mut file = self.tabs[idx].file.clone();
        file.visible = self.tabs[idx].text.clone();
        match file.save(path) {
            Ok(()) => {
                self.tabs[idx].file = file;
                self.tabs[idx].path = Some(path.to_path_buf());
                self.tabs[idx].title = path
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Без имени".to_string());
                self.tabs[idx].dirty = false;
                self.status = format!("Сохранено: {}", path.display());
                self.explorer_dir = path
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| self.explorer_dir.clone());
                self.explorer_dirty = true;
                true
            }
            Err(e) => {
                self.errors.push(ErrorEntry {
                    text: format!("Не удалось сохранить «{}»: {e}", path.display()),
                    line: None,
                });
                self.bottom_tab = BottomTab::Errors;
                self.show_bottom = true;
                self.status = "Ошибка сохранения".to_string();
                false
            }
        }
    }

    fn close_tab(&mut self, idx: usize) {
        if idx >= self.tabs.len() {
            return;
        }
        if self.tabs[idx].dirty {
            self.close_request = Some(idx);
            return;
        }
        self.remove_tab(idx);
    }

    fn remove_tab(&mut self, idx: usize) {
        if idx < self.tabs.len() {
            self.tabs.remove(idx);
            if self.active >= self.tabs.len() {
                self.active = self.tabs.len().saturating_sub(1);
            }
            if self.tabs.is_empty() {
                self.new_tab();
            }
        }
    }

    fn active_tab(&self) -> Option<&EditorTab> {
        self.tabs.get(self.active)
    }

    /// Actor modules of the active tab.
    fn tab_actors(&self) -> Vec<ActorKind> {
        let mut result = Vec::new();
        if let Some(program) = self.active_tab().and_then(|t| t.outline.as_ref()) {
            for module in &program.modules {
                if let Some(kind) = ActorKind::from_module(module) {
                    if !result.contains(&kind) {
                        result.push(kind);
                    }
                }
            }
        }
        result
    }

    /// Switch the actor panel to the actors of the newly selected tab.
    fn activate_tab_actor(&mut self) {
        let tab_actors = self.tab_actors();
        if tab_actors.is_empty() {
            self.actor_view = ActorKind::None;
            if self.right_tab == RightTab::Actor {
                self.right_tab = RightTab::Structure;
            }
            return;
        }
        let current = self
            .actor
            .lock()
            .map(|a| a.active.clone())
            .unwrap_or_default();
        if tab_actors.iter().any(|k| !current.contains(k)) {
            let modules = self
                .active_tab()
                .and_then(|t| t.outline.as_ref())
                .map(|p| p.modules.clone())
                .unwrap_or_default();
            self.ensure_actor_for_program(&modules);
        } else {
            self.actor_view = tab_actors[0];
        }
    }

    // ------------------------------------------------------------------
    // Build and run
    // ------------------------------------------------------------------

    fn compile_active(&mut self) -> Option<Program> {
        let tab = self.tabs.get(self.active)?;
        let src = tab.compilation_source();
        let dir = tab.source_dir(&self.explorer_dir);
        let name = tab.title.clone();
        match parse_source(&src, name, Some(&dir)) {
            Ok(program) => Some(program),
            Err(errors) => {
                self.errors = errors
                    .into_iter()
                    .map(|e| ErrorEntry {
                        text: e.to_string(),
                        line: Some(e.line),
                    })
                    .collect();
                self.show_errors_panel();
                self.status = "Найдены ошибки".to_string();
                None
            }
        }
    }

    fn check_active(&mut self) {
        if let Some(program) = self.compile_active() {
            let n = program.algorithms.len();
            self.errors.clear();
            self.append_output(&format!(
                "Синтаксис в порядке: {n} алгоритм(ов), {} глобальных величин.\n",
                program.globals.len()
            ));
            self.bottom_tab = BottomTab::Output;
            self.status = "Проверка завершена: ошибок нет".to_string();
            self.toast(format!(
                "Синтаксис в порядке: {n} алгоритм(ов), {} величин",
                program.globals.len()
            ));
        }
    }

    fn run_active(&mut self, ctx: &egui::Context, evaluate: bool) {
        if self.run.is_some() {
            return;
        }
        let Some(program) = self.compile_active() else {
            return;
        };
        let tab = match self.tabs.get(self.active) {
            Some(t) => t,
            None => return,
        };
        let dir = tab.source_dir(&self.explorer_dir);
        let path = tab.path.clone();
        let title = tab.title.clone();
        if self.settings.auto_save {
            if let Some(path) = &path {
                if self.tabs.get(self.active).map(|t| t.dirty).unwrap_or(false) {
                    let idx = self.active;
                    let path = path.clone();
                    self.save_tab_to(idx, &path);
                }
            }
        }
        if self.breakpoints.is_empty() {
            // Nothing special.
        }
        self.output.clear();
        self.errors.clear();
        self.output.push_str(&format!("=== Запуск: {title} ===\n"));
        self.bottom_tab = BottomTab::Output;
        // A hidden output panel stays hidden: only the user opens it.
        if self.settings.auto_open_output {
            self.show_bottom = true;
        }
        self.awaiting_input = false;
        self.run_line = 0;
        self.variables.clear();
        self.run_path = path.clone();
        // Keep the field that is currently shown (including manual wall/colour
        // edits). Recreating it here was the reason the layout reset on every
        // run.
        let desired = ActorState::for_modules(&program.modules);
        let current = self.actor.lock().ok().map(|a| a.clone());
        let same_actors = |cur: &ActorState| {
            !desired.active.is_empty()
                && desired.active.iter().all(|k| cur.is_active(*k))
                && cur.active.iter().all(|k| desired.active.contains(k))
        };
        let actor_state = match current {
            Some(cur) if same_actors(&cur) => cur,
            _ => {
                let mut fresh = desired;
                if fresh.kind() == ActorKind::Robot {
                    if let Some(field) = &self.field_path {
                        if let Err(e) = fresh.load_field_file(field) {
                            self.errors.push(ErrorEntry {
                                text: e,
                                line: None,
                            });
                        }
                    } else {
                        fresh.robot = RobotField::empty(self.field_size.0, self.field_size.1);
                    }
                }
                fresh
            }
        };
        let has_actor = actor_state.kind() != ActorKind::None;
        if actor_state.kind() == ActorKind::Robot {
            self.field_before_run = Some(actor_state.robot.clone());
            self.field_path_before_run = self.field_path.clone();
        }
        self.actor_view = actor_state.kind();
        self.actor = Arc::new(Mutex::new(actor_state));
        if has_actor {
            if !is_touch() {
                self.show_right = true;
            }
            self.right_tab = RightTab::Actor;
        }
        self.actor_zoom = 1.0;
        self.actor_pan = Vec2::ZERO;
        self.status = "Выполняется…".to_string();
        self.run = Some(runner::start(
            program,
            dir,
            self.breakpoints.clone(),
            self.actor.clone(),
            evaluate,
            Duration::from_millis(if self.actor_animate {
                self.actor_delay_ms
            } else {
                0
            }),
        ));
        ctx.request_repaint();
    }

    fn stop_run(&mut self) {
        if let Some(run) = &self.run {
            run.stop();
        }
        self.awaiting_input = false;
        self.status = "Останавливается…".to_string();
    }

    fn poll_run(&mut self, ctx: &egui::Context) {
        let mut events = VecDeque::new();
        if let Some(run) = &self.run {
            while let Ok(e) = run.events.try_recv() {
                events.push_back(e);
            }
        }
        if events.is_empty() {
            return;
        }
        let mut finished = false;
        for event in events {
            match event {
                RunEvent::Output(text) => self.append_output(&text),
                RunEvent::Line(line) => {
                    self.run_line = line;
                }
                RunEvent::InputRequested => {
                    self.awaiting_input = true;
                    self.input_focus = true;
                    self.bottom_tab = BottomTab::Output;
                    if self.settings.auto_open_output {
                        self.show_bottom = true;
                    } else {
                        self.toast("Программа ждёт ввода — откройте панель «Вывод»");
                    }
                    ctx.request_repaint();
                }
                RunEvent::Vars(vars) => {
                    self.variables = vars;
                    self.show_right = true;
                }
                RunEvent::Finished(result) => {
                    finished = true;
                    self.awaiting_input = false;
                    self.run_line = 0;
                    match result {
                        Ok(()) => {
                            self.append_output("\n=== Программа завершена ===\n");
                            self.status = "Выполнено".to_string();
                        }
                        Err(msg) => {
                            self.append_output(&format!("\n{msg}\n"));
                            self.errors.push(ErrorEntry {
                                text: msg.clone(),
                                line: extract_line(&msg),
                            });
                            self.show_errors_panel();
                            self.status = "Ошибка выполнения".to_string();
                        }
                    }
                }
            }
        }
        if finished {
            if let Some(mut run) = self.run.take() {
                run.finish();
            }
        }
        ctx.request_repaint();
    }

    fn toast(&mut self, text: impl Into<String>) {
        self.toast = Some((text.into(), std::time::Instant::now()));
    }

    /// Show the error list according to the user's preference.
    fn show_errors_panel(&mut self) {
        self.bottom_tab = BottomTab::Errors;
        if self.settings.auto_open_errors {
            self.show_bottom = true;
        } else {
            let count = self.errors.len();
            if count > 0 {
                self.toast(format!("Ошибок: {count} — откройте панель «Ошибки»"));
            }
        }
    }

    fn append_output(&mut self, text: &str) {
        self.output.push_str(text);
        if self.output.len() > 2_000_000 {
            let mut cut = self.output.len() - 1_500_000;
            while cut < self.output.len() && !self.output.is_char_boundary(cut) {
                cut += 1;
            }
            self.output.drain(..cut);
        }
    }

    fn send_input(&mut self) {
        let line = std::mem::take(&mut self.input_line);
        if let Some(run) = &self.run {
            run.send_input(line);
            run.shared
                .awaiting_input
                .store(false, std::sync::atomic::Ordering::SeqCst);
        }
        self.awaiting_input = false;
    }

    /// Collect all whole-word occurrences of the word under the cursor.
    /// Move to the next (`step = 1`) or previous error and open the panel.
    fn jump_error(&mut self, step: i32) {
        if self.errors.is_empty() {
            self.toast("Ошибок нет");
            return;
        }
        let current = self
            .tabs
            .get(self.active)
            .map(|t| t.cursor_line)
            .unwrap_or(1);
        let mut lines: Vec<u32> = self.errors.iter().filter_map(|e| e.line).collect();
        if lines.is_empty() {
            self.toast("У ошибок нет номера строки");
            return;
        }
        lines.sort_unstable();
        let target = if step > 0 {
            lines
                .iter()
                .copied()
                .find(|l| *l > current)
                .unwrap_or(lines[0])
        } else {
            lines
                .iter()
                .copied()
                .rev()
                .find(|l| *l < current)
                .unwrap_or(*lines.last().unwrap())
        };
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.jump_to_line = Some(target);
        }
        self.bottom_tab = BottomTab::Errors;
        self.status = format!("Ошибка: строка {target}");
    }

    fn find_references(&mut self) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let (_, word) = completion::word_start(&tab.text, tab.sel.0);
        let word = word.trim().to_string();
        if word.is_empty() {
            self.status = "Поставьте курсор на имя".to_string();
            return;
        }
        let mut refs = Vec::new();
        for (i, line) in tab.text.lines().enumerate() {
            if line_matches_word(line, &word) {
                refs.push((i as u32 + 1, line.trim().to_string()));
            }
        }
        self.status = if refs.is_empty() {
            format!("«{word}» не встречается")
        } else {
            format!("Упоминаний «{word}»: {}", refs.len())
        };
        self.references = refs;
        self.bottom_tab = BottomTab::References;
        self.show_bottom = true;
    }

    fn open_rename(&mut self) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let (_, word) = completion::word_start(&tab.text, tab.sel.0);
        let word = word.trim().to_string();
        if word.is_empty() {
            self.status = "Поставьте курсор на имя".to_string();
            return;
        }
        self.rename = Some(RenameState {
            from: word.clone(),
            to: word,
        });
    }

    /// Jump to the declaration of the identifier under the cursor.
    fn go_to_definition(&mut self, _ctx: &egui::Context) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let (_, word) = completion::word_start(&tab.text, tab.sel.0);
        let word = word.trim().to_string();
        if word.is_empty() {
            self.status = "Поставьте курсор на имя алгоритма или переменной".to_string();
            return;
        }
        let normalized = kumir_core::ast::normalize_name(&word).to_lowercase();
        let mut target = None;
        if let Some(program) = &tab.outline {
            if let Some(alg) = program
                .algorithms
                .iter()
                .find(|a| kumir_core::ast::normalize_name(&a.name).to_lowercase() == normalized)
            {
                target = Some((alg.line, format!("алгоритм «{}»", alg.name)));
            } else if let Some(g) = program
                .globals
                .iter()
                .find(|g| kumir_core::ast::normalize_name(&g.name).to_lowercase() == normalized)
            {
                target = Some((g.line, format!("глобальная величина «{}»", g.name)));
            }
        }
        if target.is_none() {
            // A local declaration: look for a line that starts with a type and
            // contains the word as a whole token.
            for (i, line) in tab.text.lines().enumerate() {
                let trimmed = line.trim_start();
                let is_decl = [
                    "цел",
                    "вещ",
                    "лит",
                    "сим",
                    "лог",
                    "файл",
                    "целтаб",
                    "вещтаб",
                    "литтаб",
                    "симтаб",
                    "логтаб",
                ]
                .iter()
                .any(|t| trimmed.starts_with(t));
                if is_decl && line_matches_word(line, &word) {
                    target = Some((i as u32 + 1, format!("объявление «{word}»")));
                    break;
                }
            }
        }
        if let Some((line, what)) = target {
            if let Some(tab) = self.tabs.get_mut(self.active) {
                tab.jump_to_line = Some(line);
            }
            self.status = format!("Переход: {what}, строка {line}");
            return;
        }
        let is_builtin = kumir_core::actors::commands::is_known(&word)
            || kumir_core::builtins::known_names()
                .iter()
                .any(|n| n.eq_ignore_ascii_case(&word));
        if is_builtin {
            self.help_focus = Some(word.clone());
            self.show_help = true;
            self.status = format!("Справка: {word}");
            return;
        }
        self.status = format!("Не найдено определение «{word}»");
    }

    // ------------------------------------------------------------------
    // Actions and shortcuts
    // ------------------------------------------------------------------

    fn handle_pinch_zoom(&mut self, ctx: &egui::Context) {
        let Some(mt) = ctx.input(|i| i.multi_touch()) else {
            self.zoom_accum = 0.0;
            self.pinch_field = false;
            return;
        };
        // Щипок над полем исполнителя масштабирует только поле (как колесо
        // на ПК), а не весь интерфейс.
        if self
            .field_rect
            .map(|r| r.contains(mt.center_pos))
            .unwrap_or(false)
        {
            self.pinch_field = true;
        }
        if self.pinch_field {
            self.zoom_accum = 0.0;
            return;
        }
        let delta = mt.zoom_delta - 1.0;
        if delta.abs() < 0.0008 {
            return;
        }
        // Жест приглушается и применяется только накопленное изменение,
        // чтобы пальцы дрожали — масштаб не дёргался.
        self.zoom_accum = (self.zoom_accum + delta * 0.35).clamp(-0.14, 0.14);
        if self.zoom_accum.abs() < 0.05 {
            return;
        }
        let factor = 1.0 + self.zoom_accum;
        self.zoom_accum = 0.0;

        // Щипок = общий зум всего интерфейса (как слайдер «Масштаб интерфейса»).
        let ppt = ctx.pixels_per_point();
        let next = (ppt * factor).clamp(0.55, 5.0);
        if (next - ppt).abs() > f32::EPSILON {
            ctx.set_pixels_per_point(next);
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let mut actions = Vec::new();
        ctx.input_mut(|i| {
            if i.consume_key(Modifiers::CTRL, Key::N) {
                actions.push(Action::New);
            }
            if i.consume_key(Modifiers::CTRL, Key::O) {
                actions.push(Action::OpenDialog);
            }
            if i.consume_key(Modifiers::CTRL, Key::S) {
                actions.push(Action::Save);
            }
            if i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::S) {
                actions.push(Action::SaveAs);
            }
            if i.consume_key(Modifiers::CTRL, Key::W) {
                actions.push(Action::CloseTab);
            }
            if i.consume_key(Modifiers::CTRL, Key::Q) {
                actions.push(Action::Quit);
            }
            if i.consume_key(Modifiers::CTRL, Key::F) {
                actions.push(Action::Find);
            }
            if i.consume_key(Modifiers::CTRL, Key::B) {
                actions.push(Action::Check);
            }
            if i.consume_key(Modifiers::NONE, Key::F5) {
                actions.push(Action::Run);
            }
            if i.consume_key(Modifiers::SHIFT, Key::F5) {
                actions.push(Action::Stop);
            }
            if i.consume_key(Modifiers::NONE, Key::F6) {
                actions.push(Action::Pause);
            }
            if i.consume_key(Modifiers::NONE, Key::F10) {
                actions.push(Action::Step);
            }
            if i.consume_key(Modifiers::NONE, Key::F9) {
                actions.push(Action::ToggleBreakpoint);
            }
            if i.consume_key(Modifiers::NONE, Key::F7) {
                actions.push(Action::ShowActor);
            }
            if i.consume_key(Modifiers::NONE, Key::F1) {
                actions.push(Action::ShowHelp);
            }
            if i.consume_key(Modifiers::NONE, Key::F12) {
                actions.push(Action::GoToDefinition);
            }
            if i.consume_key(Modifiers::NONE, Key::F8) {
                actions.push(Action::NextError);
            }
            if i.consume_key(Modifiers::SHIFT, Key::F8) {
                actions.push(Action::PrevError);
            }
            if i.consume_key(Modifiers::CTRL, Key::G) {
                actions.push(Action::GoToLine);
            }
            if i.consume_key(Modifiers::CTRL, Key::P) {
                actions.push(Action::QuickOpen);
            }
            if i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::I) {
                actions.push(Action::FormatDocument);
            }
            if i.consume_key(Modifiers::CTRL, Key::Equals)
                || i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::Plus)
            {
                actions.push(Action::ZoomIn);
            }
            if i.consume_key(Modifiers::CTRL, Key::Minus) {
                actions.push(Action::ZoomOut);
            }
            if i.consume_key(Modifiers::NONE, Key::Escape) && self.run.is_some() {
                actions.push(Action::Stop);
            }
        });
        self.actions.extend(actions);
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        for p in dropped {
            match p.extension().and_then(|e| e.to_str()) {
                Some("kum") => self.open_path(&p),
                Some("fil") => self.load_field_path(&p),
                _ => {}
            }
        }
    }

    fn draw_toast(&mut self, ctx: &egui::Context) {
        let Some((text, when)) = self.toast.clone() else {
            return;
        };
        if when.elapsed() > Duration::from_secs(5) {
            self.toast = None;
            return;
        }
        let palette = self.palette;
        egui::Area::new(Id::unique("kumir_toast"))
            .order(egui::Order::Foreground)
            .anchor(Align2::RIGHT_BOTTOM, Vec2::new(-16.0, -46.0))
            .show(ctx, |ui| {
                Frame::NONE
                    .fill(palette.panel_alt)
                    .stroke(Stroke::new(1.0, palette.accent))
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.label(RichText::new(text).color(palette.text));
                    });
            });
        ctx.request_repaint_after(Duration::from_millis(250));
    }

    fn apply_actions(&mut self, ctx: &egui::Context) {
        let actions = std::mem::take(&mut self.actions);
        for action in actions {
            match action {
                Action::New => self.new_tab(),
                Action::OpenDialog => self.open_dialog(Mode::Open),
                Action::OpenPath(p) => self.open_path(&p),
                Action::LoadField(p) => self.load_field_path(&p),
                Action::Save => self.save_active(false),
                Action::SaveAs => self.save_active(true),
                Action::CloseTab => self.close_tab(self.active),
                Action::Quit => {
                    self.persist_settings();
                    if self.tabs.iter().any(|t| t.dirty) {
                        self.quit_request = true;
                    } else {
                        self.allow_close = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
                Action::Run => self.run_active(ctx, false),
                Action::Evaluate => self.run_active(ctx, true),
                Action::Stop => self.stop_run(),
                Action::Pause => {
                    if let Some(run) = &self.run {
                        if run.shared.is_paused() {
                            run.shared.resume();
                            self.status = "Выполняется…".to_string();
                        } else {
                            run.shared.request_pause();
                            self.status = "Пауза".to_string();
                        }
                    }
                }
                Action::Step => {
                    if let Some(run) = &self.run {
                        run.shared.step();
                        self.status = "Шаг".to_string();
                    }
                }
                Action::Check => self.check_active(),
                Action::ToggleBreakpoint => {
                    let line = self
                        .tabs
                        .get(self.active)
                        .map(|t| t.cursor_line)
                        .unwrap_or(1);
                    if !self.breakpoints.remove(&line) {
                        self.breakpoints.insert(line);
                    }
                    if let Some(run) = &self.run {
                        run.shared.set_breakpoints(self.breakpoints.clone());
                    }
                }
                Action::Find => {
                    if let Some(tab) = self.tabs.get_mut(self.active) {
                        tab.find.open = !tab.find.open;
                    }
                }
                Action::Undo => {
                    if let Some(tab) = self.tabs.get_mut(self.active) {
                        tab.undo();
                    }
                }
                Action::Redo => {
                    if let Some(tab) = self.tabs.get_mut(self.active) {
                        tab.redo();
                    }
                }
                Action::Copy | Action::Cut => {
                    if let Some(tab) = self.tabs.get_mut(self.active) {
                        tab.pending_focus = true;
                    }
                    let event = if matches!(action, Action::Copy) {
                        egui::Event::Copy
                    } else {
                        egui::Event::Cut
                    };
                    ctx.input_mut(|i| i.events.push(event));
                }
                Action::Paste => {
                    if let Some(tab) = self.tabs.get_mut(self.active) {
                        tab.pending_focus = true;
                    }
                    if let Some(text) = clipboard_get() {
                        ctx.input_mut(|i| i.events.push(egui::Event::Paste(text)));
                    }
                }
                Action::SelectAll => {
                    if let Some(tab) = self.tabs.get_mut(self.active) {
                        let chars = tab.text.chars().count();
                        tab.pending_select = Some((0, chars));
                    }
                }
                Action::GoToDefinition => self.go_to_definition(ctx),
                Action::GoToLine => self.goto_line = Some(String::new()),
                Action::QuickOpen => self.quick_open = Some(String::new()),
                Action::ToggleComment => {
                    if let Some(tab) = self.tabs.get_mut(self.active) {
                        tab.toggle_comment(self.settings.tab_width);
                    }
                }
                Action::FormatDocument => {
                    if let Some(tab) = self.tabs.get_mut(self.active) {
                        tab.format_document(self.settings.tab_width);
                    }
                }
                Action::SetScheme(scheme) => {
                    self.set_scheme(ctx, scheme);
                    self.persist_settings();
                }
                Action::ShowShortcuts => self.show_shortcuts = true,
                Action::NextError => self.jump_error(1),
                Action::PrevError => self.jump_error(-1),
                Action::ZoomIn => {
                    self.font_size = (self.font_size + 1.0).min(26.0);
                    theme::apply_style(ctx, &self.palette, self.font_size, is_touch());
                }
                Action::ZoomOut => {
                    self.font_size = (self.font_size - 1.0).max(9.0);
                    theme::apply_style(ctx, &self.palette, self.font_size, is_touch());
                }
                Action::ToggleLeft => self.show_left = !self.show_left,
                Action::ToggleRight => self.show_right = !self.show_right,
                Action::ToggleBottom => self.show_bottom = !self.show_bottom,
                Action::ShowHelp => {
                    if let Some(tab) = self.tabs.get(self.active) {
                        let (_, word) = completion::word_start(&tab.text, tab.sel.0);
                        if !word.trim().is_empty() {
                            self.help_focus = Some(word);
                        }
                    }
                    self.show_help = true;
                }
                Action::ShowAbout => self.show_about = true,
                Action::ShowSettings => self.show_settings = true,
                Action::ShowActor => {
                    self.show_right = true;
                    self.right_tab = RightTab::Actor;
                }
                Action::OpenField => self.open_field_dialog(),
            }
        }
    }

    // ------------------------------------------------------------------
    // Top panels
    // ------------------------------------------------------------------

    fn top_panels(&mut self, ui: &mut Ui) {
        let palette = self.palette;
        let touch = is_touch();
        let mut actions = Vec::new();
        egui::Panel::top("menu")
            .frame(
                Frame::NONE
                    .fill(palette.toolbar)
                    .inner_margin(Margin::symmetric(8, 2))
                    .stroke(Stroke::new(1.0, palette.border)),
            )
            .show(ui, |ui| {
                egui::MenuBar::new().ui(ui, |ui| {
                    ui.menu_button("Файл", |ui| {
                        if ui.button("Новый\tCtrl+N").clicked() {
                            actions.push(Action::New);
                            ui.close();
                        }
                        if ui.button("Открыть…\tCtrl+O").clicked() {
                            actions.push(Action::OpenDialog);
                            ui.close();
                        }
                        if ui.button("Сохранить\tCtrl+S").clicked() {
                            actions.push(Action::Save);
                            ui.close();
                        }
                        if ui.button("Сохранить как…\tCtrl+Shift+S").clicked() {
                            actions.push(Action::SaveAs);
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Закрыть вкладку\tCtrl+W").clicked() {
                            actions.push(Action::CloseTab);
                            ui.close();
                        }
                        if ui.button("Выход\tCtrl+Q").clicked() {
                            actions.push(Action::Quit);
                            ui.close();
                        }
                    });
                    ui.menu_button("Правка", |ui| {
                        if ui.button("Отменить\tCtrl+Z").clicked() {
                            actions.push(Action::Undo);
                            ui.close();
                        }
                        if ui.button("Повторить\tCtrl+Y").clicked() {
                            actions.push(Action::Redo);
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Вырезать\tCtrl+X").clicked() {
                            actions.push(Action::Cut);
                            ui.close();
                        }
                        if ui.button("Копировать\tCtrl+C").clicked() {
                            actions.push(Action::Copy);
                            ui.close();
                        }
                        if ui.button("Вставить\tCtrl+V").clicked() {
                            actions.push(Action::Paste);
                            ui.close();
                        }
                        if ui.button("Выделить всё\tCtrl+A").clicked() {
                            actions.push(Action::SelectAll);
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Найти и заменить…\tCtrl+F").clicked() {
                            actions.push(Action::Find);
                            ui.close();
                        }
                        if ui.button("Перейти к определению\tF12").clicked() {
                            actions.push(Action::GoToDefinition);
                            ui.close();
                        }
                        if ui.button("Перейти к строке…\tCtrl+G").clicked() {
                            actions.push(Action::GoToLine);
                            ui.close();
                        }
                        if ui.button("Закомментировать\tCtrl+/").clicked() {
                            actions.push(Action::ToggleComment);
                            ui.close();
                        }
                        if ui.button("Форматировать отступы\tCtrl+Shift+I").clicked()
                        {
                            actions.push(Action::FormatDocument);
                            ui.close();
                        }
                    });
                    ui.menu_button("Программа", |ui| {
                        if ui.button("Запустить\tF5").clicked() {
                            actions.push(Action::Run);
                            ui.close();
                        }
                        if ui.button("Остановить\tShift+F5").clicked() {
                            actions.push(Action::Stop);
                            ui.close();
                        }
                        if ui.button("Пауза / продолжить\tF6").clicked() {
                            actions.push(Action::Pause);
                            ui.close();
                        }
                        if ui.button("Шаг\tF10").clicked() {
                            actions.push(Action::Step);
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Проверить синтаксис\tCtrl+B").clicked() {
                            actions.push(Action::Check);
                            ui.close();
                        }
                        if ui
                            .button("Оценить задание (скрытый @тестирование)")
                            .clicked()
                        {
                            actions.push(Action::Evaluate);
                            ui.close();
                        }
                        if ui.button("Точка останова\tF9").clicked() {
                            actions.push(Action::ToggleBreakpoint);
                            ui.close();
                        }
                    });
                    ui.menu_button("Вид", |ui| {
                        ui.label(RichText::new("Цветовая схема").small().color(palette.muted));
                        for scheme in Scheme::ALL {
                            if ui
                                .selectable_label(self.settings.scheme == scheme, scheme.label())
                                .clicked()
                            {
                                actions.push(Action::SetScheme(scheme));
                                ui.close();
                            }
                        }
                        ui.separator();
                        if ui.button("Проводник").clicked() {
                            actions.push(Action::ToggleLeft);
                            ui.close();
                        }
                        if ui.button("Структура и исполнитель").clicked() {
                            actions.push(Action::ToggleRight);
                            ui.close();
                        }
                        if ui.button("Панель вывода").clicked() {
                            actions.push(Action::ToggleBottom);
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Увеличить шрифт\tCtrl++").clicked() {
                            actions.push(Action::ZoomIn);
                            ui.close();
                        }
                        if ui.button("Уменьшить шрифт\tCtrl+-").clicked() {
                            actions.push(Action::ZoomOut);
                            ui.close();
                        }
                        if ui.button("Настройки…").clicked() {
                            actions.push(Action::ShowSettings);
                            ui.close();
                        }
                    });
                    ui.menu_button("Исполнитель", |ui| {
                        if ui.button("Показать исполнителя\tF7").clicked() {
                            actions.push(Action::ShowActor);
                            ui.close();
                        }
                        if ui.button("Открыть поле Робота (.fil)…").clicked() {
                            actions.push(Action::OpenField);
                            ui.close();
                        }
                    });
                    ui.menu_button("Справка", |ui| {
                        if ui.button("Открыть справку\tF1").clicked() {
                            actions.push(Action::ShowHelp);
                            ui.close();
                        }
                        if ui.button("Горячие клавиши").clicked() {
                            actions.push(Action::ShowShortcuts);
                            ui.close();
                        }
                        if ui.button("О программе").clicked() {
                            actions.push(Action::ShowAbout);
                            ui.close();
                        }
                    });
                });
            });

        egui::Panel::top("toolbar")
            .frame(
                Frame::NONE
                    .fill(palette.panel)
                    .inner_margin(Margin::symmetric(8, if touch { 5 } else { 3 }))
                    .stroke(Stroke::new(1.0, palette.border)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui::icon_button(
                        ui,
                        &palette,
                        Icon::File,
                        false,
                        true,
                        "Новая программа (Ctrl+N)",
                    )
                    .clicked()
                    {
                        actions.push(Action::New);
                    }
                    if ui::icon_button(ui, &palette, Icon::Open, false, true, "Открыть (Ctrl+O)")
                        .clicked()
                    {
                        actions.push(Action::OpenDialog);
                    }
                    if ui::icon_button(ui, &palette, Icon::Save, false, true, "Сохранить (Ctrl+S)")
                        .clicked()
                    {
                        actions.push(Action::Save);
                    }
                    ui.separator();
                    let can_undo = self
                        .active_tab()
                        .map(|t| !t.undo.is_empty())
                        .unwrap_or(false);
                    let can_redo = self
                        .active_tab()
                        .map(|t| !t.redo.is_empty())
                        .unwrap_or(false);
                    if ui::icon_button(
                        ui,
                        &palette,
                        Icon::Undo,
                        false,
                        can_undo,
                        "Отменить (Ctrl+Z)",
                    )
                    .clicked()
                    {
                        actions.push(Action::Undo);
                    }
                    if ui::icon_button(
                        ui,
                        &palette,
                        Icon::Redo,
                        false,
                        can_redo,
                        "Повторить (Ctrl+Y)",
                    )
                    .clicked()
                    {
                        actions.push(Action::Redo);
                    }
                    ui.separator();
                    let running = self.run.is_some();
                    if ui::filled_icon_button(
                        ui,
                        Icon::Run,
                        palette.ok,
                        Color32::from_rgb(16, 32, 16),
                        !running,
                        "Запустить программу (F5)",
                    )
                    .clicked()
                    {
                        actions.push(Action::Run);
                    }
                    if ui::filled_icon_button(
                        ui,
                        Icon::Stop,
                        palette.error,
                        Color32::WHITE,
                        running,
                        "Остановить (Shift+F5)",
                    )
                    .clicked()
                    {
                        actions.push(Action::Stop);
                    }
                    let paused = self
                        .run
                        .as_ref()
                        .map(|r| r.shared.is_paused())
                        .unwrap_or(false);
                    let pause_icon = if paused { Icon::Run } else { Icon::Pause };
                    if ui::icon_button(
                        ui,
                        &palette,
                        pause_icon,
                        paused,
                        running,
                        if paused {
                            "Продолжить (F6)"
                        } else {
                            "Пауза (F6)"
                        },
                    )
                    .clicked()
                    {
                        actions.push(Action::Pause);
                    }
                    if ui::icon_button(ui, &palette, Icon::Step, false, running, "Шаг (F10)")
                        .clicked()
                    {
                        actions.push(Action::Step);
                    }
                    ui.separator();
                    if ui::icon_button(
                        ui,
                        &palette,
                        Icon::Check,
                        false,
                        true,
                        "Проверить синтаксис (Ctrl+B)",
                    )
                    .clicked()
                    {
                        actions.push(Action::Check);
                    }
                    if ui::icon_button(
                        ui,
                        &palette,
                        Icon::Evaluate,
                        false,
                        true,
                        "Оценить задание (@тестирование)",
                    )
                    .clicked()
                    {
                        actions.push(Action::Evaluate);
                    }
                    if ui::icon_button(
                        ui,
                        &palette,
                        Icon::Breakpoint,
                        false,
                        true,
                        "Точка останова на текущей строке (F9)",
                    )
                    .clicked()
                    {
                        actions.push(Action::ToggleBreakpoint);
                    }
                    ui.separator();
                    if ui::icon_button(
                        ui,
                        &palette,
                        Icon::Search,
                        false,
                        true,
                        "Найти и заменить (Ctrl+F)",
                    )
                    .clicked()
                    {
                        actions.push(Action::Find);
                    }
                    if ui::icon_button(ui, &palette, Icon::Robot, false, true, "Исполнитель (F7)")
                        .clicked()
                    {
                        actions.push(Action::ShowActor);
                    }
                    if ui::icon_button(ui, &palette, Icon::Settings, false, true, "Настройки")
                        .clicked()
                    {
                        actions.push(Action::ShowSettings);
                    }
                    ui.separator();
                    if ui::icon_button(
                        ui,
                        &palette,
                        Icon::Folder,
                        self.show_left,
                        true,
                        "Проводник",
                    )
                    .clicked()
                    {
                        actions.push(Action::ToggleLeft);
                    }
                    if ui::icon_button(
                        ui,
                        &palette,
                        Icon::Code,
                        self.show_right,
                        true,
                        "Структура и исполнитель",
                    )
                    .clicked()
                    {
                        actions.push(Action::ToggleRight);
                    }
                    if ui::icon_button(
                        ui,
                        &palette,
                        Icon::ChevronDown,
                        self.show_bottom,
                        true,
                        "Панель вывода",
                    )
                    .clicked()
                    {
                        actions.push(Action::ToggleBottom);
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.awaiting_input
                            && ui::filled_icon_button(
                                ui,
                                Icon::Keyboard,
                                palette.warning,
                                palette.bg,
                                true,
                                "Программа ждёт ввода — открыть панель «Вывод»",
                            )
                            .clicked()
                        {
                            self.show_bottom = true;
                            self.bottom_tab = BottomTab::Output;
                            self.input_focus = true;
                        }
                        let (state, color) = if let Some(run) = &self.run {
                            if run.shared.is_paused() {
                                ("Пауза", palette.warning)
                            } else {
                                ("Выполняется", palette.ok)
                            }
                        } else {
                            ("Готово", palette.muted)
                        };
                        ui.label(RichText::new(state).small().strong().color(color));
                        if let Some(tab) = self.active_tab() {
                            ui.label(
                                RichText::new(format!(
                                    "{}  •  строка {}",
                                    tab.title, tab.cursor_line
                                ))
                                .small()
                                .color(palette.muted),
                            );
                        }
                    });
                });
            });
        self.actions.extend(actions);
    }

    // ------------------------------------------------------------------
    // Left panel: project explorer
    // ------------------------------------------------------------------

    fn refresh_explorer(&mut self) {
        self.explorer_entries.clear();
        if let Ok(rd) = std::fs::read_dir(&self.explorer_dir) {
            let mut dirs = Vec::new();
            let mut files = Vec::new();
            for e in rd.flatten() {
                let p = e.path();
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                if p.is_dir() {
                    dirs.push(p);
                } else if p
                    .extension()
                    .and_then(|x| x.to_str())
                    .map(|x| {
                        x.eq_ignore_ascii_case("kum")
                            || x.eq_ignore_ascii_case("kod")
                            || x.eq_ignore_ascii_case("fil")
                    })
                    .unwrap_or(false)
                {
                    files.push(p);
                }
            }
            dirs.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
            files.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
            self.explorer_entries = dirs;
            self.explorer_entries.extend(files);
        }
        self.explorer_dirty = false;
    }

    fn left_panel(&mut self, ui: &mut Ui) {
        if !self.show_left {
            return;
        }
        let palette = self.palette;
        let mut actions = Vec::new();
        if self.explorer_dirty {
            self.refresh_explorer();
        }
        egui::Panel::left("explorer")
            .resizable(true)
            .default_size(235.0)
            .min_size(160.0)
            .frame(ui::panel_frame(&palette))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui::section_header(ui, &palette, "Проводник");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui::icon_button(ui, &palette, Icon::Refresh, false, true, "Обновить")
                            .clicked()
                        {
                            self.explorer_dirty = true;
                        }
                        if ui::icon_button(ui, &palette, Icon::FolderUp, false, true, "Выше")
                            .clicked()
                        {
                            if let Some(p) = self.explorer_dir.parent() {
                                self.explorer_dir = p.to_path_buf();
                                self.explorer_dirty = true;
                            }
                        }
                        if ui::icon_button(ui, &palette, Icon::Plus, false, true, "Новая программа")
                            .clicked()
                        {
                            actions.push(Action::New);
                        }
                    });
                });
                ui.label(
                    RichText::new(self.explorer_dir.to_string_lossy())
                        .small()
                        .color(palette.muted),
                );
                ui.add_space(2.0);
                let list_height = (ui.available_height() - 150.0).max(80.0);
                ScrollArea::vertical()
                    .id_salt("explorer_list")
                    .auto_shrink([false, false])
                    .max_height(list_height)
                    .show(ui, |ui| {
                        let entries = self.explorer_entries.clone();
                        for path in entries {
                            let is_dir = path.is_dir();
                            let name = path
                                .file_name()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_default();
                            let ext = path
                                .extension()
                                .and_then(|e| e.to_str())
                                .unwrap_or_default()
                                .to_ascii_lowercase();
                            let icon = if is_dir {
                                Icon::Folder
                            } else if ext == "kod" {
                                Icon::FileKod
                            } else if ext == "fil" {
                                Icon::FileFil
                            } else {
                                Icon::File
                            };
                            let selected = self
                                .active_tab()
                                .and_then(|t| t.path.as_ref())
                                .map(|p| p == &path)
                                .unwrap_or(false);
                            let resp =
                                ui::list_row(ui, &palette, icon, &name, selected, ext == "kod");
                            if resp.clicked() {
                                if is_dir {
                                    self.explorer_dir = path.clone();
                                    self.explorer_dirty = true;
                                } else if ext == "kum" {
                                    actions.push(Action::OpenPath(path.clone()));
                                } else if ext == "fil" {
                                    actions.push(Action::LoadField(path.clone()));
                                }
                            }
                            if resp.double_clicked() && is_dir {
                                self.explorer_dir = path.clone();
                                self.explorer_dirty = true;
                            }
                        }
                        if self.explorer_entries.is_empty() {
                            ui::empty_state(
                                ui,
                                &palette,
                                Icon::FolderOpen,
                                "Пусто",
                                "В этой папке нет .kum, .fil или .kod",
                            );
                        }
                    });
                ui.separator();
                ui::section_header(ui, &palette, "Недавние");
                let recent = self.recent.clone();
                ScrollArea::vertical()
                    .id_salt("recent_list")
                    .auto_shrink([false, false])
                    .max_height(120.0)
                    .show(ui, |ui| {
                        if recent.is_empty() {
                            ui.label(RichText::new("Пока пусто").small().color(palette.muted));
                        }
                        for p in recent {
                            let name = p
                                .file_name()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_default();
                            if ui::list_row(ui, &palette, Icon::Recent, &name, false, true)
                                .on_hover_text(p.display().to_string())
                                .clicked()
                            {
                                actions.push(Action::OpenPath(p));
                            }
                        }
                    });
            });
        self.actions.extend(actions);
    }

    // ------------------------------------------------------------------
    // Right panel: program structure
    // ------------------------------------------------------------------

    fn refresh_outline(&mut self) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        if !tab.outline_dirty {
            return;
        }
        let src = tab.compilation_source();
        let dir = tab.source_dir(&self.explorer_dir);
        match parse_source(&src, tab.title.clone(), Some(&dir)) {
            Ok(program) => {
                tab.outline = Some(program);
                tab.outline_errors.clear();
            }
            Err(errors) => {
                tab.outline = None;
                tab.outline_errors = errors;
            }
        }
        tab.outline_dirty = false;
    }

    fn right_panel(&mut self, ui: &mut Ui) {
        if !self.show_right {
            return;
        }
        self.refresh_outline();
        let palette = self.palette;
        let mut jump: Option<u32> = None;
        let mut remove_bp: Option<u32> = None;
        egui::Panel::right("structure")
            .resizable(true)
            .default_size(300.0)
            .min_size(200.0)
            .frame(ui::panel_frame(&palette))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui::tab(
                        ui,
                        &palette,
                        "Структура",
                        self.right_tab == RightTab::Structure,
                    )
                    .clicked()
                    {
                        self.right_tab = RightTab::Structure;
                    }
                    if ui::tab(
                        ui,
                        &palette,
                        "Исполнитель",
                        self.right_tab == RightTab::Actor,
                    )
                    .clicked()
                    {
                        self.right_tab = RightTab::Actor;
                    }
                });
                ui.separator();
                if self.right_tab == RightTab::Actor {
                    self.actor_panel(ui);
                    return;
                }
                ui::section_header(ui, &palette, "Структура программы");
                let structure_height = ui.available_height() * 0.55;
                ScrollArea::vertical()
                    .id_salt("structure_scroll")
                    .auto_shrink([false, false])
                    .max_height(structure_height)
                    .show(ui, |ui| {
                        let Some(tab) = self.tabs.get(self.active) else {
                            return;
                        };
                        if !tab.outline_errors.is_empty() {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!("Ошибок: {}", tab.outline_errors.len()))
                                        .small()
                                        .color(palette.error),
                                );
                                if ui.small_button("Открыть").clicked() {
                                    jump = Some(
                                        tab.outline_errors.first().map(|e| e.line).unwrap_or(1),
                                    );
                                }
                            });
                            for e in tab.outline_errors.iter().take(6) {
                                ui.label(
                                    RichText::new(format!("• {}", e.message))
                                        .small()
                                        .color(palette.error),
                                );
                            }
                            ui.separator();
                        }
                        if let Some(program) = &tab.outline {
                            if !program.modules.is_empty() {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(
                                        RichText::new("Используется:").small().color(palette.muted),
                                    );
                                    for module in &program.modules {
                                        ui::badge(ui, &palette, module, palette.panel_alt);
                                    }
                                });
                            }
                            ui::section_header(ui, &palette, "Алгоритмы");
                            for (i, alg) in program.algorithms.iter().enumerate() {
                                let is_main = Some(i) == program.main;
                                let name = if alg.name.is_empty() {
                                    "<основная программа>"
                                } else {
                                    &alg.name
                                };
                                let icon = if is_main {
                                    Icon::Star
                                } else if alg.name.starts_with('@') {
                                    Icon::Evaluate
                                } else {
                                    Icon::Code
                                };
                                if ui::list_row(
                                    ui,
                                    &palette,
                                    icon,
                                    name,
                                    false,
                                    !is_main && alg.name.starts_with('@'),
                                )
                                .on_hover_text(format!("Строка {}", alg.line))
                                .clicked()
                                {
                                    jump = Some(alg.line);
                                }
                            }
                            if !program.globals.is_empty() {
                                ui.add_space(4.0);
                                ui::section_header(ui, &palette, "Глобальные величины");
                                for g in &program.globals {
                                    let text = format!("{}: {}", g.name, type_name(&g.typ));
                                    ui.label(RichText::new(text).small().color(palette.muted));
                                }
                            }
                        } else if tab.outline_errors.is_empty() {
                            ui.label(RichText::new("Нет данных").small().color(palette.muted));
                        }
                    });
                if !self.variables.is_empty() {
                    ui.add_space(4.0);
                    ui.separator();
                    ui::section_header(ui, &palette, "Переменные");
                    ScrollArea::vertical()
                        .id_salt("vars_scroll")
                        .auto_shrink([false, false])
                        .max_height(150.0)
                        .show(ui, |ui| {
                            for (name, ty, value) in &self.variables {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(format!("{name}:"))
                                            .small()
                                            .color(palette.text),
                                    );
                                    ui.label(
                                        RichText::new(value)
                                            .small()
                                            .monospace()
                                            .color(palette.number),
                                    );
                                    if !ty.is_empty() {
                                        ui.label(RichText::new(ty).small().color(palette.muted));
                                    }
                                });
                            }
                        });
                }
                ui.add_space(4.0);
                ui.separator();
                ui::section_header(ui, &palette, "Точки останова");
                if self.breakpoints.is_empty() {
                    ui.label(
                        RichText::new("F9 — поставить точку на текущей строке")
                            .small()
                            .color(palette.muted),
                    );
                }
                let breakpoints: Vec<u32> = self.breakpoints.iter().copied().collect();
                for line in breakpoints {
                    ui.horizontal(|ui| {
                        if ui::icon_button(
                            ui,
                            &palette,
                            Icon::Close,
                            false,
                            true,
                            "Снять точку останова",
                        )
                        .clicked()
                        {
                            remove_bp = Some(line);
                        }
                        if ui::list_row(
                            ui,
                            &palette,
                            Icon::Breakpoint,
                            &format!("строка {line}"),
                            false,
                            false,
                        )
                        .clicked()
                        {
                            jump = Some(line);
                        }
                    });
                }
                ui.add_space(4.0);
                ui.separator();
                ui::section_header(ui, &palette, "Состояние");
                let (state, color) = if let Some(run) = &self.run {
                    if run.shared.is_paused() {
                        ("Пауза", palette.warning)
                    } else {
                        ("Выполняется", palette.ok)
                    }
                } else {
                    ("Остановлено", palette.muted)
                };
                ui.label(RichText::new(state).color(color));
                if self.run_line > 0 {
                    ui.label(
                        RichText::new(format!("Текущая строка: {}", self.run_line))
                            .small()
                            .color(palette.muted),
                    );
                }
                ui.label(
                    RichText::new(format!(
                        "{} строк • {} символов",
                        self.active_tab()
                            .map(|t| t.text.lines().count())
                            .unwrap_or(0),
                        self.active_tab()
                            .map(|t| t.text.chars().count())
                            .unwrap_or(0)
                    ))
                    .small()
                    .color(palette.muted),
                );
            });
        if let Some(line) = jump {
            if let Some(tab) = self.tabs.get_mut(self.active) {
                tab.jump_to_line = Some(line);
            }
        }
        if let Some(line) = remove_bp {
            self.breakpoints.remove(&line);
            if let Some(run) = &self.run {
                run.shared.set_breakpoints(self.breakpoints.clone());
            }
        }
    }

    // ------------------------------------------------------------------
    // Bottom panels: status bar and output
    // ------------------------------------------------------------------

    fn status_bar(&mut self, ui: &mut Ui) {
        let palette = self.palette;
        egui::Panel::bottom("status")
            .exact_size(26.0)
            .frame(
                Frame::NONE
                    .fill(palette.panel_alt)
                    .inner_margin(Margin::symmetric(10, 3))
                    .stroke(Stroke::new(1.0, palette.border)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&self.status).small().color(palette.muted));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if let Some(tab) = self.active_tab() {
                            let language = kumir_core::lexer::detect_language(&tab.text);
                            let lang = match language {
                                kumir_core::lexer::Language::Russian => "Кумир (рус.)",
                                kumir_core::lexer::Language::English => "Kumir (eng.)",
                            };
                            ui.label(
                                RichText::new(format!(
                                    "Стр {}:{}   |   UTF-8   |   {}   |   {}",
                                    tab.cursor_line, tab.cursor_col, lang, tab.title
                                ))
                                .small()
                                .color(palette.muted),
                            );
                            if tab.dirty {
                                ui.label(
                                    RichText::new("•").small().strong().color(palette.warning),
                                );
                            }
                        }
                    });
                });
            });
    }

    fn push_field_undo(&mut self, field: &RobotField) {
        self.field_undo.push(field.clone());
        if self.field_undo.len() > 64 {
            self.field_undo.remove(0);
        }
        self.field_redo.clear();
    }

    fn open_field_save_dialog(&mut self) {
        let start = self
            .field_path
            .as_ref()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| self.explorer_dir.clone());
        let name = self
            .field_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().to_string())
            .or_else(|| {
                self.active_tab()
                    .and_then(|t| t.path.as_ref())
                    .and_then(|p| p.file_stem())
                    .map(|s| format!("{}.fil", s.to_string_lossy()))
            })
            .unwrap_or_else(|| "field.fil".to_string());
        self.field_save_dialog = Some(FileDialog::with_extension(Mode::Save, start, name, "fil"));
    }

    fn open_output_save_dialog(&mut self) {
        let start = self.explorer_dir.clone();
        self.output_dialog = Some(FileDialog::with_extension(
            Mode::Save,
            start,
            "вывод.txt",
            "txt",
        ));
    }

    fn actor_panel(&mut self, ui: &mut Ui) {
        let palette = self.palette;
        let mut state = self
            .actor
            .lock()
            .ok()
            .map(|s| s.clone())
            .unwrap_or_default();
        let running = self.run.is_some();
        let paused = self
            .run
            .as_ref()
            .map(|r| r.shared.is_paused())
            .unwrap_or(false);
        let can_pult = !running || paused;

        if state.active.is_empty()
            || (self.actor_view == ActorKind::None && self.tab_actors().is_empty())
        {
            ui::empty_state(
                ui,
                &palette,
                Icon::Robot,
                "Исполнитель не подключён",
                "Добавьте строку «использовать Робот», «использовать Черепаха», \
                 «использовать Чертежник», «использовать Водолей», \
                 «использовать Кузнечик» или «использовать Рисователь».",
            );
            ui.vertical_centered(|ui| {
                if ui.button("Открыть поле Робота (.fil)…").clicked() {
                    self.open_field_dialog();
                }
            });
            return;
        }
        if !state.active.contains(&self.actor_view) {
            self.actor_view = state.active[0];
        }
        let view = self.actor_view;
        let is_robot = view == ActorKind::Robot;
        let edit_on = is_robot && self.actor_edit && !running;

        ui.horizontal_wrapped(|ui| {
            for kind in state.active.clone() {
                let icon = match kind {
                    ActorKind::Robot => Icon::Robot,
                    ActorKind::Turtle => Icon::Turtle,
                    ActorKind::Draw => Icon::Draw,
                    ActorKind::Vodoley => Icon::Vodoley,
                    ActorKind::Grasshopper => Icon::Grasshopper,
                    ActorKind::Painter => Icon::Painter,
                    ActorKind::None => Icon::Code,
                };
                let _ = icon;
                if ui::tab(ui, &palette, kind.russian_name(), view == kind).clicked() {
                    self.actor_view = kind;
                    self.painter_texture = None;
                }
            }
        });

        let mut new_size = self.field_size;
        let mut apply_size = false;
        let mut clear_colors = false;
        let mut clear_field = false;
        let mut reset_robot = false;
        let mut restore_initial = false;
        let mut open_field = false;
        let mut save_field = false;
        let mut undo_field = false;
        let mut redo_field = false;
        let mut insert_protocol = false;
        let mut actor_file: Option<ActorFileAction> = None;

        ui.horizontal_wrapped(|ui| {
            if ui::icon_button(
                ui,
                &palette,
                Icon::ZoomOut,
                false,
                true,
                "Уменьшить масштаб",
            )
            .clicked()
            {
                self.actor_zoom = (self.actor_zoom / 1.25).clamp(0.2, 10.0);
            }
            ui.label(
                RichText::new(format!("{:.0}%", self.actor_zoom * 100.0))
                    .small()
                    .color(palette.muted),
            );
            if ui::icon_button(ui, &palette, Icon::ZoomIn, false, true, "Увеличить масштаб")
                .clicked()
            {
                self.actor_zoom = (self.actor_zoom * 1.25).clamp(0.2, 10.0);
            }
            if ui::icon_button(
                ui,
                &palette,
                Icon::Fit,
                false,
                true,
                "Сбросить масштаб и сдвиг",
            )
            .clicked()
            {
                self.actor_zoom = 1.0;
                self.actor_pan = Vec2::ZERO;
            }
            ui.separator();
            ui.add_enabled(
                self.actor_animate,
                egui::DragValue::new(&mut self.actor_delay_ms)
                    .range(0..=150)
                    .suffix(" мс")
                    .speed(1.0),
            )
            .on_hover_text("Пауза после каждого шага исполнителя");
            if ui.checkbox(&mut self.actor_animate, "Анимация").changed() {
                self.settings.animate_actors = self.actor_animate;
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let icon = match view {
                    ActorKind::Robot => Icon::FileFil,
                    ActorKind::Vodoley => Icon::Vodoley,
                    ActorKind::Grasshopper => Icon::Grasshopper,
                    ActorKind::Painter => Icon::Painter,
                    _ => Icon::Download,
                };
                let ext = match view {
                    ActorKind::Robot => "fil",
                    ActorKind::Vodoley => "vod",
                    ActorKind::Grasshopper => "kz",
                    ActorKind::Painter => "png",
                    _ => "svg",
                };
                if ui::icon_button(
                    ui,
                    &palette,
                    icon,
                    false,
                    !running,
                    &format!("Сохранить .{ext}"),
                )
                .clicked()
                {
                    if view == ActorKind::Robot {
                        save_field = true;
                    } else {
                        actor_file = Some(match view {
                            ActorKind::Vodoley => ActorFileAction::SaveVod,
                            ActorKind::Grasshopper => ActorFileAction::SaveKz,
                            ActorKind::Painter => ActorFileAction::SavePng,
                            _ => ActorFileAction::SaveDrawing,
                        });
                    }
                }
                if ui::icon_button(
                    ui,
                    &palette,
                    Icon::Open,
                    false,
                    !running,
                    &format!("Открыть .{ext}"),
                )
                .clicked()
                {
                    if view == ActorKind::Robot {
                        open_field = true;
                    } else {
                        actor_file = Some(match view {
                            ActorKind::Vodoley => ActorFileAction::OpenVod,
                            ActorKind::Grasshopper => ActorFileAction::OpenKz,
                            ActorKind::Painter => ActorFileAction::OpenPng,
                            _ => ActorFileAction::OpenVod,
                        });
                    }
                }
                if ui::icon_button(
                    ui,
                    &palette,
                    Icon::Definition,
                    false,
                    !state.log.is_empty(),
                    "Вставить протокол пульта в программу",
                )
                .clicked()
                {
                    insert_protocol = true;
                }
            });
        });

        if is_robot {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Поле:").small().color(palette.muted));
                ui.add_enabled(
                    !running,
                    egui::DragValue::new(&mut new_size.0)
                        .range(1..=255)
                        .speed(0.2),
                )
                .on_hover_text("Ширина поля (столбцов), 1–255");
                ui.label(RichText::new("×").color(palette.muted));
                ui.add_enabled(
                    !running,
                    egui::DragValue::new(&mut new_size.1)
                        .range(1..=128)
                        .speed(0.2),
                )
                .on_hover_text("Высота поля (строк), 1–128");
                if ui
                    .add_enabled(!running, egui::Button::new("Применить"))
                    .on_hover_text("Создать пустое поле указанного размера")
                    .clicked()
                {
                    apply_size = true;
                }
                ui.separator();
                if ui
                    .add_enabled(
                        !running,
                        egui::Checkbox::new(&mut self.actor_edit, "Правка"),
                    )
                    .on_hover_text("ЛКМ — цвет/стена, Shift+ЛКМ или ПКМ — метка")
                    .changed()
                {
                    self.status = if self.actor_edit {
                        "Режим редактирования поля включён".to_string()
                    } else {
                        "Режим редактирования поля выключен".to_string()
                    };
                }
            });
            ui.horizontal_wrapped(|ui| {
                for (tool, icon, label, tip) in [
                    (
                        FieldTool::Color,
                        Icon::Brush,
                        "Цвет",
                        "ЛКМ — закрасить/очистить",
                    ),
                    (
                        FieldTool::Wall,
                        Icon::Wall,
                        "Стена",
                        "ЛКМ — поставить/убрать стену",
                    ),
                    (
                        FieldTool::Mark,
                        Icon::Mark,
                        "Метка",
                        "ЛКМ — поставить/убрать метку",
                    ),
                    (
                        FieldTool::Robot,
                        Icon::Robot,
                        "Робот",
                        "ЛКМ — поставить робота",
                    ),
                    (
                        FieldTool::Pan,
                        Icon::Hand,
                        "Сдвиг",
                        "Перетаскивание — сдвиг",
                    ),
                ] {
                    if ui::icon_button(
                        ui,
                        &palette,
                        icon,
                        self.field_tool == tool,
                        !running && self.actor_edit,
                        &format!("{label}: {tip}"),
                    )
                    .clicked()
                    {
                        self.field_tool = tool;
                    }
                }
                ui.separator();
                if ui::icon_button(
                    ui,
                    &palette,
                    Icon::Undo,
                    false,
                    !running && !self.field_undo.is_empty(),
                    "Отменить изменение поля",
                )
                .clicked()
                {
                    undo_field = true;
                }
                if ui::icon_button(
                    ui,
                    &palette,
                    Icon::Redo,
                    false,
                    !running && !self.field_redo.is_empty(),
                    "Повторить изменение поля",
                )
                .clicked()
                {
                    redo_field = true;
                }
                ui.menu_button("Поле", |ui| {
                    if ui
                        .add_enabled(!running, egui::Button::new("Снять закраску"))
                        .on_hover_text("Убрать только цвета, сохранив стены и метки")
                        .clicked()
                    {
                        clear_colors = true;
                        ui.close();
                    }
                    if ui
                        .add_enabled(!running, egui::Button::new("Очистить всё"))
                        .on_hover_text("Сбросить цвета, стены, метки и символы")
                        .clicked()
                    {
                        clear_field = true;
                        ui.close();
                    }
                    if ui
                        .add_enabled(!running, egui::Button::new("Робот на старт"))
                        .clicked()
                    {
                        reset_robot = true;
                        ui.close();
                    }
                    let restore_label = if self.field_before_run.is_some() {
                        "Вернуть к состоянию до запуска"
                    } else {
                        "Вернуть исходное поле"
                    };
                    if ui
                        .add_enabled(
                            !running
                                && (self.field_before_run.is_some()
                                    || self.field_initial.is_some()),
                            egui::Button::new(restore_label),
                        )
                        .clicked()
                    {
                        restore_initial = true;
                        ui.close();
                    }
                });
            });
        }

        let avail = ui.available_size();
        let canvas_height = (avail.y * 0.62).clamp(150.0, avail.y.max(150.0));
        let viewport = Vec2::new(avail.x.max(40.0), canvas_height);
        let (rect, response) = ui.allocate_exact_size(viewport, Sense::click_and_drag());
        self.field_rect = Some(rect);
        let mut zoom = self.actor_zoom;
        let mut pan = self.actor_pan;
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.01 {
                let old = zoom;
                let factor = (scroll * 0.0017).exp();
                zoom = (zoom * factor).clamp(0.2, 10.0);
                if let Some(p) = response.hover_pos() {
                    let rel = p - rect.center();
                    pan = rel * (1.0 - zoom / old) + pan * (zoom / old);
                }
            }
        }
        if self.pinch_field {
            if let Some(mt) = ui.input(|i| i.multi_touch()) {
                if (mt.zoom_delta - 1.0).abs() > 0.0005 {
                    let old = zoom;
                    zoom = (zoom * mt.zoom_delta).clamp(0.2, 10.0);
                    let rel = mt.center_pos - rect.center();
                    pan = rel * (1.0 - zoom / old) + pan * (zoom / old);
                }
            }
        }

        let pointer = response.interact_pointer_pos().or(response.hover_pos());
        let shift = ui.input(|i| i.modifiers.shift);
        let cell_under = if is_robot {
            pointer.and_then(|p| cell_at(rect, &state.robot, zoom, pan, p))
        } else {
            None
        };
        let robot_hit = is_robot
            && !running
            && cell_under
                .map(|(x, y)| x == state.robot.robot_x && y == state.robot.robot_y)
                .unwrap_or(false);
        let target = if edit_on {
            pointer
                .and_then(|p| edit_target(rect, &state.robot, zoom, pan, p, self.field_tool, shift))
        } else {
            None
        };

        if is_robot && !running && response.hovered() {
            if ui
                .ctx()
                .input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Z))
            {
                undo_field = true;
            }
            if ui
                .ctx()
                .input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Y))
                || ui
                    .ctx()
                    .input_mut(|i| i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::Z))
            {
                redo_field = true;
            }
        }
        if undo_field {
            if let Some(prev) = self.field_undo.pop() {
                self.field_redo.push(state.robot.clone());
                if self.field_redo.len() > 64 {
                    self.field_redo.remove(0);
                }
                state.robot = prev;
                self.field_size = (state.robot.width, state.robot.height);
                self.field_path = None;
                state.status = "Изменение поля отменено".to_string();
                self.field_last_target = None;
                self.field_dragging_robot = false;
            }
        }
        if redo_field {
            if let Some(next) = self.field_redo.pop() {
                self.field_undo.push(state.robot.clone());
                if self.field_undo.len() > 64 {
                    self.field_undo.remove(0);
                }
                state.robot = next;
                self.field_size = (state.robot.width, state.robot.height);
                self.field_path = None;
                state.status = "Изменение поля повторено".to_string();
                self.field_last_target = None;
                self.field_dragging_robot = false;
            }
        }

        let pan_mode = !edit_on || self.field_tool == FieldTool::Pan;
        let primary_pan = pan_mode
            && response.dragged_by(egui::PointerButton::Primary)
            && !self.field_dragging_robot
            && !(robot_hit && response.drag_started_by(egui::PointerButton::Primary));
        if primary_pan
            || response.dragged_by(egui::PointerButton::Middle)
            || response.dragged_by(egui::PointerButton::Secondary)
        {
            pan += response.drag_delta();
        }
        if response.double_clicked() && !edit_on && !robot_hit {
            zoom = 1.0;
            pan = Vec2::ZERO;
        }

        if is_robot {
            if response.drag_started_by(egui::PointerButton::Primary) {
                if robot_hit {
                    self.field_dragging_robot = true;
                    self.push_field_undo(&state.robot);
                } else if edit_on && self.field_tool != FieldTool::Pan && target.is_some() {
                    self.push_field_undo(&state.robot);
                    self.field_last_target = None;
                }
            }
            if response.dragged_by(egui::PointerButton::Primary) {
                if self.field_dragging_robot {
                    if let Some((x, y)) = cell_under {
                        state.robot.robot_x = x;
                        state.robot.robot_y = y;
                        state.robot.start_x = x;
                        state.robot.start_y = y;
                        state.status = format!("Робот перемещён в ({x}, {y})");
                        self.field_path = None;
                    }
                } else if edit_on && self.field_tool != FieldTool::Pan {
                    if let Some(t) = target {
                        if self.field_last_target != Some(t) {
                            let desc = apply_target(&mut state.robot, t);
                            self.field_last_target = Some(t);
                            self.field_path = None;
                            state.status = desc;
                        }
                    }
                }
            }
            if response.drag_stopped_by(egui::PointerButton::Primary) {
                self.field_dragging_robot = false;
                self.field_last_target = None;
            }
            if !response.dragged_by(egui::PointerButton::Primary)
                && !response.drag_started_by(egui::PointerButton::Primary)
            {
                self.field_dragging_robot = false;
                self.field_last_target = None;
            }

            if edit_on && response.clicked() {
                if let Some(t) = target {
                    self.push_field_undo(&state.robot);
                    state.status = apply_target(&mut state.robot, t);
                    self.field_path = None;
                    self.field_last_target = None;
                }
            }
            if edit_on && response.secondary_clicked() {
                if let Some(p) = pointer {
                    if let Some(t) =
                        edit_target(rect, &state.robot, zoom, pan, p, FieldTool::Mark, false)
                    {
                        self.push_field_undo(&state.robot);
                        state.status = apply_target(&mut state.robot, t);
                        self.field_path = None;
                    }
                }
            }
        }

        let mut ghost_target = target;
        if self.field_dragging_robot {
            if let Some((x, y)) = cell_under {
                ghost_target = Some(EditTarget {
                    x,
                    y,
                    kind: EditTargetKind::Robot,
                });
            }
        }

        if self.field_dragging_robot {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        } else if robot_hit {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        } else if edit_on && target.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        } else if pan_mode && response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }

        self.actor_zoom = zoom;
        self.actor_pan = pan;

        match view {
            ActorKind::Robot => draw_robot(ui, rect, &state.robot, &palette, zoom, pan),
            ActorKind::Turtle => draw_turtle(ui, rect, &state.turtle, &palette, zoom, pan),
            ActorKind::Draw => draw_vector_actor(
                ui,
                rect,
                &state.draw.segments,
                &state.draw.captions,
                (state.draw.x, state.draw.y),
                &palette,
                state.draw.pen_down,
                zoom,
                pan,
            ),
            ActorKind::Vodoley => draw_vodoley(ui, rect, &state.vodoley, &palette),
            ActorKind::Grasshopper => {
                draw_grasshopper(ui, rect, &state.grasshopper, &palette, zoom, pan)
            }
            ActorKind::Painter => draw_painter(
                ui,
                rect,
                &state.painter,
                &palette,
                zoom,
                pan,
                &mut self.painter_texture,
            ),
            ActorKind::None => {}
        }

        if is_robot && (edit_on || self.field_dragging_robot) {
            if let Some(t) = ghost_target {
                draw_edit_ghost(ui, rect, &state.robot, zoom, pan, t, &palette);
            }
        }
        if edit_on {
            let painter = ui.painter_at(rect);
            painter.rect_stroke(
                rect.shrink(1.0),
                CornerRadius::same(6),
                Stroke::new(2.0, palette.accent),
                egui::StrokeKind::Inside,
            );
            let badge = Rect::from_min_size(
                egui::pos2(rect.right() - 150.0, rect.top() + 8.0),
                Vec2::new(142.0, 20.0),
            );
            painter.rect_filled(badge, CornerRadius::same(4), palette.accent_weak);
            painter.text(
                badge.center(),
                Align2::CENTER_CENTER,
                "РЕДАКТИРОВАНИЕ ПОЛЯ",
                FontId::proportional(10.0),
                palette.accent,
            );
        }
        let hint = if is_robot && edit_on {
            "ЛКМ — рисовать · ПКМ — метка · колесо — масштаб"
        } else {
            "Колесо — масштаб, перетаскивание — сдвиг, двойной клик — сброс"
        };
        ui.painter_at(rect).text(
            rect.left_bottom() + Vec2::new(8.0, -6.0),
            Align2::LEFT_BOTTOM,
            hint,
            FontId::proportional(11.0),
            palette.muted,
        );

        let controls_height = (avail.y * 0.38).clamp(80.0, 320.0);
        ScrollArea::vertical()
            .id_salt("actor_controls")
            .max_height(controls_height)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                self.actor_controls(ui, &mut state, &palette, can_pult);
            });

        let status_text = if state.status.is_empty() {
            "—".to_string()
        } else {
            state.status.clone()
        };
        ui.add_sized(
            [ui.available_width(), 16.0],
            Label::new(RichText::new(&status_text).small().color(if edit_on {
                palette.accent
            } else {
                palette.muted
            }))
            .truncate(),
        )
        .on_hover_text(status_text);

        self.field_size = new_size;
        if apply_size {
            self.push_field_undo(&state.robot);
            self.field_path = None;
            state.robot = RobotField::empty(new_size.0, new_size.1);
            self.field_initial = Some(state.robot.clone());
            self.field_before_run = None;
            self.field_path_before_run = None;
            state.status = format!("Новое поле {}×{}", new_size.0, new_size.1);
            self.actor_zoom = 1.0;
            self.actor_pan = Vec2::ZERO;
            self.actor_edit = true;
        }
        if open_field {
            self.open_field_dialog();
        }
        if save_field {
            self.open_field_save_dialog();
        }
        if clear_colors {
            self.push_field_undo(&state.robot);
            for y in 0..state.robot.height {
                for x in 0..state.robot.width {
                    if let Some(c) = state.robot.cell_mut(x, y) {
                        c.color = false;
                    }
                }
            }
            self.field_path = None;
            state.status = "Закраска снята".to_string();
        }
        if reset_robot {
            self.push_field_undo(&state.robot);
            state.robot.reset();
            state.status = "Робот возвращён на стартовую клетку".to_string();
            self.field_path = None;
        }
        if restore_initial {
            let from_run = self.field_before_run.is_some();
            let snapshot = self
                .field_before_run
                .clone()
                .or_else(|| self.field_initial.clone());
            if let Some(field) = snapshot {
                self.push_field_undo(&state.robot);
                state.robot = field;
                self.field_size = (state.robot.width, state.robot.height);
                self.actor_zoom = 1.0;
                self.actor_pan = Vec2::ZERO;
                self.field_path = if from_run {
                    self.field_path_before_run.clone()
                } else {
                    None
                };
                state.status = "Поле восстановлено".to_string();
            }
        }
        if clear_field {
            self.push_field_undo(&state.robot);
            for y in 0..state.robot.height {
                for x in 0..state.robot.width {
                    if let Some(c) = state.robot.cell_mut(x, y) {
                        *c = kumir_core::actors::RobotCell::default();
                    }
                }
            }
            self.field_path = None;
            state.status = "Поле очищено".to_string();
        }
        if insert_protocol {
            self.insert_actor_protocol(&state);
        }
        if let Some(action) = actor_file {
            self.open_actor_file_dialog(action);
        }

        // While paused the interpreter is waiting, so pult commands can be
        // written back safely; during continuous execution the UI only reads.
        if !running || paused {
            if let Ok(mut a) = self.actor.lock() {
                *a = state;
            }
        }
    }

    fn insert_actor_protocol(&mut self, state: &ActorState) {
        if state.log.is_empty() {
            self.toast("Протокол пульта пуст");
            return;
        }
        let text: String = state.log.iter().map(|line| format!("{line}\n")).collect();
        if let Some(tab) = self.tabs.get_mut(self.active) {
            let (new_text, sel) = replace_selection(tab, &text);
            apply_edit(tab, new_text, sel);
            self.toast(format!("Вставлено команд: {}", state.log.len()));
        }
    }

    fn open_actor_file_dialog(&mut self, action: ActorFileAction) {
        let ext = match action {
            ActorFileAction::OpenVod | ActorFileAction::SaveVod => "vod",
            ActorFileAction::OpenKz | ActorFileAction::SaveKz => "kz",
            ActorFileAction::OpenPng | ActorFileAction::SavePng => "png",
            ActorFileAction::SaveDrawing => "svg",
        };
        let mode = match action {
            ActorFileAction::OpenVod | ActorFileAction::OpenKz | ActorFileAction::OpenPng => {
                Mode::Open
            }
            _ => Mode::Save,
        };
        let start = self
            .active_tab()
            .and_then(|t| t.path.as_ref())
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| self.explorer_dir.clone());
        let name = match action {
            ActorFileAction::SaveDrawing => format!("рисунок.{ext}"),
            ActorFileAction::SavePng | ActorFileAction::OpenPng => format!("лист.{ext}"),
            _ => format!("задание.{ext}"),
        };
        let dialog = FileDialog::with_extension(mode, start, name, ext);
        self.actor_dialog = Some((dialog, action));
    }

    fn actor_controls(
        &mut self,
        ui: &mut Ui,
        state: &mut ActorState,
        palette: &Palette,
        can_pult: bool,
    ) {
        if !can_pult {
            ui.label(
                RichText::new("Пульт доступен на паузе или после остановки программы.")
                    .small()
                    .color(palette.muted),
            );
            return;
        }
        let dir = self
            .active_tab()
            .map(|t| t.source_dir(&self.explorer_dir))
            .unwrap_or_else(|| self.explorer_dir.clone());
        let view = self.actor_view;
        ui.separator();
        match view {
            ActorKind::Robot => {
                ui.label(
                    RichText::new("Пульт Робота")
                        .small()
                        .strong()
                        .color(palette.muted),
                );
                ui.horizontal(|ui| {
                    if ui.button("Вверх").clicked() {
                        self.pult(state, "вверх", &[], &dir);
                    }
                    if ui.button("Вниз").clicked() {
                        self.pult(state, "вниз", &[], &dir);
                    }
                    if ui.button("Влево").clicked() {
                        self.pult(state, "влево", &[], &dir);
                    }
                    if ui.button("Вправо").clicked() {
                        self.pult(state, "вправо", &[], &dir);
                    }
                    if ui.button("Закрасить").clicked() {
                        self.pult(state, "закрасить", &[], &dir);
                    }
                });
            }
            ActorKind::Turtle => {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Расстояние:").small().color(palette.muted));
                    ui.add(
                        egui::DragValue::new(&mut self.pult_a)
                            .range(0.0..=500.0)
                            .speed(0.5),
                    );
                    if ui.button("Вперёд").clicked() {
                        let a = self.pult_a;
                        self.pult(state, "вперед", &[Value::Real(a)], &dir);
                    }
                    if ui.button("Назад").clicked() {
                        let a = self.pult_a;
                        self.pult(state, "назад", &[Value::Real(a)], &dir);
                    }
                    ui.separator();
                    ui.label(RichText::new("Угол:").small().color(palette.muted));
                    ui.add(
                        egui::DragValue::new(&mut self.pult_b)
                            .range(0.0..=360.0)
                            .speed(1.0),
                    );
                    if ui.button("Влево").clicked() {
                        let b = self.pult_b;
                        self.pult(state, "влево", &[Value::Real(b)], &dir);
                    }
                    if ui.button("Вправо").clicked() {
                        let b = self.pult_b;
                        self.pult(state, "вправо", &[Value::Real(b)], &dir);
                    }
                    ui.separator();
                    if ui.button("Поднять хвост").clicked() {
                        self.pult(state, "поднять хвост", &[], &dir);
                    }
                    if ui.button("Опустить хвост").clicked() {
                        self.pult(state, "опустить хвост", &[], &dir);
                    }
                });
            }
            ActorKind::Draw => {
                ui.horizontal_wrapped(|ui| {
                    ui.add(egui::DragValue::new(&mut self.pult_a).speed(1.0));
                    ui.add(egui::DragValue::new(&mut self.pult_b).speed(1.0));
                    if ui.button("В точку").clicked() {
                        let (a, b) = (self.pult_a, self.pult_b);
                        self.pult(
                            state,
                            "сместиться в точку",
                            &[Value::Real(a), Value::Real(b)],
                            &dir,
                        );
                    }
                    if ui.button("На вектор").clicked() {
                        let (a, b) = (self.pult_a, self.pult_b);
                        self.pult(
                            state,
                            "сместиться на вектор",
                            &[Value::Real(a), Value::Real(b)],
                            &dir,
                        );
                    }
                    ui.separator();
                    if ui.button("Опустить перо").clicked() {
                        self.pult(state, "опустить перо", &[], &dir);
                    }
                    if ui.button("Поднять перо").clicked() {
                        self.pult(state, "поднять перо", &[], &dir);
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.pult_text)
                            .desired_width(120.0)
                            .hint_text("текст"),
                    );
                    if ui.button("Написать").clicked() {
                        let text = self.pult_text.clone();
                        self.pult(
                            state,
                            "написать",
                            &[Value::Real(1.0), Value::Str(text)],
                            &dir,
                        );
                    }
                });
            }
            ActorKind::Vodoley => {
                let mut v = state.vodoley.clone();
                ui.horizontal_wrapped(|ui| {
                    for (label, name) in [
                        ("Наполни A", "наполни A"),
                        ("Наполни B", "наполни B"),
                        ("Наполни C", "наполни C"),
                        ("Вылей A", "вылей A"),
                        ("Вылей B", "вылей B"),
                        ("Вылей C", "вылей C"),
                    ] {
                        if ui.button(label).clicked() {
                            self.pult(state, name, &[], &dir);
                        }
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    for (label, name) in [
                        ("A → B", "перелей из A в B"),
                        ("A → C", "перелей из A в C"),
                        ("B → A", "перелей из B в A"),
                        ("B → C", "перелей из B в C"),
                        ("C → A", "перелей из C в A"),
                        ("C → B", "перелей из C в B"),
                    ] {
                        if ui.button(label).clicked() {
                            self.pult(state, name, &[], &dir);
                        }
                    }
                });
                ui.separator();
                ui::section_header(ui, palette, "Задание Водолея");
                ui.horizontal_wrapped(|ui| {
                    ui.label("Ёмкости:");
                    for i in 0..3 {
                        ui.add(egui::DragValue::new(&mut self.vod_edit[i]).range(0..=100));
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("Налито:");
                    for i in 0..3 {
                        ui.add(egui::DragValue::new(&mut self.vod_edit[3 + i]).range(0..=100));
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("Отмерить:");
                    ui.add(egui::DragValue::new(&mut self.vod_edit[6]).range(0..=100));
                    if ui.button("Применить задание").clicked() {
                        v.set_task(
                            [self.vod_edit[0], self.vod_edit[1], self.vod_edit[2]],
                            [self.vod_edit[3], self.vod_edit[4], self.vod_edit[5]],
                            self.vod_edit[6],
                        );
                        state.vodoley = v.clone();
                        self.toast("Задание Водолея применено");
                    }
                });
                let current = state.vodoley.clone();
                ui.label(
                    RichText::new(format!(
                        "Сейчас: A={} B={} C={} ({})",
                        current.fill[0],
                        current.fill[1],
                        current.fill[2],
                        if current.solved() {
                            "готово"
                        } else {
                            "цель не достигнута"
                        }
                    ))
                    .small()
                    .color(if current.solved() {
                        palette.ok
                    } else {
                        palette.muted
                    }),
                );
            }
            ActorKind::Grasshopper => {
                let mut g = state.grasshopper.clone();
                ui.horizontal_wrapped(|ui| {
                    if ui.button(format!("Вперёд {}", g.forward)).clicked() {
                        let step = g.forward;
                        self.pult(state, "вперед", &[Value::Int(step)], &dir);
                    }
                    if ui.button(format!("Назад {}", g.backward)).clicked() {
                        let step = g.backward;
                        self.pult(state, "назад", &[Value::Int(step)], &dir);
                    }
                    if ui.button("Перекрасить").clicked() {
                        self.pult(state, "перекрасить", &[], &dir);
                    }
                });
                ui.separator();
                ui::section_header(ui, palette, "Задание Кузнечика");
                ui.horizontal_wrapped(|ui| {
                    ui.label("Шаг вперёд:");
                    ui.add(egui::DragValue::new(&mut self.gh_edit.0).range(1..=100));
                    ui.label("назад:");
                    ui.add(egui::DragValue::new(&mut self.gh_edit.1).range(1..=100));
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("Старт:");
                    ui.add(egui::DragValue::new(&mut self.gh_edit.2).range(-1000..=1000));
                    ui.checkbox(&mut self.gh_edit.3, "Границы");
                    ui.add_enabled(
                        self.gh_edit.3,
                        egui::DragValue::new(&mut self.gh_edit.4).range(-1000..=1000),
                    );
                    ui.add_enabled(
                        self.gh_edit.3,
                        egui::DragValue::new(&mut self.gh_edit.5).range(-1000..=1000),
                    );
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("Флажки:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.gh_edit.6)
                            .desired_width(140.0)
                            .hint_text("3 7"),
                    );
                    if ui.button("Применить задание").clicked() {
                        let flags: Vec<i64> = self
                            .gh_edit
                            .6
                            .split(|c: char| c == ',' || c.is_whitespace())
                            .filter_map(|s| s.parse().ok())
                            .collect();
                        let borders = if self.gh_edit.3 {
                            Some((self.gh_edit.4, self.gh_edit.5))
                        } else {
                            None
                        };
                        g.set_task(
                            self.gh_edit.0,
                            self.gh_edit.1,
                            self.gh_edit.2,
                            borders,
                            flags,
                        );
                        state.grasshopper = g.clone();
                        self.toast("Задание Кузнечика применено");
                    }
                });
                let current = &state.grasshopper;
                ui.label(
                    RichText::new(format!(
                        "Позиция: {}{}",
                        current.pos,
                        if current.on_flag(current.pos) {
                            " (на флажке)"
                        } else {
                            ""
                        }
                    ))
                    .small()
                    .color(palette.muted),
                );
            }
            ActorKind::Painter => {
                ui.label(
                    RichText::new(format!(
                        "Лист {}×{}, перо {} px, кисть: {}",
                        state.painter.page.width,
                        state.painter.page.height,
                        state.painter.pen_width,
                        if state.painter.brush.is_some() {
                            "включена"
                        } else {
                            "выключена"
                        }
                    ))
                    .small()
                    .color(palette.muted),
                );
            }
            ActorKind::None => {}
        }

        if !state.log.is_empty() {
            ui.collapsing("Протокол пульта", |ui| {
                ScrollArea::vertical()
                    .id_salt("actor_protocol")
                    .max_height(90.0)
                    .show(ui, |ui| {
                        for line in state.log.iter().rev().take(40) {
                            ui.label(RichText::new(line).monospace().small());
                        }
                    });
            });
        }
    }

    fn pult(&mut self, state: &mut ActorState, name: &str, args: &[Value], dir: &Path) {
        match kumir_core::actors::lookup_for(name, &state.active, args.len()) {
            Some(spec) => match state.exec(spec, args, dir) {
                Ok(result) => {
                    if spec.is_query() {
                        state.status = format!("{name} = {}", value_text(&result.value));
                    }
                }
                Err(e) => {
                    state.status = e.clone();
                    self.toast(e);
                }
            },
            None => {
                state.status = format!("Команда «{name}» не найдена");
            }
        }
    }

    fn bottom_panel(&mut self, ui: &mut Ui) {
        if !self.show_bottom {
            return;
        }
        let palette = self.palette;
        let mut copy_output = false;
        let mut save_output = false;
        egui::Panel::bottom("output_panel")
            .resizable(true)
            .default_size(240.0)
            .min_size(90.0)
            .frame(ui::panel_frame(&palette))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui::tab(ui, &palette, "Вывод", self.bottom_tab == BottomTab::Output)
                        .clicked()
                    {
                        self.bottom_tab = BottomTab::Output;
                    }
                    if ui::tab(ui, &palette, "Ошибки", self.bottom_tab == BottomTab::Errors)
                        .clicked()
                    {
                        self.bottom_tab = BottomTab::Errors;
                    }
                    if !self.errors.is_empty() {
                        ui::badge(ui, &palette, &self.errors.len().to_string(), palette.error);
                    }
                    if ui::tab(
                        ui,
                        &palette,
                        "Упоминания",
                        self.bottom_tab == BottomTab::References,
                    )
                    .clicked()
                    {
                        self.bottom_tab = BottomTab::References;
                    }
                    if !self.references.is_empty() {
                        ui::badge(
                            ui,
                            &palette,
                            &self.references.len().to_string(),
                            palette.accent,
                        );
                    }
                    if ui::tab(ui, &palette, "Справка", self.bottom_tab == BottomTab::Reference)
                        .clicked()
                    {
                        self.bottom_tab = BottomTab::Reference;
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui::icon_button(ui, &palette, Icon::Trash, false, true, "Очистить панель")
                            .clicked()
                        {
                            match self.bottom_tab {
                                BottomTab::Output => self.output.clear(),
                                BottomTab::Errors => self.errors.clear(),
                                BottomTab::References => self.references.clear(),
                                BottomTab::Reference => {}
                            }
                        }
                        if self.bottom_tab == BottomTab::Output {
                            if ui::icon_button(ui, &palette, Icon::Download, false, true, "Сохранить вывод в файл").clicked() {
                                save_output = true;
                            }
                            if ui::icon_button(ui, &palette, Icon::Copy, false, true, "Копировать вывод").clicked() {
                                copy_output = true;
                            }
                        }
                        if self.run.is_some() {
                            ui::badge(ui, &palette, "выполняется", palette.ok);
                        }
                    });
                });
                ui.add_space(2.0);
                ui.separator();
                if self.awaiting_input {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Ввод:").strong().color(palette.accent));
                        let resp = ui.add(
                            TextEdit::singleline(&mut self.input_line)
                                .desired_width(ui.available_width() - 110.0)
                                .hint_text("Введите строку и нажмите Enter"),
                        );
                        if self.input_focus {
                            resp.request_focus();
                            self.input_focus = false;
                        }
                        let send = ui.button("Отправить").clicked()
                            || (resp.has_focus() && ui.input(|i| i.key_pressed(Key::Enter)));
                        if send {
                            self.send_input();
                        }
                    });
                    ui.separator();
                }
                match self.bottom_tab {
                    BottomTab::Output => {
                        let text = self.output.clone();
                        ScrollArea::vertical()
                            .id_salt("output_scroll")
                            .auto_shrink([false, false])
                            .stick_to_bottom(true)
                            .show(ui, |ui| {
                                ui.add(
                                    Label::new(
                                        RichText::new(if text.is_empty() {
                                            "Здесь появится вывод программы."
                                        } else {
                                            &text
                                        })
                                        .monospace()
                                        .color(if text.is_empty() {
                                            palette.muted
                                        } else {
                                            palette.text
                                        }),
                                    )
                                    .selectable(true)
                                    .wrap(),
                                );
                            });
                    }
                    BottomTab::Errors => {
                        let errors = self.errors.clone();
                        ScrollArea::vertical()
                            .id_salt("errors_scroll")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                if errors.is_empty() {
                                    ui.label(
                                        RichText::new("Ошибок нет.").small().color(palette.muted),
                                    );
                                }
                                for e in errors {
                                    let resp = ui.add(
                                        Label::new(
                                            RichText::new(&e.text).monospace().color(palette.error),
                                        )
                                        .selectable(true)
                                        .sense(Sense::click()),
                                    );
                                    if resp.clicked() {
                                        if let Some(line) = e.line {
                                            if let Some(tab) = self.tabs.get_mut(self.active) {
                                                tab.jump_to_line = Some(line);
                                            }
                                        }
                                    }
                                }
                            });
                    }
                    BottomTab::References => {
                        let refs = self.references.clone();
                        ScrollArea::vertical()
                            .id_salt("references_scroll")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                if refs.is_empty() {
                                    ui.label(
                                        RichText::new(
                                            "Поставьте курсор на имя и выберите «Найти упоминания» в контекстном меню.",
                                        )
                                        .small()
                                        .color(palette.muted),
                                    );
                                }
                                for (line, text) in refs {
                                    let label = Label::new(
                                        RichText::new(format!("{line:>4}  {text}"))
                                            .monospace()
                                            .small()
                                            .color(palette.text),
                                    )
                                    .sense(Sense::click());
                                    if ui.add(label).clicked() {
                                        if let Some(tab) = self.tabs.get_mut(self.active) {
                                            tab.jump_to_line = Some(line);
                                        }
                                    }
                                }
                            });
                    }
                    BottomTab::Reference => {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(
                                    "Полная справка с поиском открывается по F1 или через меню «Справка».",
                                )
                                .small()
                                .color(palette.muted),
                            );
                            if ui.button("Открыть справку").clicked() {
                                self.show_help = true;
                            }
                        });
                        ScrollArea::vertical()
                            .id_salt("reference_scroll")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.label(RichText::new(QUICK_REFERENCE).monospace().small());
                            });
                    }
                }
            });
        if copy_output {
            clipboard_set(&self.output);
            self.toast = Some((
                "Вывод скопирован в буфер обмена".to_string(),
                std::time::Instant::now(),
            ));
        }
        if save_output {
            self.open_output_save_dialog();
        }
    }

    // ------------------------------------------------------------------
    // Central editor
    // ------------------------------------------------------------------

    fn central(&mut self, ui: &mut Ui) {
        let palette = self.palette;
        let ctx = ui.ctx().clone();
        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(palette.bg).inner_margin(Margin::same(0)))
            .show(ui, |ui| {
                if self.tabs.is_empty() {
                    self.welcome(ui);
                    return;
                }
                self.tab_bar(ui);
                if self
                    .tabs
                    .get(self.active)
                    .map(|t| t.find.open)
                    .unwrap_or(false)
                {
                    self.find_bar(ui);
                }
                self.refresh_outline();
                let font_size = self.font_size;
                let palette = self.palette;
                let show_line_numbers = self.show_line_numbers;
                let run_line = if self
                    .active_tab()
                    .and_then(|t| t.path.as_ref())
                    .map(|p| Some(p) == self.run_path.as_ref())
                    .unwrap_or(false)
                {
                    self.run_line
                } else {
                    0
                };
                let active = self.active;
                let mut breakpoints = self.breakpoints.clone();
                let settings = self.settings.clone();
                let touch = is_touch();
                if touch {
                    self.touch_symbol_bar(ui);
                }
                let mut command = None;
                let mut app_action = None;
                if let Some(tab) = self.tabs.get_mut(active) {
                    let result = draw_editor(
                        ui,
                        tab,
                        &palette,
                        font_size,
                        show_line_numbers,
                        run_line,
                        &mut breakpoints,
                        &settings,
                        touch,
                    );
                    if let Some(line) = result.toggled_breakpoint {
                        if !self.breakpoints.remove(&line) {
                            self.breakpoints.insert(line);
                        }
                        if let Some(run) = &self.run {
                            run.shared.set_breakpoints(self.breakpoints.clone());
                        }
                    }
                    command = result.command;
                    app_action = result.action;
                }
                if let Some(action) = app_action {
                    self.actions.push(action);
                }
                if let Some(command) = command {
                    match command {
                        EditorCommand::GoToDefinition => self.go_to_definition(&ctx),
                        EditorCommand::FindReferences => self.find_references(),
                        EditorCommand::Rename => self.open_rename(),
                        EditorCommand::Help => {
                            let word = self
                                .active_tab()
                                .map(|t| t.menu_word.clone())
                                .unwrap_or_default();
                            self.help_focus = Some(word);
                            self.show_help = true;
                        }
                    }
                }
            });
    }

    /// A compact row of symbols that are hard to reach on a phone keyboard.
    fn touch_symbol_bar(&mut self, ui: &mut Ui) {
        let palette = self.palette;
        let mut insert: Option<&'static str> = None;
        let mut tab_pressed = false;
        let mut untab_pressed = false;
        Frame::NONE
            .fill(palette.panel_alt)
            .inner_margin(Margin::symmetric(4, 3))
            .show(ui, |ui| {
                ScrollArea::horizontal()
                    .id_salt("touch_symbols")
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for symbol in [":=", "=", "(", ")", "[", "]", "'", ",", "|"] {
                                if ui
                                    .add(egui::Button::new(
                                        RichText::new(symbol).monospace().small(),
                                    ))
                                    .clicked()
                                {
                                    insert = Some(symbol);
                                }
                            }
                            if ui
                                .add(egui::Button::new(RichText::new("нс").monospace().small()))
                                .on_hover_text("Символ новой строки")
                                .clicked()
                            {
                                insert = Some("нс");
                            }
                            ui.separator();
                            if ui
                                .add(egui::Button::new(RichText::new("Tab").small()))
                                .clicked()
                            {
                                tab_pressed = true;
                            }
                            if ui
                                .add(egui::Button::new(RichText::new("Отступ −").small()))
                                .on_hover_text("Снять отступ")
                                .clicked()
                            {
                                untab_pressed = true;
                            }
                            ui.separator();
                            if ui.small_button("Отменить").clicked() {
                                if let Some(tab) = self.tabs.get_mut(self.active) {
                                    tab.undo();
                                }
                            }
                            if ui.small_button("Повторить").clicked() {
                                if let Some(tab) = self.tabs.get_mut(self.active) {
                                    tab.redo();
                                }
                            }
                        });
                    });
            });
        let width = self.settings.tab_width;
        if let Some(tab) = self.tabs.get_mut(self.active) {
            if let Some(symbol) = insert {
                let (text, sel) = replace_selection(tab, symbol);
                apply_edit(tab, text, sel);
            } else if tab_pressed {
                let (text, sel) = crate::editing::indent_selection(&tab.text, tab.sel, width);
                apply_edit(tab, text, sel);
            } else if untab_pressed {
                let (text, sel) = crate::editing::unindent_selection(&tab.text, tab.sel, width);
                apply_edit(tab, text, sel);
            }
        }
    }

    fn tab_bar(&mut self, ui: &mut Ui) {
        let palette = self.palette;
        Frame::NONE
            .fill(palette.panel)
            .inner_margin(Margin::symmetric(6, 3))
            .stroke(Stroke::new(1.0, palette.border))
            .show(ui, |ui| {
                ScrollArea::horizontal()
                    .id_salt("tabs_scroll")
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let mut close: Option<usize> = None;
                            let mut select: Option<usize> = None;
                            for (i, tab) in self.tabs.iter().enumerate() {
                                let active = i == self.active;
                                let title = if tab.dirty {
                                    format!("{} •", tab.title)
                                } else {
                                    tab.title.clone()
                                };
                                if ui::tab(ui, &palette, &title, active).clicked() {
                                    select = Some(i);
                                }
                                if ui::icon_button(
                                    ui,
                                    &palette,
                                    Icon::Close,
                                    false,
                                    true,
                                    "Закрыть вкладку",
                                )
                                .clicked()
                                {
                                    close = Some(i);
                                }
                                ui.add_space(4.0);
                            }
                            if ui::icon_button(
                                ui,
                                &palette,
                                Icon::Plus,
                                false,
                                true,
                                "Новая программа (Ctrl+N)",
                            )
                            .clicked()
                            {
                                self.actions.push(Action::New);
                            }
                            if let Some(i) = select {
                                self.active = i;
                                self.activate_tab_actor();
                                if let Some(tab) = self.tabs.get(i) {
                                    self.status = format!("Файл: {}", tab.title);
                                }
                            }
                            if let Some(i) = close {
                                self.close_tab(i);
                            }
                        });
                    });
            });
    }

    fn find_bar(&mut self, ui: &mut Ui) {
        let palette = self.palette;
        let mut next = false;
        let mut prev = false;
        let mut replace = false;
        let mut replace_all = false;
        let mut close = false;
        let mut refresh = false;
        Frame::NONE
            .fill(palette.panel_alt)
            .inner_margin(Margin::symmetric(8, 5))
            .stroke(Stroke::new(1.0, palette.border))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Найти:").small());
                    if let Some(tab) = self.tabs.get_mut(self.active) {
                        let resp = ui.add(
                            TextEdit::singleline(&mut tab.find.query)
                                .desired_width(180.0)
                                .hint_text("текст"),
                        );
                        refresh |= resp.changed();
                        ui.label(RichText::new("Заменить:").small());
                        ui.add(
                            TextEdit::singleline(&mut tab.find.replacement)
                                .desired_width(180.0)
                                .hint_text("на"),
                        );
                        if ui::icon_button(
                            ui,
                            &palette,
                            Icon::ChevronLeft,
                            false,
                            true,
                            "Предыдущее",
                        )
                        .clicked()
                        {
                            prev = true;
                        }
                        if ui::icon_button(
                            ui,
                            &palette,
                            Icon::ChevronRight,
                            false,
                            true,
                            "Следующее",
                        )
                        .clicked()
                        {
                            next = true;
                        }
                        if ui.button("Заменить").clicked() {
                            replace = true;
                        }
                        if ui.button("Заменить все").clicked() {
                            replace_all = true;
                        }
                        if ui.small_button("×").clicked() {
                            close = true;
                        }
                    }
                });
            });
        if let Some(tab) = self.tabs.get_mut(self.active) {
            if refresh {
                tab.find.refresh(&tab.text);
            }
            if prev {
                if let Some((a, b)) = tab.find.prev_match(&tab.text) {
                    tab.pending_select = Some((char_index(&tab.text, a), char_index(&tab.text, b)));
                }
            }
            if next {
                if let Some((a, b)) = tab.find.next_match(&tab.text) {
                    tab.pending_select = Some((char_index(&tab.text, a), char_index(&tab.text, b)));
                }
            }
            if replace {
                if let Some((a, b)) = tab.find.matches.get(tab.find.current.unwrap_or(0)).copied() {
                    let replacement = tab.find.replacement.clone();
                    tab.text.replace_range(a..b, &replacement);
                    tab.dirty = true;
                    tab.highlight_hash = 0;
                    tab.outline_dirty = true;
                    tab.find.refresh(&tab.text);
                }
            }
            if replace_all && !tab.find.query.is_empty() {
                let q = tab.find.query.clone();
                let r = tab.find.replacement.clone();
                tab.text = tab.text.replace(&q, &r);
                tab.dirty = true;
                tab.highlight_hash = 0;
                tab.outline_dirty = true;
                tab.find.refresh(&tab.text);
            }
            if close {
                tab.find.open = false;
            }
        }
    }

    fn welcome(&mut self, ui: &mut Ui) {
        let palette = self.palette;
        ui.vertical_centered(|ui| {
            ui.add_space(70.0);
            ui.label(
                RichText::new("КуМир")
                    .size(46.0)
                    .strong()
                    .color(palette.accent),
            );
            ui.label(
                RichText::new("Современная среда для языка Кумир")
                    .size(16.0)
                    .color(palette.muted),
            );
            ui.add_space(24.0);
            ui.horizontal(|ui| {
                let total = ui.available_width();
                ui.add_space(total * 0.32);
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new("＋ Новая программа").color(Color32::WHITE),
                        )
                        .fill(palette.accent),
                    )
                    .clicked()
                {
                    self.actions.push(Action::New);
                }
                if ui::icon_button(
                    ui,
                    &palette,
                    Icon::Open,
                    false,
                    true,
                    "Открыть файл (Ctrl+O)",
                )
                .clicked()
                {
                    self.actions.push(Action::OpenDialog);
                }
                if ui::icon_button(ui, &palette, Icon::Help, false, true, "Справка (F1)").clicked()
                {
                    self.actions.push(Action::ShowHelp);
                }
            });
            ui.add_space(30.0);
            ui.label(RichText::new("Быстрый старт").strong().color(palette.text));
            ui.label(
                RichText::new(
                    "F5 — запуск    •    F6 — пауза    •    F10 — шаг    •    F9 — точка останова",
                )
                .small()
                .color(palette.muted),
            );
            ui.label(
                RichText::new(
                    "Ctrl+F — поиск    •    Ctrl+S — сохранить    •    Ctrl+B — проверка",
                )
                .small()
                .color(palette.muted),
            );
        });
    }

    // ------------------------------------------------------------------
    // Dialogs
    // ------------------------------------------------------------------

    fn dialogs(&mut self, ctx: &egui::Context) {
        let palette = self.palette;
        if let Some(mut dialog) = self.field_save_dialog.take() {
            match dialog.show(ctx, &palette) {
                Some(DialogOutcome::Save(path)) => {
                    if let Ok(actor) = self.actor.lock() {
                        match std::fs::write(&path, actor.robot.to_fil_string()) {
                            Ok(()) => {
                                self.actor_view = ActorKind::Robot;
                                self.show_right = true;
                                self.right_tab = RightTab::Actor;
                                self.field_path = Some(path.clone());
                                self.field_size = (actor.robot.width, actor.robot.height);
                                self.actor_zoom = 1.0;
                                self.actor_pan = Vec2::ZERO;
                                self.field_undo.clear();
                                self.field_redo.clear();
                                self.field_last_target = None;
                                self.status = format!("Поле сохранено: {}", path.display());
                            }
                            Err(e) => {
                                self.errors.push(ErrorEntry {
                                    text: format!(
                                        "Не удалось сохранить поле «{}»: {e}",
                                        path.display()
                                    ),
                                    line: None,
                                });
                                self.bottom_tab = BottomTab::Errors;
                                self.show_bottom = true;
                            }
                        }
                    }
                }
                Some(DialogOutcome::Open(_)) | Some(DialogOutcome::Cancelled) => {}
                None => self.field_save_dialog = Some(dialog),
            }
        }

        if let Some(mut dialog) = self.field_dialog.take() {
            match dialog.show(ctx, &palette) {
                Some(DialogOutcome::Open(path)) => {
                    if let Ok(mut actor) = self.actor.lock() {
                        match actor.load_field_file(&path) {
                            Ok(()) => {
                                self.actor_view = ActorKind::Robot;
                                self.show_right = true;
                                self.right_tab = RightTab::Actor;
                                self.field_path = Some(path.clone());
                                self.field_size = (actor.robot.width, actor.robot.height);
                                self.actor_zoom = 1.0;
                                self.actor_pan = Vec2::ZERO;
                                self.field_undo.clear();
                                self.field_redo.clear();
                                self.field_last_target = None;
                                self.status = format!(
                                    "Поле загружено: {} ({}×{})",
                                    path.display(),
                                    actor.robot.width,
                                    actor.robot.height
                                );
                            }
                            Err(e) => {
                                self.errors.push(ErrorEntry {
                                    text: e,
                                    line: None,
                                });
                                self.bottom_tab = BottomTab::Errors;
                                self.show_bottom = true;
                            }
                        }
                    }
                }
                Some(DialogOutcome::Save(_)) | Some(DialogOutcome::Cancelled) => {}
                None => self.field_dialog = Some(dialog),
            }
        }

        if let Some((mut dialog, action)) = self.actor_dialog.take() {
            match dialog.show(ctx, &palette) {
                Some(DialogOutcome::Open(path)) => {
                    let result = match action {
                        ActorFileAction::OpenVod => {
                            kumir_core::actors::VodoleyState::load_vod(&path).map(|v| {
                                self.vod_edit = [
                                    v.capacity[0],
                                    v.capacity[1],
                                    v.capacity[2],
                                    v.initial[0],
                                    v.initial[1],
                                    v.initial[2],
                                    v.target,
                                ];
                                if let Ok(mut actor) = self.actor.lock() {
                                    actor.vodoley = v;
                                }
                            })
                        }
                        ActorFileAction::OpenKz => {
                            kumir_core::actors::GrasshopperState::load_kz(&path).map(|g| {
                                self.gh_edit = (
                                    g.forward,
                                    g.backward,
                                    g.start,
                                    g.borders.is_some(),
                                    g.borders.map(|b| b.0).unwrap_or(-5),
                                    g.borders.map(|b| b.1).unwrap_or(5),
                                    g.flags
                                        .iter()
                                        .map(|f| f.to_string())
                                        .collect::<Vec<_>>()
                                        .join(" "),
                                );
                                if let Ok(mut actor) = self.actor.lock() {
                                    if !actor.is_active(ActorKind::Grasshopper) {
                                        actor.active.push(ActorKind::Grasshopper);
                                    }
                                    actor.grasshopper = g;
                                }
                                self.actor_view = ActorKind::Grasshopper;
                                self.right_tab = RightTab::Actor;
                            })
                        }
                        ActorFileAction::OpenPng => {
                            kumir_core::actors::PainterState::load_png(&path).map(|p| {
                                if let Ok(mut actor) = self.actor.lock() {
                                    if !actor.is_active(ActorKind::Painter) {
                                        actor.active.push(ActorKind::Painter);
                                    }
                                    actor.painter = p;
                                }
                                self.actor_view = ActorKind::Painter;
                                self.right_tab = RightTab::Actor;
                                self.painter_texture = None;
                            })
                        }
                        _ => Ok(()),
                    };
                    match result {
                        Ok(()) => self.toast(format!("Загружено: {}", path.display())),
                        Err(e) => self.toast(e),
                    }
                }
                Some(DialogOutcome::Save(path)) => {
                    let result = match action {
                        ActorFileAction::SaveVod => self
                            .actor
                            .lock()
                            .map(|a| a.vodoley.to_vod_string())
                            .map_err(|_| "Ошибка доступа к исполнителю".to_string())
                            .and_then(|text| {
                                std::fs::write(&path, text)
                                    .map_err(|e| format!("Не удалось записать задание: {e}"))
                            }),
                        ActorFileAction::SaveKz => self
                            .actor
                            .lock()
                            .map(|a| a.grasshopper.to_kz_string())
                            .map_err(|_| "Ошибка доступа к исполнителю".to_string())
                            .and_then(|text| {
                                std::fs::write(&path, text)
                                    .map_err(|e| format!("Не удалось записать задание: {e}"))
                            }),
                        ActorFileAction::SavePng => self
                            .actor
                            .lock()
                            .map_err(|_| "Ошибка доступа к исполнителю".to_string())
                            .and_then(|a| a.painter.save_png(&path)),
                        ActorFileAction::SaveDrawing => {
                            let svg = self
                                .actor
                                .lock()
                                .map(|a| match self.actor_view {
                                    ActorKind::Turtle => {
                                        vector_to_svg(&a.turtle.segments, &[], 520.0, 520.0)
                                    }
                                    _ => vector_to_svg(
                                        &a.draw.segments,
                                        &a.draw.captions,
                                        800.0,
                                        600.0,
                                    ),
                                })
                                .map_err(|_| "Ошибка доступа к исполнителю".to_string());
                            svg.and_then(|text| {
                                std::fs::write(&path, text)
                                    .map_err(|e| format!("Не удалось записать SVG: {e}"))
                            })
                        }
                        _ => Ok(()),
                    };
                    match result {
                        Ok(()) => self.toast(format!("Сохранено: {}", path.display())),
                        Err(e) => self.toast(e),
                    }
                }
                Some(DialogOutcome::Cancelled) => {}
                None => self.actor_dialog = Some((dialog, action)),
            }
        }

        if let Some(dialog) = &mut self.output_dialog {
            match dialog.show(ctx, &palette) {
                Some(DialogOutcome::Save(path)) => {
                    self.output_dialog = None;
                    match std::fs::write(&path, &self.output) {
                        Ok(()) => self.toast(format!("Вывод сохранён: {}", path.display())),
                        Err(e) => self.toast(format!("Не удалось записать вывод: {e}")),
                    }
                }
                Some(DialogOutcome::Open(_)) | Some(DialogOutcome::Cancelled) => {
                    self.output_dialog = None;
                }
                None => {}
            }
        }

        if let Some(mut state) = self.rename.take() {
            let mut keep = true;
            let mut apply = false;
            let mut open = true;
            egui::Window::new(format!("Переименовать «{}»", state.from))
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label("Новое имя (заменяются все целые слова):");
                    let resp = ui.add(TextEdit::singleline(&mut state.to).desired_width(280.0));
                    resp.request_focus();
                    ui.horizontal(|ui| {
                        if ui.button("Переименовать").clicked() {
                            apply = true;
                            keep = false;
                        }
                        if ui.button("Отмена").clicked() {
                            keep = false;
                        }
                    });
                });
            if apply && !state.to.trim().is_empty() {
                let from = state.from.clone();
                let to = state.to.trim().to_string();
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    let (text, count) = crate::editing::replace_word_all(&tab.text, &from, &to);
                    let sel = tab.sel;
                    apply_edit(tab, text, sel);
                    self.status = format!("«{from}» → «{to}»: заменено {count}");
                }
            }
            if keep && open {
                self.rename = Some(state);
            }
        }

        if let Some(mut query) = self.quick_open.take() {
            let mut keep = true;
            let mut selected_path: Option<PathBuf> = None;
            let mut open = true;
            egui::Window::new("Быстрый переход к файлу")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_TOP, Vec2::new(0.0, 80.0))
                .default_width(if is_touch() { 380.0 } else { 520.0 })
                .show(ctx, |ui| {
                    let resp = ui.add(
                        TextEdit::singleline(&mut query)
                            .desired_width(f32::INFINITY)
                            .hint_text("Имя файла .kum"),
                    );
                    resp.request_focus();
                    let needle = query.trim().to_lowercase();
                    let mut candidates: Vec<PathBuf> = Vec::new();
                    for path in self
                        .recent
                        .iter()
                        .cloned()
                        .chain(self.explorer_entries.iter().cloned())
                    {
                        if path.is_file()
                            && path.extension().and_then(|e| e.to_str()) == Some("kum")
                            && !candidates.contains(&path)
                        {
                            candidates.push(path);
                        }
                    }
                    let mut shown = 0;
                    ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                        for path in candidates {
                            let name = path
                                .file_name()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_default();
                            if !needle.is_empty() && !name.to_lowercase().contains(&needle) {
                                continue;
                            }
                            if ui::list_row(ui, &palette, Icon::File, &name, false, false)
                                .on_hover_text(path.display().to_string())
                                .clicked()
                            {
                                selected_path = Some(path.clone());
                            }
                            shown += 1;
                            if shown >= 12 {
                                break;
                            }
                        }
                    });
                    if ui.input(|i| i.key_pressed(Key::Enter)) {
                        if let Some(path) = self
                            .recent
                            .iter()
                            .chain(self.explorer_entries.iter())
                            .find(|p| {
                                p.extension().and_then(|e| e.to_str()) == Some("kum")
                                    && needle.is_empty()
                                    || p.file_name()
                                        .map(|n| {
                                            n.to_string_lossy().to_lowercase().contains(&needle)
                                        })
                                        .unwrap_or(false)
                            })
                            .cloned()
                        {
                            selected_path = Some(path);
                        }
                    }
                });
            if let Some(path) = selected_path {
                self.actions.push(Action::OpenPath(path));
                keep = false;
            }
            if keep && open {
                self.quick_open = Some(query);
            }
        }

        if let Some(mut text) = self.goto_line.take() {
            let mut keep = true;
            let mut apply = false;
            let mut open = true;
            egui::Window::new("Перейти к строке")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    let resp = ui.add(
                        TextEdit::singleline(&mut text)
                            .desired_width(160.0)
                            .hint_text("номер строки"),
                    );
                    resp.request_focus();
                    ui.horizontal(|ui| {
                        if ui.button("Перейти").clicked() || ui.input(|i| i.key_pressed(Key::Enter))
                        {
                            apply = true;
                            keep = false;
                        }
                        if ui.button("Отмена").clicked() {
                            keep = false;
                        }
                    });
                });
            if apply {
                if let Ok(line) = text.trim().parse::<u32>() {
                    if let Some(tab) = self.tabs.get_mut(self.active) {
                        tab.jump_to_line = Some(line.max(1));
                    }
                    self.status = format!("Переход к строке {line}");
                } else {
                    self.toast("Номер строки должен быть числом");
                }
            }
            if keep && open {
                self.goto_line = Some(text);
            }
        }

        if self.show_shortcuts {
            egui::Window::new("Горячие клавиши")
                .open(&mut self.show_shortcuts)
                .collapsible(false)
                .resizable(false)
                .default_width(460.0)
                .show(ctx, |ui| {
                    egui::Grid::new("shortcuts_grid")
                        .num_columns(2)
                        .spacing([18.0, 4.0])
                        .striped(true)
                        .show(ui, |ui| {
                            for (keys, action) in SHORTCUTS {
                                ui.label(RichText::new(*keys).monospace().color(palette.accent));
                                ui.label(*action);
                                ui.end_row();
                            }
                        });
                });
        }

        if let Some(dialog) = &mut self.file_dialog {
            match dialog.show(ctx, &palette) {
                Some(DialogOutcome::Open(path)) => {
                    self.file_dialog = None;
                    self.open_path(&path);
                }
                Some(DialogOutcome::Save(path)) => {
                    self.file_dialog = None;
                    if let Some(idx) = self.save_after_dialog.take() {
                        if self.save_tab_to(idx, &path) {
                            self.remove_tab(idx);
                        }
                    } else {
                        let idx = self.active;
                        self.save_tab_to(idx, &path);
                    }
                }
                Some(DialogOutcome::Cancelled) => {
                    self.file_dialog = None;
                    self.save_after_dialog = None;
                }
                None => {}
            }
        }

        if let Some(idx) = self.close_request {
            egui::Window::new("Несохранённые изменения")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label("В программе есть несохранённые изменения.");
                    ui.horizontal(|ui| {
                        if ui.button("Сохранить и закрыть").clicked() {
                            self.close_request = None;
                            if self.tabs[idx].path.is_some() {
                                let path = self.tabs[idx].path.clone().unwrap();
                                if self.save_tab_to(idx, &path) {
                                    self.remove_tab(idx);
                                }
                            } else {
                                self.save_after_dialog = Some(idx);
                                self.open_dialog(Mode::Save);
                            }
                        }
                        if ui.button("Закрыть без сохранения").clicked() {
                            self.close_request = None;
                            self.remove_tab(idx);
                        }
                        if ui.button("Отмена").clicked() {
                            self.close_request = None;
                        }
                    });
                });
        }

        if self.quit_request {
            egui::Window::new("Выход")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label("Есть несохранённые программы. Выйти без сохранения?");
                    ui.horizontal(|ui| {
                        if ui.button("Выйти").clicked() {
                            self.quit_request = false;
                            self.persist_settings();
                            self.allow_close = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if ui.button("Отмена").clicked() {
                            self.quit_request = false;
                        }
                    });
                });
        }

        if self.show_help {
            if let Some(word) = self.help_focus.take() {
                if let Some(topic) = crate::help::topic_for_word(&word) {
                    self.help_topic = topic;
                }
            }
            let mut window_open = self.show_help;
            let mut selected = self.help_topic;
            egui::Window::new("Справка")
                .open(&mut window_open)
                .collapsible(false)
                .resizable(true)
                .default_width(if is_touch() { 480.0 } else { 900.0 })
                .default_height(if is_touch() { 560.0 } else { 640.0 })
                .show(ctx, |ui| {
                    ui.horizontal_top(|ui| {
                        ui.allocate_ui_with_layout(
                            egui::vec2(
                                if is_touch() { 130.0 } else { 190.0 },
                                ui.available_height(),
                            ),
                            Layout::top_down(Align::Min),
                            |ui| {
                                ScrollArea::vertical()
                                    .id_salt("help_topics")
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| {
                                        for topic in crate::help::TOPICS {
                                            let label = topic.title;
                                            if ui
                                                .selectable_label(selected == topic.id, label)
                                                .clicked()
                                            {
                                                selected = topic.id;
                                            }
                                        }
                                    });
                            },
                        );
                        ui.separator();
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.add(
                                    TextEdit::singleline(&mut self.help_query)
                                        .desired_width(ui.available_width() - 80.0)
                                        .hint_text("Поиск по справке"),
                                );
                                if ui.button("Найти").clicked()
                                    || ui.input(|i| i.key_pressed(Key::Enter))
                                {
                                    if let Some(hit) = crate::help::search(&self.help_query).first()
                                    {
                                        selected = hit.topic.id;
                                    }
                                }
                            });
                            let hits = crate::help::search(&self.help_query);
                            if !self.help_query.trim().is_empty() {
                                ui.label(
                                    RichText::new(format!("Найдено строк: {}", hits.len()))
                                        .small()
                                        .color(palette.muted),
                                );
                                ScrollArea::vertical()
                                    .id_salt("help_hits")
                                    .max_height(110.0)
                                    .show(ui, |ui| {
                                        for hit in &hits {
                                            let text =
                                                format!("{} — {}", hit.topic.title, hit.line);
                                            if ui
                                                .selectable_label(
                                                    false,
                                                    RichText::new(text).small(),
                                                )
                                                .clicked()
                                            {
                                                selected = hit.topic.id;
                                            }
                                        }
                                    });
                            }
                            ui.separator();
                            ScrollArea::vertical()
                                .id_salt("help_body")
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    crate::help::show_topic(ui, &palette, selected);
                                });
                        });
                    });
                });
            self.show_help = window_open;
            self.help_topic = selected;
        }
        if self.show_about {
            egui::Window::new("О программе")
                .open(&mut self.show_about)
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.heading("КуМир для Rust");
                    ui.label(format!("Версия {}", env!("CARGO_PKG_VERSION")));
                    ui.separator();
                    ui.label(
                        "Свободная реализация среды Кумир 2 на языке Rust.\n\
                         Ядро языка, стандартная библиотека, формат .kum и все шесть\n\
                         исполнителей перенесены с оригинального проекта (GPLv3).\n\
                         Интерфейс написан заново на egui/eframe.",
                    );
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(
                            "F1 — справка, F5 — запуск, F9 — точка останова, F12 — определение",
                        )
                        .small()
                        .color(palette.muted),
                    );
                });
        }
        if self.show_settings {
            let open = self.show_settings;
            let mut window_open = open;
            let mut close_now = false;
            let mut save = false;
            let mut restyle = false;
            egui::Window::new("Настройки")
                .open(&mut window_open)
                .collapsible(false)
                .resizable(true)
                .default_width(if is_touch() { 460.0 } else { 540.0 })
                .default_height(520.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        for (tab, label) in [
                            (SettingsTab::Appearance, "Вид"),
                            (SettingsTab::Editor, "Редактор"),
                            (SettingsTab::Run, "Выполнение"),
                            (SettingsTab::Scheme, "Своя схема"),
                        ] {
                            if ui::tab(ui, &palette, label, self.settings_tab == tab).clicked() {
                                self.settings_tab = tab;
                                if tab == SettingsTab::Scheme {
                                    self.scheme_text.clear();
                                }
                            }
                        }
                    });
                    ui.separator();
                    ScrollArea::vertical()
                        .id_salt("settings_scroll")
                        .auto_shrink([false, false])
                        .max_height(420.0)
                        .show(ui, |ui| match self.settings_tab {
                            SettingsTab::Appearance => {
                                ui::section_header(ui, &palette, "Цветовая схема");
                                for scheme in Scheme::ALL {
                                    let selected = self.settings.scheme == scheme;
                                    ui.horizontal(|ui| {
                                        if ui.radio(selected, scheme.label()).clicked() && !selected
                                        {
                                            self.settings.set_scheme(scheme);
                                            restyle = true;
                                        }
                                        if scheme != Scheme::Custom {
                                            let preview = scheme.palette();
                                            for color in [
                                                preview.bg,
                                                preview.panel,
                                                preview.accent,
                                                preview.keyword,
                                                preview.string,
                                            ] {
                                                let (rect, _) = ui.allocate_exact_size(
                                                    egui::vec2(14.0, 14.0),
                                                    Sense::hover(),
                                                );
                                                ui.painter().rect_filled(
                                                    rect,
                                                    CornerRadius::same(3),
                                                    color,
                                                );
                                            }
                                        }
                                    });
                                }
                                if ui.button("Взять текущие цвета как свою схему").clicked()
                                {
                                    self.settings.custom_palette = self.palette;
                                    self.settings.scheme = Scheme::Custom;
                                    self.settings_tab = SettingsTab::Scheme;
                                    restyle = true;
                                }
                                ui.add_space(8.0);
                                ui::section_header(ui, &palette, "Шрифт и масштаб");
                                ui.label("Размер шрифта");
                                if ui
                                    .add(
                                        egui::Slider::new(&mut self.settings.font_size, 9.0..=40.0)
                                            .integer(),
                                    )
                                    .changed()
                                {
                                    self.font_size = self.settings.font_size;
                                    restyle = true;
                                }
                                ui.checkbox(&mut self.settings.show_line_numbers, "Номера строк");
                                self.show_line_numbers = self.settings.show_line_numbers;
                                ui.checkbox(&mut self.settings.word_wrap, "Перенос длинных строк");
                                ui.checkbox(
                                    &mut self.settings.highlight_current_line,
                                    "Подсвечивать текущую строку",
                                );
                                ui.checkbox(
                                    &mut self.settings.indent_guides,
                                    "Направляющие отступов",
                                );
                                ui.label("Масштаб интерфейса (или щипок двумя пальцами)");
                                if ui
                                    .add(
                                        egui::Slider::new(&mut self.settings.ui_scale, 0.55..=3.0)
                                            .suffix("×"),
                                    )
                                    .changed()
                                {
                                    self.apply_ui_scale(ctx);
                                }
                            }
                            SettingsTab::Editor => {
                                ui::section_header(ui, &palette, "Набор текста");
                                ui.checkbox(
                                    &mut self.settings.auto_indent,
                                    "Автоотступ (Enter выравнивает строку по блокам)",
                                );
                                ui.horizontal(|ui| {
                                    ui.label("Ширина отступа:");
                                    ui.add(
                                        egui::DragValue::new(&mut self.settings.tab_width)
                                            .range(1..=12),
                                    );
                                });
                                ui.checkbox(
                                    &mut self.settings.autocomplete,
                                    "Автодополнение по Ctrl+Space и при вводе",
                                );
                                ui.checkbox(
                                    &mut self.settings.inline_suggestions,
                                    "Подсказка-призрак в строке (Tab — принять)",
                                );
                                ui.checkbox(
                                    &mut self.settings.auto_save,
                                    "Автосохранение перед запуском",
                                );
                                ui.add_space(8.0);
                                ui::section_header(ui, &palette, "Поведение при запуске");
                                ui.checkbox(
                                    &mut self.settings.auto_open_output,
                                    "Открывать панель «Вывод» при запуске",
                                );
                                ui.checkbox(
                                    &mut self.settings.auto_open_errors,
                                    "Открывать панель «Ошибки» при ошибке",
                                );
                            }
                            SettingsTab::Run => {
                                ui::section_header(ui, &palette, "Исполнители");
                                ui.checkbox(
                                    &mut self.settings.animate_actors,
                                    "Анимация шагов исполнителя",
                                );
                                self.actor_animate = self.settings.animate_actors;
                                ui.horizontal(|ui| {
                                    ui.label("Задержка шага:");
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut self.settings.actor_delay_ms)
                                                .range(0..=1000)
                                                .suffix(" мс"),
                                        )
                                        .changed()
                                    {
                                        self.actor_delay_ms = self.settings.actor_delay_ms;
                                    }
                                });
                                ui.add_space(6.0);
                                ui.label("Черепаха:");
                                ui.radio_value(
                                    &mut self.settings.turtle_bounds,
                                    kumir_core::actors::TurtleBounds::Refuse,
                                    "Арена 500×500 — отказ у воды (как в оригинале)",
                                );
                                ui.radio_value(
                                    &mut self.settings.turtle_bounds,
                                    kumir_core::actors::TurtleBounds::Free,
                                    "Без границ — рисовать можно везде",
                                );
                                self.apply_turtle_bounds();
                                ui.add_space(8.0);
                                ui::section_header(ui, &palette, "Отладчик");
                                ui.label(
                                    RichText::new(
                                        "Точки останова: F9 или клик по номеру строки. \
                                         Пауза — F6, шаг — F10, продолжить — F6.",
                                    )
                                    .small()
                                    .color(palette.muted),
                                );
                            }
                            SettingsTab::Scheme => {
                                ui::section_header(ui, &palette, "Цвета схемы");
                                ui.label(
                                    RichText::new(
                                        "Изменения сразу применяются к интерфейсу и сохраняются.",
                                    )
                                    .small()
                                    .color(palette.muted),
                                );
                                let mut custom = self.settings.custom_palette;
                                let mut changed = false;
                                egui::Grid::new("palette_grid")
                                    .num_columns(2)
                                    .spacing([12.0, 4.0])
                                    .show(ui, |ui| {
                                        for (role, label) in crate::theme::PALETTE_ROLES {
                                            ui.label(*label);
                                            let mut color =
                                                custom.get(role).unwrap_or(palette.text);
                                            if egui::color_picker::color_edit_button_srgba(
                                                ui,
                                                &mut color,
                                                egui::color_picker::Alpha::OnlyBlend,
                                            )
                                            .changed()
                                            {
                                                custom.set(role, color);
                                                changed = true;
                                            }
                                            ui.end_row();
                                        }
                                    });
                                if changed {
                                    self.settings.custom_palette = custom;
                                    self.settings.scheme = Scheme::Custom;
                                    restyle = true;
                                }
                                ui.horizontal(|ui| {
                                    if ui.button("Сбросить к тёмной").clicked() {
                                        self.settings.custom_palette = Palette::dark();
                                        restyle = true;
                                    }
                                    if ui.button("Сбросить к светлой").clicked() {
                                        self.settings.custom_palette = Palette::light();
                                        restyle = true;
                                    }
                                    if ui.button("Экспорт в текст").clicked() {
                                        self.scheme_text = self.settings.custom_palette.to_text();
                                    }
                                    if ui.button("Импорт из текста").clicked() {
                                        let mut palette = self.settings.custom_palette;
                                        let n = palette.apply_text(&self.scheme_text);
                                        self.settings.custom_palette = palette;
                                        self.settings.scheme = Scheme::Custom;
                                        self.scheme_message =
                                            Some(format!("Применено цветов: {n}"));
                                        restyle = true;
                                    }
                                });
                                if let Some(message) = &self.scheme_message {
                                    ui.label(RichText::new(message).small().color(palette.ok));
                                }
                                ui.add(
                                    TextEdit::multiline(&mut self.scheme_text)
                                        .desired_rows(6)
                                        .code_editor()
                                        .hint_text("accent=#5c9eff\nbg=#17191e"),
                                );
                            }
                        });
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("Готово").clicked() {
                            save = true;
                            close_now = true;
                        }
                        ui.label(
                            RichText::new("Настройки сохраняются автоматически.")
                                .small()
                                .color(palette.muted),
                        );
                    });
                });
            self.show_settings = window_open && !close_now;
            if restyle {
                self.palette = self.settings.palette();
                self.dark = self.palette.dark;
                theme::apply_style(ctx, &self.palette, self.font_size, is_touch());
                save = true;
            }
            if save {
                self.persist_settings();
            }
        }
    }
}

impl eframe::App for KumirApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            if self.tabs.iter().any(|t| t.dirty) {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.quit_request = true;
            } else {
                self.persist_settings();
                self.allow_close = true;
            }
        }
        // Responsive layout: hide the side panels on narrow (phone) screens.
        let width = ctx.input(|i| i.viewport_rect().width());
        let narrow = width < 980.0;
        if narrow != self.last_narrow {
            self.last_narrow = narrow;
            if narrow {
                self.show_left = false;
                self.show_right = false;
            } else {
                self.show_left = true;
                self.show_right = true;
            }
        }
        self.handle_pinch_zoom(&ctx);
        // Прямоугольник поля заполняется при отрисовке панели исполнителя;
        // сбрасываем, чтобы не масштабировать по устаревшим координатам.
        self.field_rect = None;
        self.handle_shortcuts(&ctx);
        self.poll_run(&ctx);
        self.handle_dropped_files(&ctx);

        // Отступы под системные панели (статус-бар / навигационная панель).
        let bg = self.palette.bg;
        if let Some([_l, top, _r, bottom]) = crate::android_platform::safe_area(&ctx) {
            if top > 0.0 {
                egui::Panel::top("safe_inset_top")
                    .exact_size(top)
                    .frame(Frame::NONE.fill(bg))
                    .show(ui, |_ui| {});
            }
            if bottom > 0.0 {
                egui::Panel::bottom("safe_inset_bottom")
                    .exact_size(bottom)
                    .frame(Frame::NONE.fill(bg))
                    .show(ui, |_ui| {});
            }
        }

        self.top_panels(ui);
        self.status_bar(ui);
        self.bottom_panel(ui);
        self.left_panel(ui);
        self.right_panel(ui);
        self.central(ui);
        self.dialogs(&ctx);
        self.apply_actions(&ctx);
        crate::android_platform::sync_clipboard(&ctx);
        self.draw_toast(&ctx);

        if self.run.is_some() {
            let actor_active = self
                .actor
                .lock()
                .map(|a| a.kind() != ActorKind::None)
                .unwrap_or(false);
            ctx.request_repaint_after(Duration::from_millis(if actor_active { 16 } else { 40 }));
        }
    }
}

// ----------------------------------------------------------------------
// Editor widget and syntax layout
// ----------------------------------------------------------------------

fn apply_edit(tab: &mut EditorTab, text: String, sel: (usize, usize)) {
    if text == tab.text {
        tab.pending_select = Some(sel);
        tab.sel = sel;
        return;
    }
    tab.undo.push(tab.text.clone());
    if tab.undo.len() > 300 {
        tab.undo.remove(0);
    }
    tab.redo.clear();
    tab.text = text;
    tab.last_text = tab.text.clone();
    tab.dirty = true;
    tab.highlight_hash = 0;
    tab.outline_dirty = true;
    tab.sel = sel;
    tab.pending_select = Some(sel);
    tab.recompute_highlights();
}

fn sorted_sel(sel: (usize, usize)) -> (usize, usize) {
    if sel.0 <= sel.1 {
        sel
    } else {
        (sel.1, sel.0)
    }
}

fn selection_text(text: &str, sel: (usize, usize)) -> String {
    let (a, b) = sorted_sel(sel);
    let chars: Vec<char> = text.chars().collect();
    chars[a.min(chars.len())..b.min(chars.len())]
        .iter()
        .collect()
}

fn replace_selection(tab: &mut EditorTab, text: &str) -> (String, (usize, usize)) {
    let (a, b) = sorted_sel(tab.sel);
    let chars: Vec<char> = tab.text.chars().collect();
    let a = a.min(chars.len());
    let b = b.min(chars.len());
    let mut result: String = chars[..a].iter().collect();
    result.push_str(text);
    result.extend(chars[b..].iter());
    let cursor = a + text.chars().count();
    (result, (cursor, cursor))
}

#[allow(clippy::too_many_arguments)]
/// Signature of the call whose argument list contains `cursor`.
fn call_signature_at(tab: &EditorTab, cursor: usize) -> Option<(String, usize)> {
    let chars: Vec<char> = tab.text.chars().collect();
    let cursor = cursor.min(chars.len());
    let mut depth = 0i32;
    let mut open = None;
    let mut i = cursor;
    while i > 0 {
        i -= 1;
        match chars[i] {
            ')' => depth += 1,
            '(' => {
                if depth == 0 {
                    open = Some(i);
                    break;
                }
                depth -= 1;
            }
            '\n' if depth == 0 => break,
            _ => {}
        }
    }
    let open = open?;
    let prefix: String = chars[..open].iter().collect();
    let (_, word) = completion::word_start(&prefix, open);
    let word = word.trim().to_string();
    if word.is_empty() {
        return None;
    }
    // Active argument: count commas at the top level.
    let mut active = 0usize;
    let mut d = 0i32;
    for ch in chars.iter().take(cursor).skip(open + 1) {
        match ch {
            '(' | '[' => d += 1,
            ')' | ']' => d -= 1,
            ',' if d == 0 => active += 1,
            _ => {}
        }
    }
    let normalized = kumir_core::ast::normalize_name(&word).to_lowercase();
    if let Some(program) = &tab.outline {
        if let Some(alg) = program
            .algorithms
            .iter()
            .find(|a| kumir_core::ast::normalize_name(&a.name).to_lowercase() == normalized)
        {
            let params: Vec<String> = alg
                .params
                .iter()
                .map(|p| format!("{} {}", type_name(&p.typ), p.name))
                .collect();
            let ret = alg
                .ret
                .as_ref()
                .map(|t| format!(" → {}", type_name(t)))
                .unwrap_or_default();
            return Some((format!("{}({}){ret}", alg.name, params.join(", ")), active));
        }
    }
    if let Some(spec) = kumir_core::actors::commands::lookup_all(&word).first() {
        return Some((format!("{}: {}", spec.signature(), spec.doc), active));
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn draw_editor(
    ui: &mut Ui,
    tab: &mut EditorTab,
    palette: &Palette,
    font_size: f32,
    show_line_numbers: bool,
    run_line: u32,
    breakpoints: &mut BTreeSet<u32>,
    settings: &Settings,
    touch: bool,
) -> EditorResult {
    let ctx = ui.ctx().clone();
    let id = Id::unique(("kumir_editor", tab.id));
    let font_id = FontId::monospace(font_size);
    let focused = ctx.memory(|m| m.has_focus(id));
    let mut result = EditorResult::default();

    if focused {
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Z)) {
            tab.undo();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Y))
            || ctx.input_mut(|i| i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::Z))
        {
            tab.redo();
        }
    }

    // Completion: Ctrl+Space opens the full list, arrow keys navigate and
    // Enter/Tab accept. The keys are consumed before TextEdit sees them.
    let mut completion_manual = false;
    let mut completion_accept = false;
    let mut completion_nav = false;
    if focused {
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Space)) {
            tab.completion.manual = true;
            completion_manual = true;
        }
        if tab.completion.open {
            let down = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowDown));
            let up = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowUp));
            if !tab.completion.items.is_empty() {
                let len = tab.completion.items.len();
                if down {
                    tab.completion.selected = (tab.completion.selected + 1) % len;
                    completion_nav = true;
                }
                if up {
                    tab.completion.selected = (tab.completion.selected + len - 1) % len;
                    completion_nav = true;
                }
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
                tab.completion.close();
            } else if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter))
                || ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Tab))
            {
                if !tab.completion.items.is_empty() {
                    completion_accept = true;
                } else {
                    tab.completion.close();
                }
            }
        }
    } else {
        tab.completion.close();
    }

    // Automatic indentation, Tab/Shift+Tab, comment toggle and line operations.
    if focused && !completion_accept {
        if settings.auto_indent
            && !tab.completion.open
            && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter))
        {
            let (a, b) = sorted_sel(tab.sel);
            let indent = crate::editing::indent_for_new_line(&tab.text, a, settings.tab_width);
            let chars: Vec<char> = tab.text.chars().collect();
            let a = a.min(chars.len());
            let b = b.min(chars.len());
            let mut text: String = chars[..a].iter().collect();
            text.push('\n');
            text.push_str(&indent);
            text.extend(chars[b..].iter());
            let cursor = a + 1 + indent.chars().count();
            apply_edit(tab, text, (cursor, cursor));
            tab.completion.close();
        }

        let shift = ctx.input(|i| i.modifiers.shift);
        let tab_key = ctx.input_mut(|i| {
            i.consume_key(
                if shift {
                    Modifiers::SHIFT
                } else {
                    Modifiers::NONE
                },
                Key::Tab,
            )
        });
        if tab_key {
            if shift {
                let (text, sel) =
                    crate::editing::unindent_selection(&tab.text, tab.sel, settings.tab_width);
                apply_edit(tab, text, sel);
            } else if let Some(suffix) = tab.inline_hint.take() {
                let (text, sel) = replace_selection(tab, &suffix);
                apply_edit(tab, text, sel);
            } else {
                let (text, sel) =
                    crate::editing::indent_selection(&tab.text, tab.sel, settings.tab_width);
                apply_edit(tab, text, sel);
            }
        }

        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Slash)) {
            tab.toggle_comment(settings.tab_width);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::D)) {
            let (text, cursor) = crate::editing::duplicate_line(&tab.text, tab.sel.0);
            apply_edit(tab, text, (cursor, cursor));
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::ALT, Key::ArrowUp)) {
            let (text, cursor) = crate::editing::move_line(&tab.text, tab.sel.0, true);
            apply_edit(tab, text, (cursor, cursor));
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::ALT, Key::ArrowDown)) {
            let (text, cursor) = crate::editing::move_line(&tab.text, tab.sel.0, false);
            apply_edit(tab, text, (cursor, cursor));
        }
    }

    let digit_width = ui.fonts_mut(|f| f.glyph_width(&font_id, '0'));
    let line_count = tab.text.lines().count().max(1);
    let digits = line_count.to_string().len();
    let gutter_width = if show_line_numbers {
        digits as f32 * digit_width + 22.0
    } else {
        10.0
    };
    let highlights = tab.highlights.clone();
    let find_ranges = tab.find.matches.clone();
    let protected: BTreeSet<usize> = tab.file.protected_lines.iter().map(|l| l + 1).collect();
    let cursor_line = tab.cursor_line;
    let block_lines = crate::editing::matching_blocks(&tab.text);
    let matching_line = block_lines
        .get(cursor_line.saturating_sub(1) as usize)
        .copied()
        .flatten()
        .map(|l| l as u32 + 1);
    let line_indents: Vec<usize> = tab
        .text
        .lines()
        .map(|line| {
            let spaces = line.chars().take_while(|c| *c == ' ').count();
            spaces / settings.tab_width.max(1)
        })
        .collect();
    let text_for_layout = tab.text.clone();
    let _ = &text_for_layout;
    let palette_copy = *palette;
    let mut toggled_bp = None;

    let wrap = settings.word_wrap;
    let mut layouter = |ui: &Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
        let mut job = build_layout_job(
            buf.as_str(),
            &highlights,
            &find_ranges,
            font_id.clone(),
            &palette_copy,
            run_line,
            &protected,
        );
        if wrap {
            job.wrap.max_width = wrap_width;
        }
        ui.fonts_mut(|f| f.layout_job(job))
    };

    let scroll_output = ScrollArea::both()
        .id_salt(("editor_scroll", tab.id))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                let (gutter_rect, _) =
                    ui.allocate_exact_size(Vec2::new(gutter_width, 1.0), Sense::hover());
                let mut output = TextEdit::multiline(&mut tab.text)
                    .id(id)
                    .font(font_id.clone())
                    .lock_focus(true)
                    .desired_width(if wrap {
                        (ui.available_width() - gutter_width - 16.0).max(120.0)
                    } else {
                        f32::INFINITY
                    })
                    .desired_rows(24)
                    .frame(Frame::NONE)
                    .margin(Margin::symmetric(6, 2))
                    .layouter(&mut layouter)
                    .show(ui);

                if tab.pending_focus {
                    output.response.request_focus();
                    tab.pending_focus = false;
                }

                let painter = ui.painter().with_clip_rect(ui.clip_rect());
                let galley = &output.galley;
                let galley_pos = output.galley_pos;
                let gutter_full = Rect::from_min_max(
                    egui::pos2(gutter_rect.left(), galley_pos.y),
                    egui::pos2(gutter_rect.right(), galley_pos.y + galley.rect.height()),
                );
                painter.rect_filled(gutter_full, CornerRadius::ZERO, palette_copy.bg);

                let mut line = 1usize;
                let mut row_ranges: Vec<(usize, f32, f32)> = Vec::new();
                for row in &galley.rows {
                    row_ranges.push((line, galley_pos.y + row.pos.y, row.row.size.y.max(1.0)));
                    if row.ends_with_newline {
                        line += 1;
                    }
                }
                for (line, y, h) in &row_ranges {
                    let is_run = *line as u32 == run_line && run_line > 0;
                    let is_cursor = *line as u32 == cursor_line;
                    if is_run {
                        painter.rect_filled(
                            Rect::from_min_max(
                                egui::pos2(gutter_full.left(), *y),
                                egui::pos2(gutter_full.right(), y + h),
                            ),
                            CornerRadius::ZERO,
                            palette_copy.accent_weak,
                        );
                        painter.rect_filled(
                            Rect::from_min_size(
                                egui::pos2(gutter_full.left(), *y),
                                Vec2::new(3.0, *h),
                            ),
                            CornerRadius::ZERO,
                            palette_copy.accent,
                        );
                    }
                    if show_line_numbers {
                        let color = if is_run {
                            palette_copy.accent
                        } else if is_cursor {
                            palette_copy.gutter_active
                        } else {
                            palette_copy.gutter
                        };
                        painter.text(
                            egui::pos2(gutter_full.right() - 8.0, *y),
                            Align2::RIGHT_TOP,
                            line.to_string(),
                            font_id.clone(),
                            color,
                        );
                    }
                    if *line as u32 == cursor_line {
                        painter.rect_filled(
                            Rect::from_min_size(
                                egui::pos2(gutter_full.right() - 2.0, *y),
                                Vec2::new(2.0, *h),
                            ),
                            CornerRadius::ZERO,
                            palette_copy.accent,
                        );
                    }
                    if settings.indent_guides {
                        let line_index = (*line).saturating_sub(1);
                        let indent = line_indents.get(line_index).copied().unwrap_or(0);
                        if indent > 0 {
                            let space = ui.fonts_mut(|f| f.glyph_width(&font_id, ' '));
                            let step = space * settings.tab_width.max(1) as f32;
                            if step > 2.0 {
                                for level in 1..=indent {
                                    let x = galley_pos.x + level as f32 * step;
                                    painter.line_segment(
                                        [egui::pos2(x, *y), egui::pos2(x, y + h)],
                                        Stroke::new(1.0, palette_copy.border.gamma_multiply(0.35)),
                                    );
                                }
                            }
                        }
                    }
                    if breakpoints.contains(&(*line as u32)) {
                        painter.circle_filled(
                            egui::pos2(gutter_full.left() + 8.0, y + h * 0.5),
                            3.5,
                            palette_copy.error,
                        );
                    }
                }

                // Full-width current-line / execution-line highlight.
                let text_right = ui.clip_rect().right();
                for (line, y, h) in &row_ranges {
                    if *line as u32 == run_line && run_line > 0 {
                        painter.rect_filled(
                            Rect::from_min_max(
                                egui::pos2(galley_pos.x, *y),
                                egui::pos2(text_right, y + h),
                            ),
                            CornerRadius::ZERO,
                            Color32::from_rgba_unmultiplied(91, 157, 255, 22),
                        );
                    } else if Some(*line as u32) == matching_line {
                        painter.rect_filled(
                            Rect::from_min_max(
                                egui::pos2(galley_pos.x, *y),
                                egui::pos2(text_right, y + h),
                            ),
                            CornerRadius::ZERO,
                            Color32::from_rgba_unmultiplied(
                                palette_copy.accent.r(),
                                palette_copy.accent.g(),
                                palette_copy.accent.b(),
                                44,
                            ),
                        );
                    } else if *line as u32 == cursor_line && settings.highlight_current_line {
                        painter.rect_filled(
                            Rect::from_min_max(
                                egui::pos2(galley_pos.x, *y),
                                egui::pos2(text_right, y + h),
                            ),
                            CornerRadius::ZERO,
                            if palette_copy.dark {
                                Color32::from_rgba_unmultiplied(255, 255, 255, 9)
                            } else {
                                Color32::from_rgba_unmultiplied(0, 0, 0, 7)
                            },
                        );
                    }
                }
                painter.line_segment(
                    [
                        egui::pos2(gutter_full.right() - 1.0, gutter_full.top()),
                        egui::pos2(gutter_full.right() - 1.0, gutter_full.bottom()),
                    ],
                    Stroke::new(1.0, palette_copy.border),
                );

                // Convert cursor position to line:column.
                if let Some(range) = output.cursor_range {
                    let idx = range.primary.index;
                    let (l, c) = line_col_of_char(&tab.text, idx.0);
                    tab.cursor_line = l;
                    tab.cursor_col = c;
                    tab.sel = (idx.0, range.secondary.index.0);
                }

                // Jump to line / find selection.
                if let Some((a, b)) = tab.pending_select.take() {
                    let char_count = tab.text.chars().count();
                    let a = a.min(char_count);
                    let b = b.min(char_count);
                    let start = CCursor::new(a);
                    let end = CCursor::new(b);
                    output
                        .state
                        .cursor
                        .set_char_range(Some(CCursorRange::two(start, end)));
                    output.state.store(&ctx, id);
                    output.response.request_focus();
                    let r = galley.pos_from_cursor(CCursor::new(a));
                    ui.scroll_to_rect(
                        Rect::from_min_size(galley_pos + r.min.to_vec2(), r.size()),
                        Some(Align::Center),
                    );
                } else if let Some(line) = tab.jump_to_line.take() {
                    let byte = byte_index_of_line(&tab.text, line);
                    let ci = char_index(&tab.text, byte);
                    output
                        .state
                        .cursor
                        .set_char_range(Some(CCursorRange::one(CCursor::new(ci))));
                    output.state.store(&ctx, id);
                    output.response.request_focus();
                    let r = galley.pos_from_cursor(CCursor::new(ci));
                    ui.scroll_to_rect(
                        Rect::from_min_size(
                            galley_pos + r.min.to_vec2(),
                            Vec2::new(r.size().x.max(1.0), r.size().y),
                        ),
                        Some(Align::Center),
                    );
                }

                // ------------------------------------------------------------------
                // Code completion popup
                // ------------------------------------------------------------------
                let changed = output.response.changed();
                if changed {
                    tab.on_text_changed();
                }
                let cursor_char = output.cursor_range.map(|r| r.primary.index.0);
                let selection_empty = output
                    .cursor_range
                    .map(|r| r.primary == r.secondary)
                    .unwrap_or(true);
                let (word_start_char, prefix) = cursor_char
                    .map(|c| completion::word_start(&tab.text, c))
                    .unwrap_or((0, String::new()));
                let word_byte = byte_index_of_char(&tab.text, word_start_char);
                let in_literal = highlight_kind_at(&tab.highlights, word_byte)
                    .map(|k| {
                        matches!(
                            k,
                            HighlightKind::String
                                | HighlightKind::Char
                                | HighlightKind::Comment
                                | HighlightKind::Number
                        )
                    })
                    .unwrap_or(false);
                let cursor_screen = cursor_char.map(|c| {
                    let r = output.galley.pos_from_cursor(CCursor::new(c));
                    output.galley_pos + r.min.to_vec2()
                });

                // Inline suggestion: a grey ghost suffix of the best match.
                tab.inline_hint = None;
                if settings.inline_suggestions
                    && !tab.completion.open
                    && selection_empty
                    && !in_literal
                    && prefix.chars().count() >= 2
                {
                    let items = completion::suggest(&prefix, tab.outline.as_ref());
                    let lower = prefix.to_lowercase();
                    if let Some(item) = items.iter().find(|item| {
                        item.text.to_lowercase().starts_with(&lower)
                            && item.text.chars().count() > prefix.chars().count()
                            && !item.text.contains('(')
                    }) {
                        let suffix: String =
                            item.text.chars().skip(prefix.chars().count()).collect();
                        tab.inline_hint = Some(suffix);
                    }
                }
                if let (Some(hint), Some(pos)) = (&tab.inline_hint, cursor_screen) {
                    let ghost = ui.painter().layout_no_wrap(
                        hint.clone(),
                        font_id.clone(),
                        palette_copy.muted.gamma_multiply(0.75),
                    );
                    ui.painter().galley(
                        pos + Vec2::new(1.0, 0.0),
                        ghost,
                        palette_copy.muted.gamma_multiply(0.75),
                    );
                }

                // Signature help inside a call: `вперед(|)`.
                if let Some(pos) = cursor_screen {
                    if !tab.completion.open {
                        if let Some((signature, active)) =
                            call_signature_at(tab, cursor_char.unwrap_or(0))
                        {
                            egui::Area::new(Id::unique(("kumir_signature", tab.id)))
                                .order(egui::Order::Foreground)
                                .fixed_pos(pos + Vec2::new(0.0, font_size + 8.0))
                                .show(&ctx, |ui| {
                                    Frame::NONE
                                        .fill(palette_copy.panel)
                                        .stroke(Stroke::new(1.0, palette_copy.border))
                                        .corner_radius(5)
                                        .inner_margin(Margin::symmetric(8, 4))
                                        .show(ui, |ui| {
                                            ui.label(
                                                RichText::new(signature)
                                                    .monospace()
                                                    .small()
                                                    .color(palette_copy.builtin),
                                            );
                                            ui.label(
                                                RichText::new(format!("аргумент {}", active + 1))
                                                    .small()
                                                    .color(palette_copy.muted),
                                            );
                                        });
                                });
                        }
                    }
                }

                // Bounding rectangle of the current selection.
                let mut sel_rect = None;
                if let Some(range) = output.cursor_range {
                    let (a, b) = (range.primary.index.0, range.secondary.index.0);
                    if a != b {
                        let (lo, hi) = if a < b { (a, b) } else { (b, a) };
                        let ra = output.galley.pos_from_cursor(CCursor::new(lo));
                        let rb = output.galley.pos_from_cursor(CCursor::new(hi));
                        let min = egui::pos2(ra.left().min(rb.left()), ra.top().min(rb.top()));
                        let max =
                            egui::pos2(ra.right().max(rb.right()), ra.bottom().max(rb.bottom()));
                        sel_rect = Some(Rect::from_min_max(
                            galley_pos + min.to_vec2(),
                            galley_pos + max.to_vec2(),
                        ));
                    }
                }

                // Floating selection toolbar (touch input).
                if touch {
                    if let Some(rect) = sel_rect {
                        let screen = ctx.input(|i| i.viewport_rect());
                        let pos = egui::pos2(
                            (rect.center().x - 160.0)
                                .clamp(screen.left() + 4.0, screen.right() - 320.0),
                            (rect.top() - 42.0).max(screen.top() + 4.0),
                        );
                        egui::Area::new(Id::unique(("kumir_sel_bar", tab.id)))
                            .order(egui::Order::Foreground)
                            .fixed_pos(pos)
                            .show(&ctx, |ui| {
                                Frame::menu(ui.style()).show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        if ui::icon_button(
                                            ui,
                                            &palette_copy,
                                            Icon::Cut,
                                            false,
                                            true,
                                            "Вырезать",
                                        )
                                        .clicked()
                                        {
                                            let text = selection_text(&tab.text, tab.sel);
                                            ctx.copy_text(text);
                                            let (new_text, sel) = replace_selection(tab, "");
                                            apply_edit(tab, new_text, sel);
                                        }
                                        if ui::icon_button(
                                            ui,
                                            &palette_copy,
                                            Icon::Copy,
                                            false,
                                            true,
                                            "Копировать",
                                        )
                                        .clicked()
                                        {
                                            let text = selection_text(&tab.text, tab.sel);
                                            ctx.copy_text(text.clone());
                                            clipboard_set(&text);
                                        }
                                        if ui::icon_button(
                                            ui,
                                            &palette_copy,
                                            Icon::Paste,
                                            false,
                                            true,
                                            "Вставить",
                                        )
                                        .clicked()
                                        {
                                            if let Some(text) = clipboard_get() {
                                                let (new_text, sel) = replace_selection(tab, &text);
                                                apply_edit(tab, new_text, sel);
                                            }
                                        }
                                        if ui::icon_button(
                                            ui,
                                            &palette_copy,
                                            Icon::SelectAll,
                                            false,
                                            true,
                                            "Выделить всё",
                                        )
                                        .clicked()
                                        {
                                            let chars = tab.text.chars().count();
                                            tab.pending_select = Some((0, chars));
                                        }
                                        if ui::icon_button(
                                            ui,
                                            &palette_copy,
                                            Icon::Comment,
                                            false,
                                            true,
                                            "Закомментировать",
                                        )
                                        .clicked()
                                        {
                                            tab.toggle_comment(settings.tab_width);
                                        }
                                        if ui::icon_button(
                                            ui,
                                            &palette_copy,
                                            Icon::Definition,
                                            false,
                                            true,
                                            "Перейти к определению",
                                        )
                                        .clicked()
                                        {
                                            result.command = Some(EditorCommand::GoToDefinition);
                                        }
                                        if ui::icon_button(
                                            ui,
                                            &palette_copy,
                                            Icon::ChevronDown,
                                            false,
                                            true,
                                            "Ещё",
                                        )
                                        .clicked()
                                        {
                                            tab.context_menu = Some(pos);
                                            tab.menu_word =
                                                completion::word_start(&tab.text, tab.sel.0).1;
                                        }
                                        if ui::icon_button(
                                            ui,
                                            &palette_copy,
                                            Icon::Run,
                                            false,
                                            true,
                                            "Запустить",
                                        )
                                        .clicked()
                                        {
                                            result.action = Some(Action::Run);
                                        }
                                    });
                                });
                            });
                    }
                }

                // Context menu (right click or long touch).
                let open_menu =
                    output.response.secondary_clicked() || output.response.long_touched();
                if open_menu {
                    if let Some(pos) = output
                        .response
                        .interact_pointer_pos()
                        .or_else(|| output.response.hover_pos())
                    {
                        let cursor = output.galley.cursor_from_pos(pos - galley_pos);
                        let (start, prefix) = completion::word_start(&tab.text, cursor.index.0);
                        tab.menu_word = prefix.trim().to_string();
                        if output.response.long_touched() {
                            let end = start + prefix.chars().count();
                            tab.pending_select = Some((start, end));
                            tab.sel = (start, end);
                        } else {
                            let in_selection = tab.sel.0.min(tab.sel.1) <= cursor.index.0
                                && cursor.index.0 <= tab.sel.0.max(tab.sel.1);
                            if !in_selection {
                                tab.pending_select = Some((cursor.index.0, cursor.index.0));
                                tab.sel = (cursor.index.0, cursor.index.0);
                            }
                        }
                        tab.context_menu = Some(pos);
                    }
                }
                if let Some(pos) = tab.context_menu {
                    let mut close = false;
                    egui::Area::new(Id::unique(("kumir_context_menu", tab.id)))
                        .order(egui::Order::Foreground)
                        .fixed_pos(pos)
                        .show(&ctx, |ui| {
                            egui::Frame::menu(ui.style()).show(ui, |ui| {
                                ui.set_min_width(230.0);
                                ui.label(
                                    RichText::new(if tab.menu_word.is_empty() {
                                        "Редактор".to_string()
                                    } else {
                                        tab.menu_word.clone()
                                    })
                                    .small()
                                    .strong()
                                    .color(palette_copy.muted),
                                );
                                ui.separator();
                                if ui.button("Вырезать").clicked() {
                                    let text = selection_text(&tab.text, tab.sel);
                                    ctx.copy_text(text.clone());
                                    clipboard_set(&text);
                                    let (new_text, sel) = replace_selection(tab, "");
                                    apply_edit(tab, new_text, sel);
                                    close = true;
                                }
                                if ui.button("Копировать").clicked() {
                                    let text = selection_text(&tab.text, tab.sel);
                                    ctx.copy_text(text.clone());
                                    clipboard_set(&text);
                                    close = true;
                                }
                                if ui.button("Вставить").clicked() {
                                    if let Some(text) = clipboard_get() {
                                        let (new_text, sel) = replace_selection(tab, &text);
                                        apply_edit(tab, new_text, sel);
                                    }
                                    close = true;
                                }
                                if ui.button("Выделить всё").clicked() {
                                    let chars = tab.text.chars().count();
                                    tab.pending_select = Some((0, chars));
                                    close = true;
                                }
                                ui.separator();
                                let word = tab.menu_word.clone();
                                if ui
                                    .add_enabled(
                                        !word.is_empty(),
                                        egui::Button::new("Перейти к определению\tF12"),
                                    )
                                    .clicked()
                                {
                                    result.command = Some(EditorCommand::GoToDefinition);
                                    close = true;
                                }
                                if ui
                                    .add_enabled(
                                        !word.is_empty(),
                                        egui::Button::new("Найти все упоминания"),
                                    )
                                    .clicked()
                                {
                                    result.command = Some(EditorCommand::FindReferences);
                                    close = true;
                                }
                                if ui
                                    .add_enabled(
                                        !word.is_empty(),
                                        egui::Button::new("Переименовать…"),
                                    )
                                    .clicked()
                                {
                                    result.command = Some(EditorCommand::Rename);
                                    close = true;
                                }
                                if ui
                                    .add_enabled(
                                        !word.is_empty(),
                                        egui::Button::new("Справка по слову\tF1"),
                                    )
                                    .clicked()
                                {
                                    result.command = Some(EditorCommand::Help);
                                    close = true;
                                }
                                ui.separator();
                                if ui.button("Закомментировать\tCtrl+/").clicked() {
                                    tab.toggle_comment(settings.tab_width);
                                    close = true;
                                }
                                if ui.button("Дублировать строку\tCtrl+D").clicked()
                                {
                                    let (text, cursor) =
                                        crate::editing::duplicate_line(&tab.text, tab.sel.0);
                                    apply_edit(tab, text, (cursor, cursor));
                                    close = true;
                                }
                                if ui.button("Строку выше\tAlt+↑").clicked() {
                                    let (text, cursor) =
                                        crate::editing::move_line(&tab.text, tab.sel.0, true);
                                    apply_edit(tab, text, (cursor, cursor));
                                    close = true;
                                }
                                if ui.button("Строку ниже\tAlt+↓").clicked() {
                                    let (text, cursor) =
                                        crate::editing::move_line(&tab.text, tab.sel.0, false);
                                    apply_edit(tab, text, (cursor, cursor));
                                    close = true;
                                }
                                if ui.button("Форматировать отступы").clicked()
                                {
                                    tab.format_document(settings.tab_width);
                                    close = true;
                                }
                                ui.separator();
                                ui.menu_button("Вставить шаблон", |ui| {
                                    for (label, template) in [
                                        ("Если", "если условие то\n    \nвсе"),
                                        ("Цикл для", "нц для i от 1 до 10\n    \nкц"),
                                        ("Цикл пока", "нц пока условие\n    \nкц"),
                                        ("Вывод", "вывод , нс"),
                                        ("Алгоритм", "алг Имя\nнач\n    \nкон"),
                                    ] {
                                        if ui.button(label).clicked() {
                                            let (text, sel) = replace_selection(tab, template);
                                            apply_edit(tab, text, sel);
                                            close = true;
                                        }
                                    }
                                });
                                ui.separator();
                                if ui.button("Запустить\tF5").clicked() {
                                    result.action = Some(Action::Run);
                                    close = true;
                                }
                            });
                        });
                    if ctx.input(|i| i.key_pressed(Key::Escape)) {
                        close = true;
                    }
                    if close {
                        tab.context_menu = None;
                    }
                }

                if output.response.clicked() {
                    tab.completion.close();
                }

                let mut accept_index = if completion_accept && !tab.completion.items.is_empty() {
                    Some(tab.completion.selected)
                } else {
                    None
                };
                let should_refresh = changed
                    || completion_manual
                    || (tab.completion.open
                        && (tab.completion.prefix != prefix
                            || tab.completion.replace.1 != cursor_char.unwrap_or(word_start_char)));
                if accept_index.is_none() && should_refresh {
                    if selection_empty
                        && !in_literal
                        && (tab.completion.manual
                            || (settings.autocomplete && prefix.chars().count() >= 2))
                    {
                        let items = completion::suggest(&prefix, tab.outline.as_ref());
                        if !items.is_empty() {
                            if !tab.completion.open || tab.completion.prefix != prefix {
                                tab.completion.selected = 0;
                            }
                            tab.completion.items = items;
                            tab.completion.replace =
                                (word_start_char, cursor_char.unwrap_or(word_start_char));
                            tab.completion.prefix = prefix.clone();
                            tab.completion.open = true;
                        } else {
                            tab.completion.close();
                        }
                    } else {
                        tab.completion.close();
                    }
                } else if tab.completion.open && (!selection_empty || in_literal) {
                    tab.completion.close();
                }

                let mut popup_accept = None;
                if tab.completion.open && !tab.completion.items.is_empty() {
                    if let Some(cursor_pos) = cursor_screen {
                        let items = tab.completion.items.clone();
                        let mut selected =
                            tab.completion.selected.min(items.len().saturating_sub(1));
                        let screen = ctx.input(|i| i.viewport_rect());
                        let mut popup_pos = cursor_pos + Vec2::new(0.0, font_size + 6.0);
                        if popup_pos.y + 290.0 > screen.bottom() {
                            popup_pos.y = (cursor_pos.y - 296.0).max(screen.top() + 4.0);
                        }
                        popup_pos.x = popup_pos.x.clamp(
                            screen.left() + 4.0,
                            (screen.right() - 390.0).max(screen.left() + 4.0),
                        );
                        let pointer_moved = ui.input(|i| i.pointer.is_moving());
                        let tab_id = tab.id;
                        egui::Area::new(Id::unique(("kumir_completion", tab_id)))
                            .order(egui::Order::Foreground)
                            .fixed_pos(popup_pos)
                            .show(&ctx, |ui| {
                                Frame::NONE
                                    .fill(palette_copy.panel)
                                    .stroke(Stroke::new(1.0, palette_copy.accent_weak))
                                    .corner_radius(6)
                                    .inner_margin(6)
                                    .show(ui, |ui| {
                                        ui.set_min_width(330.0);
                                        ui.set_max_width(390.0);
                                        ui.label(
                                            RichText::new(
                                                "Подсказки · Enter/Tab — вставить · Esc — закрыть",
                                            )
                                            .small()
                                            .color(palette_copy.muted),
                                        );
                                        ScrollArea::vertical()
                                            .id_salt(("completion_scroll", tab_id))
                                            .max_height(240.0)
                                            .show(ui, |ui| {
                                                for (i, item) in items.iter().enumerate() {
                                                    let label = format!(
                                                        "{}   {}",
                                                        item.kind.icon(),
                                                        item.text
                                                    );
                                                    let resp = ui.add_sized(
                                                        [ui.available_width().max(220.0), 20.0],
                                                        egui::Button::selectable(
                                                            i == selected,
                                                            RichText::new(label).monospace(),
                                                        ),
                                                    );
                                                    if resp.hovered() && pointer_moved {
                                                        selected = i;
                                                    }
                                                    if resp.clicked() {
                                                        popup_accept = Some(i);
                                                    }
                                                    if i == selected && completion_nav {
                                                        resp.scroll_to_me(None);
                                                    }
                                                    resp.on_hover_text(format!(
                                                        "{} — {}",
                                                        item.kind.label(),
                                                        item.detail
                                                    ));
                                                }
                                            });
                                        if let Some(item) = items.get(selected) {
                                            ui.label(
                                                RichText::new(format!(
                                                    "{} — {}",
                                                    item.text, item.detail
                                                ))
                                                .small()
                                                .color(palette_copy.muted),
                                            );
                                        }
                                    });
                            });
                        tab.completion.selected = selected;
                    }
                }
                if let Some(i) = popup_accept {
                    accept_index = Some(i);
                }

                if let Some(index) = accept_index {
                    if let Some(item) = tab.completion.items.get(index).cloned() {
                        let (start, end) = tab.completion.replace;
                        let byte_start = byte_index_of_char(&tab.text, start);
                        let byte_end = byte_index_of_char(&tab.text, end);
                        if byte_start <= byte_end && byte_end <= tab.text.len() {
                            let insert = item.text;
                            tab.text.replace_range(byte_start..byte_end, &insert);
                            let new_char = start + insert.chars().count();
                            let mut edit_state =
                                egui::widgets::text_edit::TextEditState::load(&ctx, id)
                                    .unwrap_or_default();
                            edit_state
                                .cursor
                                .set_char_range(Some(CCursorRange::one(CCursor::new(new_char))));
                            edit_state.store(&ctx, id);
                            let (l, c) = line_col_of_char(&tab.text, new_char);
                            tab.cursor_line = l;
                            tab.cursor_col = c;
                            tab.completion.close();
                            tab.on_text_changed();
                            output.response.request_focus();
                        }
                    }
                }
                // Gutter click toggles a breakpoint.
                let gutter_resp =
                    ui.interact(gutter_full, Id::unique(("gutter", tab.id)), Sense::click());
                if gutter_resp.clicked() {
                    if let Some(pos) = gutter_resp.interact_pointer_pos() {
                        for (line, y, h) in &row_ranges {
                            if pos.y >= *y && pos.y < y + h {
                                toggled_bp = Some(*line as u32);
                                break;
                            }
                        }
                    }
                }
            });
        });
    let _ = scroll_output;

    if tab.last_text != tab.text {
        tab.on_text_changed();
    }
    result.toggled_breakpoint = toggled_bp;
    result
}

#[allow(clippy::field_reassign_with_default)]
fn build_layout_job(
    text: &str,
    spans: &[HighlightSpan],
    find: &[(usize, usize)],
    font_id: FontId,
    palette: &Palette,
    run_line: u32,
    protected_lines: &BTreeSet<usize>,
) -> LayoutJob {
    let len = text.len();
    let mut job = LayoutJob::default();
    job.text = text.to_string();
    job.wrap.max_width = f32::INFINITY;
    job.break_on_newline = true;
    if len == 0 {
        return job;
    }
    // Build color segments covering the entire text.  Highlight spans can be
    // one frame stale (the text buffer changes before the layouter runs), and
    // their byte offsets may land inside a multi-byte Cyrillic character.
    // Snap every boundary down to a valid UTF-8 char boundary.
    let mut segments: Vec<(usize, usize, Color32)> = Vec::new();
    let mut pos = 0usize;
    for span in spans {
        let start = floor_char_boundary(text, span.range.start.min(len));
        let end = floor_char_boundary(text, span.range.end.min(len));
        if start > pos {
            segments.push((pos, start, palette.ident));
        }
        if end > start {
            let color = color_for(span.kind, palette);
            let safe_start = start.max(pos);
            if safe_start < end {
                segments.push((safe_start, end, color));
                pos = pos.max(end);
            }
        }
    }
    if pos < len {
        segments.push((pos, len, palette.ident));
    }
    if segments.is_empty() {
        segments.push((0, len, palette.ident));
    }
    let find: Vec<(usize, usize)> = find
        .iter()
        .filter_map(|(a, b)| {
            let a = floor_char_boundary(text, (*a).min(len));
            let b = floor_char_boundary(text, (*b).min(len));
            if b > a {
                Some((a, b))
            } else {
                None
            }
        })
        .collect();

    let line_starts = line_starts(text);
    let mut boundaries = BTreeSet::new();
    boundaries.insert(0);
    boundaries.insert(len);
    for (a, b, _) in &segments {
        boundaries.insert(*a);
        boundaries.insert(*b);
    }
    for (a, b) in find.iter().copied() {
        if a < len {
            boundaries.insert(a);
        }
        if b <= len {
            boundaries.insert(b);
        }
    }
    let bounds: Vec<usize> = boundaries.into_iter().collect();
    for pair in bounds.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if a >= b {
            continue;
        }
        let color = segments
            .iter()
            .find(|(s, e, _)| *s <= a && a < *e)
            .map(|(_, _, c)| *c)
            .unwrap_or(palette.ident);
        let line = line_number_of(&line_starts, a);
        let in_find = find.iter().any(|(fa, fb)| *fa <= a && a < *fb);
        let mut background = Color32::TRANSPARENT;
        if in_find {
            background = palette.find_match;
        } else if run_line > 0 && line == run_line as usize {
            background = palette.run_line;
        } else if protected_lines.contains(&line) {
            background = palette.find_match.gamma_multiply(0.20);
        }
        let format = TextFormat {
            font_id: font_id.clone(),
            color,
            background,
            ..Default::default()
        };
        job.sections.push(LayoutSection {
            leading_space: 0.0,
            byte_range: egui::text::ByteIndex(a)..egui::text::ByteIndex(b),
            format,
        });
    }
    job
}

fn color_for(kind: HighlightKind, palette: &Palette) -> Color32 {
    match kind {
        HighlightKind::Keyword => palette.keyword,
        HighlightKind::Type => palette.type_,
        HighlightKind::Builtin => palette.builtin,
        HighlightKind::String | HighlightKind::Char => palette.string,
        HighlightKind::Number => palette.number,
        HighlightKind::Comment => palette.comment,
        HighlightKind::Operator => palette.text,
        HighlightKind::Ident => palette.ident,
        HighlightKind::Call => palette.call,
        HighlightKind::Error => palette.error,
    }
}

fn floor_char_boundary(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn byte_index_of_char(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map(|(byte, _)| byte)
        .unwrap_or(text.len())
}

fn highlight_kind_at(spans: &[HighlightSpan], byte: usize) -> Option<HighlightKind> {
    spans
        .iter()
        .find(|s| s.range.start <= byte && byte < s.range.end)
        .map(|s| s.kind)
}

fn line_starts(text: &str) -> Vec<usize> {
    let mut v = vec![0usize];
    for (i, c) in text.char_indices() {
        if c == '\n' {
            v.push(i + 1);
        }
    }
    v
}

fn line_number_of(starts: &[usize], byte: usize) -> usize {
    match starts.binary_search(&byte) {
        Ok(i) => i + 1,
        Err(i) => i,
    }
    .max(1)
}

fn line_col_of_char(text: &str, char_index: usize) -> (u32, u32) {
    let mut line = 1u32;
    let mut col = 1u32;
    for (i, c) in text.chars().enumerate() {
        if i >= char_index {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

fn char_index(text: &str, byte: usize) -> usize {
    let b = floor_char_boundary(text, byte.min(text.len()));
    text[..b].chars().count()
}

fn byte_index_of_line(text: &str, line: u32) -> usize {
    if line <= 1 {
        return 0;
    }
    let mut current = 1u32;
    for (i, c) in text.char_indices() {
        if c == '\n' {
            current += 1;
            if current == line {
                return i + 1;
            }
        }
    }
    text.len()
}

fn line_matches_word(line: &str, word: &str) -> bool {
    let code = line.split(['|', '!', '#']).next().unwrap_or(line);
    let mut rest = code;
    while let Some(pos) = rest.find(word) {
        let before = rest[..pos].chars().next_back();
        let after = rest[pos + word.len()..].chars().next();
        let is_boundary =
            |c: Option<char>| c.map(|c| !c.is_alphanumeric() && c != '_').unwrap_or(true);
        if is_boundary(before) && is_boundary(after) {
            return true;
        }
        rest = &rest[pos + word.len()..];
    }
    false
}

fn extract_line(msg: &str) -> Option<u32> {
    let start = msg.find("строке ")? + "строке ".len();
    let digits: String = msg[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

fn type_name(t: &TypeRef) -> String {
    let base = match &t.base {
        BaseType::Int => "цел",
        BaseType::Real => "вещ",
        BaseType::Str => "лит",
        BaseType::Char => "сим",
        BaseType::Bool => "лог",
        BaseType::File => "файл",
        BaseType::User(s) => s,
        BaseType::Void => "—",
    };
    if t.is_array {
        format!("{base}таб")
    } else {
        base.to_string()
    }
}

const QUICK_REFERENCE: &str = r#"ЯЗЫК КУМИР — КРАТКИЙ СПРАВОЧНИК
=================================

Программа:
    алг Имя алгоритма
    нач
        ...
    кон

    алг цел Функция(цел n)
    нач
        знач := n * 2
    кон

Типы: цел, вещ, лит, сим, лог, файл
Массивы: целтаб A[1:10], вещтаб M[1:3, 1:4]

Операторы:
    :=          присваивание
    + - * /     арифметика
    **          возведение в степень
    = <> < > <= >=
    и, или, не  логические операции
    div, mod    целочисленное деление и остаток

Ввод и вывод:
    ввод a, b
    вывод a, ' ', b, нс
    вывод x:7:3         ширина и число знаков после точки

Условия:
    если условие то
        ...
    иначе
        ...
    все

Выбор:
    выбор
    при x = 1: вывод "один"
    при x = 2: вывод "два"
    иначе вывод "нет"
    все

Циклы:
    нц для i от 1 до 10 [шаг 2]
        ...
    кц

    нц пока условие
        ...
    кц

    нц 5 раз
        ...
    кц

    нц
        ...
    кц при условие

    выход   — прервать цикл или вернуться из алгоритма
    пауза   — приостановить выполнение
    стоп    — завершить программу

Алгоритмы и параметры:
    алг Процедура(арг цел x, рез цел y, аргрез лит s)
    значение возвращается присваиванием: знач := ...

Строки:
    s := 'привет'
    s[1]        — первый символ
    s[1:3]      — подстрока (включительно)
    длин(s), код(s[1]), символ(65)

Стандартные функции:
    abs, iabs, sqrt, sin, cos, tg, arcsin, arccos, arctg,
    exp, ln, lg, min, max, imin, imax, sign, int,
    div, mod, rnd, irnd, rand, irand, время

Файлы (использовать Файлы):
    файл f
    f := открыть на чтение('input.txt')
    ввод f, x
    нц пока не конец файла(f)
        ввод f, x
    кц
    закрыть(f)

    g := открыть на запись('output.txt')
    вывод g, x, нс
    закрыть(g)

Строки (использовать Строки):
    позиция(фрагмент, строка)
    вставить(фрагмент, s, позиция)
    удалить(s, начало, количество)
    заменить(s, старый, новый, да)
    верхний регистр(s), нижний регистр(s)

Совет: имена могут состоять из нескольких слов:
    цел число проходов по таблицам
"#;

const SHORTCUTS: &[(&str, &str)] = &[
    ("F5", "Запустить программу"),
    ("Shift+F5", "Остановить"),
    ("F6", "Пауза / продолжить"),
    ("F10", "Шаг"),
    ("F9", "Точка останова на текущей строке"),
    ("F7", "Показать исполнителя"),
    ("F8 / Shift+F8", "Следующая / предыдущая ошибка"),
    ("F12", "Перейти к определению"),
    ("F1", "Справка"),
    ("Ctrl+N / Ctrl+O", "Новый / открыть файл"),
    ("Ctrl+S / Ctrl+Shift+S", "Сохранить / сохранить как"),
    ("Ctrl+W / Ctrl+Q", "Закрыть вкладку / выйти"),
    ("Ctrl+Z / Ctrl+Y", "Отменить / повторить"),
    (
        "Ctrl+X / Ctrl+C / Ctrl+V",
        "Вырезать / копировать / вставить",
    ),
    ("Ctrl+A", "Выделить всё"),
    ("Ctrl+F", "Найти и заменить"),
    ("Ctrl+G", "Перейти к строке"),
    ("Ctrl+P", "Быстрый переход к файлу"),
    ("Ctrl+B", "Проверить синтаксис"),
    ("Ctrl+/", "Закомментировать строки"),
    ("Ctrl+D", "Дублировать строку"),
    ("Alt+↑ / Alt+↓", "Переместить строку"),
    ("Ctrl+Shift+I", "Форматировать отступы"),
    ("Ctrl+Space", "Показать список подсказок"),
    ("Tab / Shift+Tab", "Отступ / снять отступ"),
    ("Ctrl+= / Ctrl+-", "Увеличить / уменьшить шрифт"),
];

// ----------------------------------------------------------------------
// Actor rendering
// ----------------------------------------------------------------------

fn cell_at(
    rect: Rect,
    field: &RobotField,
    zoom: f32,
    pan: Vec2,
    pos: egui::Pos2,
) -> Option<(i32, i32)> {
    let (gx, gy, cell) = robot_layout(rect, field, zoom, pan);
    let fx = (pos.x - gx) / cell;
    let fy = (pos.y - gy) / cell;
    if fx >= 0.0 && fy >= 0.0 && fx < field.width as f32 && fy < field.height as f32 {
        Some((fx.floor() as i32, fy.floor() as i32))
    } else {
        None
    }
}

fn edit_target(
    rect: Rect,
    field: &RobotField,
    zoom: f32,
    pan: Vec2,
    pos: egui::Pos2,
    tool: FieldTool,
    shift: bool,
) -> Option<EditTarget> {
    if tool == FieldTool::Pan {
        return None;
    }
    let (x, y) = cell_at(rect, field, zoom, pan, pos)?;
    if shift {
        return Some(EditTarget {
            x,
            y,
            kind: EditTargetKind::Mark,
        });
    }
    let (gx, gy, cell) = robot_layout(rect, field, zoom, pan);
    let lx = (pos.x - gx) / cell - x as f32;
    let ly = (pos.y - gy) / cell - y as f32;
    let kind = match tool {
        FieldTool::Color => EditTargetKind::Color,
        FieldTool::Mark => EditTargetKind::Mark,
        FieldTool::Robot => EditTargetKind::Robot,
        FieldTool::Pan => return None,
        FieldTool::Wall => {
            let distances = [
                (lx, kumir_core::actors::LEFT_WALL),
                (1.0 - lx, kumir_core::actors::RIGHT_WALL),
                (ly, kumir_core::actors::UP_WALL),
                (1.0 - ly, kumir_core::actors::DOWN_WALL),
            ];
            let &(_, bit) = distances
                .iter()
                .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
                .unwrap_or(&distances[0]);
            EditTargetKind::Wall(bit)
        }
    };
    Some(EditTarget { x, y, kind })
}

fn toggle_wall(field: &mut RobotField, x: i32, y: i32, bit: u8) {
    use kumir_core::actors::{DOWN_WALL, LEFT_WALL, RIGHT_WALL, UP_WALL};
    let (nx, ny, opposite) = match bit {
        LEFT_WALL => (x - 1, y, RIGHT_WALL),
        RIGHT_WALL => (x + 1, y, LEFT_WALL),
        UP_WALL => (x, y - 1, DOWN_WALL),
        DOWN_WALL => (x, y + 1, UP_WALL),
        _ => return,
    };
    if let Some(c) = field.cell_mut(x, y) {
        c.walls ^= bit;
    }
    if let Some(c) = field.cell_mut(nx, ny) {
        c.walls ^= opposite;
    }
}

fn apply_target(field: &mut RobotField, target: EditTarget) -> String {
    let (x, y) = (target.x, target.y);
    match target.kind {
        EditTargetKind::Color => {
            let mut state = false;
            if let Some(c) = field.cell_mut(x, y) {
                c.color = !c.color;
                state = c.color;
            }
            format!(
                "Клетка ({x}, {y}): {}",
                if state {
                    "закрашена"
                } else {
                    "очищена"
                }
            )
        }
        EditTargetKind::Mark => {
            let mut state = false;
            if let Some(c) = field.cell_mut(x, y) {
                c.point = !c.point;
                state = c.point;
            }
            format!(
                "Клетка ({x}, {y}): метка {}",
                if state {
                    "поставлена"
                } else {
                    "снята"
                }
            )
        }
        EditTargetKind::Wall(bit) => {
            toggle_wall(field, x, y, bit);
            let now_set = field
                .cell(x, y)
                .map(|c| c.walls & bit != 0)
                .unwrap_or(false);
            let side = match bit {
                kumir_core::actors::LEFT_WALL => "слева",
                kumir_core::actors::RIGHT_WALL => "справа",
                kumir_core::actors::UP_WALL => "сверху",
                _ => "снизу",
            };
            format!(
                "Клетка ({x}, {y}): стена {side} {}",
                if now_set {
                    "поставлена"
                } else {
                    "убрана"
                }
            )
        }
        EditTargetKind::Robot => {
            field.start_x = x;
            field.start_y = y;
            field.robot_x = x;
            field.robot_y = y;
            format!("Робот перемещён в ({x}, {y})")
        }
    }
}

fn draw_edit_ghost(
    ui: &mut Ui,
    rect: Rect,
    field: &RobotField,
    zoom: f32,
    pan: Vec2,
    target: EditTarget,
    palette: &Palette,
) {
    let (gx, gy, cell) = robot_layout(rect, field, zoom, pan);
    let r = Rect::from_min_size(
        egui::pos2(gx + target.x as f32 * cell, gy + target.y as f32 * cell),
        Vec2::splat(cell),
    );
    let painter = ui.painter_at(rect);
    match target.kind {
        EditTargetKind::Color => {
            let existing = field
                .cell(target.x, target.y)
                .map(|c| c.color)
                .unwrap_or(false);
            let color = if existing {
                palette.error
            } else {
                palette.accent
            };
            let fill = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 45);
            painter.rect_filled(r.shrink(1.0), CornerRadius::ZERO, fill);
            painter.rect_stroke(
                r,
                CornerRadius::ZERO,
                Stroke::new(2.0, color),
                egui::StrokeKind::Inside,
            );
        }
        EditTargetKind::Mark => {
            let existing = field
                .cell(target.x, target.y)
                .map(|c| c.point)
                .unwrap_or(false);
            let color = if existing {
                palette.error
            } else {
                palette.accent
            };
            let center = r.right_top() + Vec2::new(-cell * 0.16, cell * 0.16);
            painter.circle_stroke(center, cell * 0.12, Stroke::new(2.0, color));
            painter.rect_stroke(
                r,
                CornerRadius::ZERO,
                Stroke::new(1.0, color.gamma_multiply(0.6)),
                egui::StrokeKind::Inside,
            );
        }
        EditTargetKind::Wall(bit) => {
            let existing = field
                .cell(target.x, target.y)
                .map(|c| c.walls & bit != 0)
                .unwrap_or(false);
            let color = if existing {
                palette.error
            } else {
                palette.accent
            };
            let a = egui::pos2(gx + target.x as f32 * cell, gy + target.y as f32 * cell);
            let b = a + Vec2::new(cell, 0.0);
            let c = a + Vec2::new(0.0, cell);
            let d = a + Vec2::new(cell, cell);
            let (p1, p2) = match bit {
                kumir_core::actors::LEFT_WALL => (a, c),
                kumir_core::actors::RIGHT_WALL => (b, d),
                kumir_core::actors::UP_WALL => (a, b),
                _ => (c, d),
            };
            painter.line_segment(
                [p1, p2],
                Stroke::new(
                    (cell * 0.16).max(5.0),
                    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 70),
                ),
            );
            painter.line_segment([p1, p2], Stroke::new(2.5, color));
            painter.rect_stroke(
                r,
                CornerRadius::ZERO,
                Stroke::new(1.0, color.gamma_multiply(0.5)),
                egui::StrokeKind::Inside,
            );
        }
        EditTargetKind::Robot => {
            let center = r.center();
            painter.rect_stroke(
                r,
                CornerRadius::ZERO,
                Stroke::new(2.0, palette.accent),
                egui::StrokeKind::Inside,
            );
            painter.circle_stroke(center, cell * 0.32, Stroke::new(2.0, palette.accent));
            painter.text(
                center,
                Align2::CENTER_CENTER,
                "Р",
                FontId::proportional((cell * 0.35).max(8.0)),
                palette.accent,
            );
        }
    }
}

fn robot_layout(rect: Rect, field: &RobotField, zoom: f32, pan: Vec2) -> (f32, f32, f32) {
    let w = field.width.max(1) as f32;
    let h = field.height.max(1) as f32;
    let pad = 18.0;
    let base = ((rect.width() - 2.0 * pad) / w)
        .min((rect.height() - 2.0 * pad) / h)
        .max(1.0);
    let cell = (base * zoom).max(1.0);
    let center = rect.center() + pan;
    (center.x - cell * w / 2.0, center.y - cell * h / 2.0, cell)
}

fn draw_robot(
    ui: &mut Ui,
    rect: Rect,
    field: &RobotField,
    palette: &Palette,
    zoom: f32,
    pan: Vec2,
) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(6), palette.field_bg);

    let w = field.width.max(1) as f32;
    let h = field.height.max(1) as f32;
    let (gx, gy, cell) = robot_layout(rect, field, zoom, pan);
    let grid_w = cell * w;
    let grid_h = cell * h;
    let field_rect = Rect::from_min_size(egui::pos2(gx, gy), Vec2::new(grid_w, grid_h));
    painter.rect_filled(field_rect, CornerRadius::ZERO, palette.field_cell);

    let wall_stroke = Stroke::new((cell * 0.09).max(1.5), palette.field_wall);
    let grid_stroke = Stroke::new(1.0, palette.border);
    for y in 0..field.height {
        for x in 0..field.width {
            let r = Rect::from_min_size(
                egui::pos2(gx + x as f32 * cell, gy + y as f32 * cell),
                Vec2::splat(cell),
            );
            let c = field.cell(x, y);
            if let Some(c) = c {
                if c.color {
                    painter.rect_filled(r.shrink(1.0), CornerRadius::ZERO, palette.field_painted);
                }
                let mark = if c.symbol1 != '$' && c.symbol1 != '\0' {
                    Some(c.symbol1)
                } else if c.symbol != '$' && c.symbol != '\0' {
                    Some(c.symbol)
                } else {
                    None
                };
                if let Some(ch) = mark {
                    painter.text(
                        r.center(),
                        Align2::CENTER_CENTER,
                        ch,
                        FontId::proportional((cell * 0.55).max(7.0)),
                        palette.text,
                    );
                }
                if c.point {
                    painter.circle_filled(
                        r.right_top() + Vec2::new(-cell * 0.16, cell * 0.16),
                        (cell * 0.07).max(1.5),
                        palette.field_mark,
                    );
                }
                if c.walls & kumir_core::actors::LEFT_WALL != 0 {
                    painter.line_segment([r.left_top(), r.left_bottom()], wall_stroke);
                }
                if c.walls & kumir_core::actors::RIGHT_WALL != 0 {
                    painter.line_segment([r.right_top(), r.right_bottom()], wall_stroke);
                }
                if c.walls & kumir_core::actors::UP_WALL != 0 {
                    painter.line_segment([r.left_top(), r.right_top()], wall_stroke);
                }
                if c.walls & kumir_core::actors::DOWN_WALL != 0 {
                    painter.line_segment([r.left_bottom(), r.right_bottom()], wall_stroke);
                }
            }
            painter.rect_stroke(r, CornerRadius::ZERO, grid_stroke, egui::StrokeKind::Inside);
        }
    }
    painter.rect_stroke(
        field_rect,
        CornerRadius::ZERO,
        Stroke::new(2.0, palette.field_wall),
        egui::StrokeKind::Inside,
    );
    if field.crashed {
        painter.rect_stroke(
            field_rect.shrink(2.0),
            CornerRadius::ZERO,
            Stroke::new(3.0, palette.error),
            egui::StrokeKind::Inside,
        );
    }

    // Start cell.
    let sx = gx + (field.start_x as f32 + 0.5) * cell;
    let sy = gy + (field.start_y as f32 + 0.5) * cell;
    painter.circle_stroke(
        egui::pos2(sx, sy),
        cell * 0.14,
        Stroke::new(1.5, palette.muted),
    );

    // Robot.
    let rx = gx + (field.robot_x as f32 + 0.5) * cell;
    let ry = gy + (field.robot_y as f32 + 0.5) * cell;
    let center = egui::pos2(rx, ry);
    painter.circle_filled(center, cell * 0.32, palette.accent);
    painter.circle_filled(
        center + Vec2::new(-cell * 0.10, -cell * 0.05),
        (cell * 0.055).max(1.0),
        Color32::WHITE,
    );
    painter.circle_filled(
        center + Vec2::new(cell * 0.10, -cell * 0.05),
        (cell * 0.055).max(1.0),
        Color32::WHITE,
    );
    painter.text(
        center + Vec2::new(0.0, cell * 0.10),
        Align2::CENTER_CENTER,
        "Р",
        FontId::proportional((cell * 0.32).max(6.0)),
        Color32::WHITE,
    );

    painter.text(
        rect.left_top() + Vec2::new(8.0, 6.0),
        Align2::LEFT_TOP,
        format!(
            "Поле {}×{}   Робот: ({}, {})",
            field.width, field.height, field.robot_x, field.robot_y
        ),
        FontId::proportional(11.0),
        palette.muted,
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_vector_actor(
    ui: &mut Ui,
    rect: Rect,
    segments: &[kumir_core::actors::Segment],
    captions: &[kumir_core::actors::Caption],
    current: (f64, f64),
    palette: &Palette,
    pen_down: bool,
    zoom: f32,
    pan: Vec2,
) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(6), palette.bg);

    let mut min_x = current.0.min(0.0);
    let mut max_x = current.0.max(0.0);
    let mut min_y = current.1.min(0.0);
    let mut max_y = current.1.max(0.0);
    for s in segments {
        min_x = min_x.min(s.x1).min(s.x2);
        max_x = max_x.max(s.x1).max(s.x2);
        min_y = min_y.min(s.y1).min(s.y2);
        max_y = max_y.max(s.y1).max(s.y2);
    }
    for c in captions {
        min_x = min_x.min(c.x);
        max_x = max_x.max(c.x);
        min_y = min_y.min(c.y);
        max_y = max_y.max(c.y);
    }
    let range_x = (max_x - min_x).max(1.0);
    let range_y = (max_y - min_y).max(1.0);
    let pad = 18.0;
    let base = ((rect.width() - 2.0 * pad) / range_x as f32)
        .min((rect.height() - 2.0 * pad) / range_y as f32)
        .max(0.1);
    let scale = base * zoom;
    let cx = (min_x + max_x) / 2.0;
    let cy = (min_y + max_y) / 2.0;
    let center = rect.center() + pan;
    let map = |x: f64, y: f64| {
        egui::pos2(
            center.x + ((x - cx) as f32) * scale,
            center.y - ((y - cy) as f32) * scale,
        )
    };

    if scale > 4.0 {
        let gx0 = min_x.floor() as i32;
        let gx1 = max_x.ceil() as i32;
        let gy0 = min_y.floor() as i32;
        let gy1 = max_y.ceil() as i32;
        let grid = Stroke::new(1.0, palette.border.gamma_multiply(0.6));
        for x in gx0..=gx1 {
            painter.line_segment([map(x as f64, min_y), map(x as f64, max_y)], grid);
        }
        for y in gy0..=gy1 {
            painter.line_segment([map(min_x, y as f64), map(max_x, y as f64)], grid);
        }
    }
    let axis = Stroke::new(1.5, palette.muted);
    painter.line_segment([map(min_x, 0.0), map(max_x, 0.0)], axis);
    painter.line_segment([map(0.0, min_y), map(0.0, max_y)], axis);

    for s in segments {
        let color = actor_color(s.color, palette.call);
        painter.line_segment([map(s.x1, s.y1), map(s.x2, s.y2)], Stroke::new(2.0, color));
    }
    for c in captions {
        let size = (c.cell_width * 1.6 * scale as f64).clamp(8.0, 48.0) as f32;
        painter.text(
            map(c.x, c.y) + Vec2::new(2.0, -2.0),
            Align2::LEFT_BOTTOM,
            &c.text,
            FontId::proportional(size),
            actor_color(c.color, palette.text),
        );
    }
    let p = map(current.0, current.1);
    painter.circle_filled(p, 5.0, palette.accent);
    if pen_down {
        painter.circle_stroke(p, 8.0, Stroke::new(1.0, palette.accent));
    }
    painter.text(
        rect.left_top() + Vec2::new(8.0, 6.0),
        Align2::LEFT_TOP,
        format!("Масштаб: {:.0}%", zoom * 100.0),
        FontId::proportional(11.0),
        palette.muted,
    );
}

fn vector_to_svg(
    segments: &[kumir_core::actors::Segment],
    captions: &[kumir_core::actors::Caption],
    width: f32,
    height: f32,
) -> String {
    let mut min_x = 0.0f64;
    let mut max_x = 0.0f64;
    let mut min_y = 0.0f64;
    let mut max_y = 0.0f64;
    for s in segments {
        min_x = min_x.min(s.x1).min(s.x2);
        max_x = max_x.max(s.x1).max(s.x2);
        min_y = min_y.min(s.y1).min(s.y2);
        max_y = max_y.max(s.y1).max(s.y2);
    }
    let pad = 10.0;
    let range_x = (max_x - min_x).max(1.0);
    let range_y = (max_y - min_y).max(1.0);
    let scale = ((width - 2.0 * pad) as f64 / range_x).min((height - 2.0 * pad) as f64 / range_y);
    let ox = pad as f64 - min_x * scale;
    let oy = pad as f64 + max_y * scale;
    let mut out = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\">\n<rect width=\"100%\" height=\"100%\" fill=\"white\"/>\n"
    );
    for s in segments {
        let (r, g, b, a) = s.color.to_rgba_tuple();
        out.push_str(&format!(
            "<line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" stroke=\"rgba({r},{g},{b},{:.2})\" stroke-width=\"1.5\"/>\n",
            ox + s.x1 * scale,
            oy - s.y1 * scale,
            ox + s.x2 * scale,
            oy - s.y2 * scale,
            a as f32 / 255.0
        ));
    }
    for c in captions {
        let (r, g, b, a) = c.color.to_rgba_tuple();
        let size = (c.cell_width * 1.5 * scale).max(4.0);
        let text = c
            .text
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        out.push_str(&format!(
            "<text x=\"{:.2}\" y=\"{:.2}\" font-family=\"monospace\" font-size=\"{size:.2}\" fill=\"rgba({r},{g},{b},{:.2})\">{text}</text>\n",
            ox + c.x * scale,
            oy - c.y * scale,
            a as f32 / 255.0
        ));
    }
    out.push_str("</svg>\n");
    out
}

fn value_text(value: &Value) -> String {
    match value {
        Value::Undefined => "—".to_string(),
        Value::Int(v) => v.to_string(),
        Value::Real(v) => format!("{v}"),
        Value::Bool(v) => {
            if *v {
                "да".to_string()
            } else {
                "нет".to_string()
            }
        }
        Value::Char(c) => c.to_string(),
        Value::Str(s) => s.clone(),
        Value::File(_) => "[файл]".to_string(),
        Value::Array(a) => format!("[таблица {}]", a.element_count()),
        Value::Record(_) => "[запись]".to_string(),
    }
}

fn draw_turtle(
    ui: &mut Ui,
    rect: Rect,
    turtle: &kumir_core::actors::TurtleState,
    palette: &Palette,
    zoom: f32,
    pan: Vec2,
) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(6), palette.water);
    let side = (rect.width().min(rect.height()) * 0.86).max(20.0);
    let arena = Rect::from_center_size(rect.center() + pan, Vec2::splat(side * zoom));
    painter.rect_filled(arena, CornerRadius::ZERO, palette.sand);
    painter.rect_stroke(
        arena,
        CornerRadius::ZERO,
        Stroke::new(2.0, palette.water),
        egui::StrokeKind::Inside,
    );
    if arena.width() < 4.0 {
        return;
    }
    let scale = arena.width() / kumir_core::actors::ARENA_SIZE as f32;
    let map = |x: f64, y: f64| {
        egui::pos2(
            arena.center().x + x as f32 * scale,
            arena.center().y - y as f32 * scale,
        )
    };
    // Grid every 50 turtle units.
    if scale * 50.0 > 14.0 {
        let grid = Stroke::new(1.0, palette.muted.gamma_multiply(0.25));
        for i in -5..=5 {
            let v = i as f64 * 50.0;
            painter.line_segment([map(v, -250.0), map(v, 250.0)], grid);
            painter.line_segment([map(-250.0, v), map(250.0, v)], grid);
        }
    }
    for segment in &turtle.segments {
        painter.line_segment(
            [map(segment.x1, segment.y1), map(segment.x2, segment.y2)],
            Stroke::new(2.0, actor_color(segment.color, palette.call)),
        );
    }
    let p = map(turtle.x, turtle.y);
    let angle = turtle.angle.to_radians() as f32;
    let size = (10.0 * zoom).clamp(5.0, 26.0);
    let forward = Vec2::angled(-angle);
    let right = Vec2::new(-forward.y, forward.x);
    let body = vec![
        p + forward * size,
        p - forward * size * 0.6 + right * size * 0.6,
        p - forward * size * 0.6 - right * size * 0.6,
    ];
    painter.add(egui::Shape::convex_polygon(
        body,
        palette.accent,
        Stroke::new(1.0, palette.bg),
    ));
    if turtle.pen_down {
        painter.circle_stroke(p, 3.0, Stroke::new(1.0, palette.text));
    }
    painter.text(
        rect.left_top() + Vec2::new(8.0, 6.0),
        Align2::LEFT_TOP,
        format!(
            "Арена 500×500 · хвост {} · {}°",
            if turtle.pen_down {
                "опущен"
            } else {
                "поднят"
            },
            turtle.angle.round() as i64
        ),
        FontId::proportional(11.0),
        palette.muted,
    );
}

fn draw_vodoley(
    ui: &mut Ui,
    rect: Rect,
    state: &kumir_core::actors::VodoleyState,
    palette: &Palette,
) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(6), palette.bg);
    let max_capacity = state.capacity.iter().copied().max().unwrap_or(1).max(1) as f32;
    let top_area = 42.0;
    let usable_h = (rect.height() - top_area - 34.0).max(30.0);
    let usable_w = rect.width() - 32.0;
    let gap = usable_w / 3.0;
    let vial_w = (gap * 0.55).min(usable_h * 0.45).max(12.0);
    painter.text(
        egui::pos2(rect.center().x, rect.top() + 14.0),
        Align2::CENTER_CENTER,
        format!("Отмерить: {}", state.target),
        FontId::proportional(14.0),
        if state.solved() {
            palette.ok
        } else {
            palette.text
        },
    );
    for i in 0..3 {
        let cx = rect.left() + 16.0 + gap * (i as f32 + 0.5);
        let base_y = rect.bottom() - 34.0;
        let capacity = state.capacity[i].max(0) as f32;
        let full_h = usable_h * (capacity / max_capacity);
        let vial = Rect::from_min_size(
            egui::pos2(cx - vial_w / 2.0, base_y - full_h),
            Vec2::new(vial_w, full_h),
        );
        painter.rect_filled(vial, CornerRadius::same(3), palette.panel_alt);
        painter.rect_stroke(
            vial,
            CornerRadius::same(3),
            Stroke::new(1.5, palette.border),
            egui::StrokeKind::Inside,
        );
        if capacity > 0.0 {
            let fill = state.fill[i].clamp(0, state.capacity[i]) as f32;
            let fill_h = full_h * (fill / capacity);
            if fill_h > 0.0 {
                let water = Rect::from_min_size(
                    egui::pos2(vial.left(), vial.bottom() - fill_h),
                    Vec2::new(vial_w, fill_h),
                );
                painter.rect_filled(water, CornerRadius::same(2), palette.water);
            }
            if state.target > 0 && state.target <= state.capacity[i] {
                let y = vial.bottom() - full_h * (state.target as f32 / capacity);
                let mark = Rect::from_min_max(
                    egui::pos2(vial.left() - 4.0, y - 1.0),
                    egui::pos2(vial.right() + 4.0, y + 1.0),
                );
                painter.rect_filled(mark, CornerRadius::ZERO, palette.ok);
            }
        }
        painter.text(
            egui::pos2(cx, base_y + 14.0),
            Align2::CENTER_CENTER,
            format!(
                "{} · {}",
                kumir_core::actors::vodoley::VIAL_NAMES[i],
                state.capacity[i]
            ),
            FontId::proportional(12.0),
            palette.muted,
        );
        painter.text(
            egui::pos2(cx, vial.top() - 10.0),
            Align2::CENTER_CENTER,
            state.fill[i].to_string(),
            FontId::proportional(12.0),
            palette.text,
        );
    }
}

fn draw_grasshopper(
    ui: &mut Ui,
    rect: Rect,
    state: &kumir_core::actors::GrasshopperState,
    palette: &Palette,
    zoom: f32,
    pan: Vec2,
) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(6), palette.bg);
    let axis_y = rect.center().y + pan.y;
    let scale = 40.0 * zoom;
    let center_x = rect.center().x + pan.x;
    let map = |x: i64| center_x + x as f32 * scale;
    painter.line_segment(
        [
            egui::pos2(rect.left(), axis_y),
            egui::pos2(rect.right(), axis_y),
        ],
        Stroke::new(1.5, palette.muted),
    );
    let step = if scale >= 28.0 {
        1
    } else if scale >= 10.0 {
        5
    } else {
        10
    };
    let start = ((rect.left() - center_x) / scale).floor() as i64 / step * step - step;
    let end = ((rect.right() - center_x) / scale).ceil() as i64 + step;
    let mut x = start;
    while x <= end {
        let px = map(x);
        painter.line_segment(
            [egui::pos2(px, axis_y - 4.0), egui::pos2(px, axis_y + 4.0)],
            Stroke::new(1.0, palette.border),
        );
        painter.text(
            egui::pos2(px, axis_y + 16.0),
            Align2::CENTER_TOP,
            x.to_string(),
            FontId::proportional(10.0),
            palette.muted,
        );
        x += step;
    }
    if let Some((left, right)) = state.borders {
        for border in [left, right] {
            let px = map(border);
            painter.line_segment(
                [
                    egui::pos2(px, rect.top() + 20.0),
                    egui::pos2(px, rect.bottom() - 20.0),
                ],
                Stroke::new(2.0, palette.error),
            );
        }
    }
    for flag in &state.flags {
        let px = map(*flag);
        let reached = state.pos == *flag;
        let color = if reached { palette.ok } else { palette.warning };
        painter.line_segment(
            [egui::pos2(px, axis_y - 4.0), egui::pos2(px, axis_y - 30.0)],
            Stroke::new(1.5, color),
        );
        painter.add(egui::Shape::convex_polygon(
            vec![
                egui::pos2(px, axis_y - 30.0),
                egui::pos2(px + 14.0, axis_y - 25.0),
                egui::pos2(px, axis_y - 20.0),
            ],
            color,
            Stroke::NONE,
        ));
    }
    for point in &state.painted {
        painter.circle_filled(egui::pos2(map(*point), axis_y + 8.0), 4.0, palette.call);
    }
    let px = map(state.pos);
    painter.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(px, axis_y - 14.0),
            egui::pos2(px + 10.0, axis_y),
            egui::pos2(px, axis_y + 10.0),
            egui::pos2(px - 10.0, axis_y),
        ],
        palette.accent,
        Stroke::new(1.0, palette.bg),
    ));
    painter.text(
        rect.left_top() + Vec2::new(8.0, 6.0),
        Align2::LEFT_TOP,
        format!(
            "Позиция {} · вперёд {} · назад {}",
            state.pos, state.forward, state.backward
        ),
        FontId::proportional(11.0),
        palette.muted,
    );
}

fn draw_painter(
    ui: &mut Ui,
    rect: Rect,
    state: &kumir_core::actors::PainterState,
    palette: &Palette,
    zoom: f32,
    pan: Vec2,
    cache: &mut Option<(u64, egui::TextureHandle)>,
) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(6), palette.bg);
    let page_w = state.page.width.max(1) as f32;
    let page_h = state.page.height.max(1) as f32;
    let base = ((rect.width() - 24.0) / page_w)
        .min((rect.height() - 24.0) / page_h)
        .max(0.02);
    let scale = base * zoom;
    let size = Vec2::new(page_w * scale, page_h * scale);
    let page = Rect::from_center_size(rect.center() + pan, size);
    if cache.as_ref().map(|(v, _)| *v) != Some(state.version) {
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [state.page.width as usize, state.page.height as usize],
            &state.rgba_bytes(),
        );
        let texture =
            ui.ctx()
                .load_texture("kumir_painter_sheet", image, egui::TextureOptions::NEAREST);
        *cache = Some((state.version, texture));
    }
    if let Some((_, texture)) = cache.as_ref() {
        painter.image(
            texture.id(),
            page,
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    painter.rect_stroke(
        page,
        CornerRadius::ZERO,
        Stroke::new(1.0, palette.border),
        egui::StrokeKind::Outside,
    );
    let map = |x: i32, y: i32| {
        egui::pos2(
            page.left() + x as f32 * scale,
            page.top() + y as f32 * scale,
        )
    };
    for text in &state.texts {
        let font_size = (text.size as f32 * scale).clamp(4.0, 72.0);
        painter.text(
            map(text.x, text.y),
            Align2::LEFT_TOP,
            &text.text,
            FontId::proportional(font_size),
            actor_color(text.color, palette.text),
        );
    }
    let cursor = map(state.pos.0, state.pos.1);
    if page.contains(cursor) {
        painter.circle_stroke(cursor, 3.0, Stroke::new(1.0, palette.accent));
    }
    painter.text(
        rect.left_top() + Vec2::new(8.0, 6.0),
        Align2::LEFT_TOP,
        format!(
            "Лист {}×{} · масштаб {:.0}%",
            state.page.width,
            state.page.height,
            zoom * 100.0
        ),
        FontId::proportional(11.0),
        palette.muted,
    );
}

fn actor_color(s: kumir_core::actors::Rgba, fallback: Color32) -> Color32 {
    let _ = fallback;
    Color32::from_rgba_unmultiplied(s.r, s.g, s.b, s.a)
}

#[cfg(target_os = "android")]
fn clipboard_get() -> Option<String> {
    crate::android_platform::clipboard_text()
}

#[cfg(not(target_os = "android"))]
fn clipboard_get() -> Option<String> {
    arboard::Clipboard::new()
        .and_then(|mut c| c.get_text())
        .ok()
}

#[cfg(target_os = "android")]
fn clipboard_set(text: &str) {
    crate::android_platform::set_clipboard_text(text);
}

#[cfg(not(target_os = "android"))]
fn clipboard_set(text: &str) {
    let _ = arboard::Clipboard::new().and_then(|mut c| c.set_text(text.to_string()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actor_drawing_smoke_tests() {
        egui::__run_test_ui(|ui| {
            ui.set_min_size(egui::vec2(640.0, 320.0));
            let rect = Rect::from_min_size(ui.cursor().min, egui::vec2(640.0, 300.0));
            draw_robot(
                ui,
                rect,
                &RobotField::empty(7, 7),
                &Palette::dark(),
                1.0,
                Vec2::ZERO,
            );
        });
        egui::__run_test_ui(|ui| {
            ui.set_min_size(egui::vec2(640.0, 320.0));
            let rect = Rect::from_min_size(ui.cursor().min, egui::vec2(640.0, 300.0));
            draw_vector_actor(
                ui,
                rect,
                &[],
                &[],
                (0.0, 0.0),
                &Palette::dark(),
                true,
                1.0,
                Vec2::ZERO,
            );
        });
    }

    #[test]
    fn signature_hint_for_user_algorithm() {
        let src = "Сумма(1, ";
        let program_src = "алг цел Сумма(цел a, цел b)\nнач\nзнач := a + b\nкон\n";
        let mut tab = EditorTab::new(7, None, src.to_string(), kumir_core::KumFile::from_str(""));
        tab.outline = Some(kumir_core::parse(program_src).unwrap());
        let cursor = src.chars().position(|c| c == ',').unwrap() + 2;
        let (signature, active) = call_signature_at(&tab, cursor).unwrap();
        assert!(signature.starts_with("Сумма(цел a, цел b)"), "{signature}");
        assert_eq!(active, 1);
    }

    #[test]
    fn editor_and_completion_smoke() {
        let mut tab = EditorTab::new(
            42,
            None,
            "алг\nнач\nвы\nкон\n".to_string(),
            kumir_core::KumFile::from_str(""),
        );
        tab.completion.open = true;
        tab.completion.items = completion::suggest("вы", None);
        tab.completion.replace = (8, 10);
        tab.completion.prefix = "вы".to_string();
        let mut breakpoints = BTreeSet::new();
        egui::__run_test_ui(|ui| {
            ui.set_min_size(egui::vec2(720.0, 360.0));
            let _ = draw_editor(
                ui,
                &mut tab,
                &Palette::dark(),
                14.0,
                true,
                0,
                &mut breakpoints,
                &Settings::default(),
                false,
            );
        });
        assert!(tab.text.contains("вы"));
    }

    #[test]
    fn field_tools_and_wall_mirroring() {
        let mut field = RobotField::empty(3, 3);
        toggle_wall(&mut field, 0, 0, kumir_core::actors::RIGHT_WALL);
        assert!(field.cell(0, 0).unwrap().walls & kumir_core::actors::RIGHT_WALL != 0);
        assert!(field.cell(1, 0).unwrap().walls & kumir_core::actors::LEFT_WALL != 0);
        toggle_wall(&mut field, 0, 0, kumir_core::actors::RIGHT_WALL);
        assert!(field.cell(0, 0).unwrap().walls & kumir_core::actors::RIGHT_WALL == 0);
        assert!(field.cell(1, 0).unwrap().walls & kumir_core::actors::LEFT_WALL == 0);

        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(300.0, 300.0));
        let (gx, gy, cell) = robot_layout(rect, &field, 1.0, Vec2::ZERO);
        let left_edge = egui::pos2(gx + cell + 4.0, gy + cell + cell * 0.5);
        let target = edit_target(
            rect,
            &field,
            1.0,
            Vec2::ZERO,
            left_edge,
            FieldTool::Wall,
            false,
        )
        .unwrap();
        assert_eq!(target.x, 1);
        assert_eq!(target.y, 1);
        assert_eq!(
            target.kind,
            EditTargetKind::Wall(kumir_core::actors::LEFT_WALL)
        );

        let center = egui::pos2(gx + cell * 1.5, gy + cell * 1.5);
        let color = edit_target(
            rect,
            &field,
            1.0,
            Vec2::ZERO,
            center,
            FieldTool::Color,
            false,
        )
        .unwrap();
        assert_eq!(color.kind, EditTargetKind::Color);
        let mark =
            edit_target(rect, &field, 1.0, Vec2::ZERO, center, FieldTool::Wall, true).unwrap();
        assert_eq!(mark.kind, EditTargetKind::Mark);

        let robot = apply_target(
            &mut field,
            EditTarget {
                x: 2,
                y: 1,
                kind: EditTargetKind::Robot,
            },
        );
        assert!(robot.contains("Робот"));
        assert_eq!((field.start_x, field.start_y), (2, 1));
    }

    #[test]
    fn layout_job_survives_stale_utf8_offsets() {
        let text = "алг Тест\nнач\nкон\n";
        // Byte 1 is inside the two-byte 'а'; old spans/matches may contain it.
        let spans = vec![kumir_core::highlight::HighlightSpan {
            range: 1..5,
            kind: kumir_core::highlight::HighlightKind::Keyword,
        }];
        let job = build_layout_job(
            text,
            &spans,
            &[(3, 5), (5, 9)],
            FontId::monospace(14.0),
            &Palette::dark(),
            1,
            &BTreeSet::new(),
        );
        for section in &job.sections {
            assert!(text.is_char_boundary(section.byte_range.start.0));
            assert!(text.is_char_boundary(section.byte_range.end.0));
        }
    }

    #[test]
    fn layout_job_covers_text() {
        let text = "алг Тест\nнач\nвывод 1\nкон\n";
        let spans = kumir_core::highlight::highlight(text);
        let job = build_layout_job(
            text,
            &spans,
            &[(4, 8)],
            FontId::monospace(14.0),
            &Palette::dark(),
            3,
            &BTreeSet::new(),
        );
        assert_eq!(job.text, text);
        assert!(job.sections.first().unwrap().byte_range.start == egui::text::ByteIndex(0));
        assert_eq!(
            job.sections.last().unwrap().byte_range.end,
            egui::text::ByteIndex(text.len())
        );
    }
}
