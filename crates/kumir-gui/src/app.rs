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
use kumir_core::{load_program, parse_source};

use crate::completion;
use crate::file_dialog::{DialogOutcome, FileDialog, Mode};
use crate::runner::{self, RunEvent, RunHandle};
use crate::theme::{self, Palette};

const DEFAULT_PROGRAM: &str = "\u{feff}алг Моя программа\nнач\n    | Напишите здесь свою программу\n    вывод \"Привет, мир!\", нс\nкон\n";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BottomTab {
    Output,
    Errors,
    Reference,
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
    ZoomIn,
    ZoomOut,
    ToggleTheme,
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
    palette: Palette,
    dark: bool,
    font_size: f32,
    show_line_numbers: bool,

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
    field_tool: FieldTool,
    field_undo: Vec<RobotField>,
    field_redo: Vec<RobotField>,
    field_initial: Option<RobotField>,
    field_before_run: Option<RobotField>,
    field_path_before_run: Option<PathBuf>,
    field_dragging_robot: bool,
    field_last_target: Option<EditTarget>,
    field_save_dialog: Option<FileDialog>,
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

    show_settings: bool,
    show_about: bool,
    show_help: bool,

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
        let palette = Palette::dark();
        // На Android cwd = «/», поэтому по умолчанию берём доступный каталог
        // (Документы при наличии разрешения, иначе приватный каталог приложения).
        let cwd = crate::android_platform::storage_dir()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        let mut app = Self {
            palette,
            dark: true,
            font_size: 14.0,
            show_line_numbers: true,
            tabs: Vec::new(),
            active: 0,
            next_id: 1,
            output: String::new(),
            errors: Vec::new(),
            bottom_tab: BottomTab::Output,
            show_bottom: true,
            show_left: true,
            show_right: true,
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
            field_tool: FieldTool::Color,
            field_undo: Vec::new(),
            field_redo: Vec::new(),
            field_initial: None,
            field_before_run: None,
            field_path_before_run: None,
            field_dragging_robot: false,
            field_last_target: None,
            field_save_dialog: None,
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
            show_settings: false,
            show_about: false,
            show_help: false,
            zoom_accum: 0.0,
            field_rect: None,
            pinch_field: false,
            actions: Vec::new(),
        };
        theme::apply_style(&cc.egui_ctx, &app.palette, app.font_size);
        app.new_tab();
        if let Some(arg) = std::env::args().nth(1) {
            let p = PathBuf::from(arg);
            if p.is_file() {
                app.open_path(&p);
            }
        }
        app
    }

    // ------------------------------------------------------------------
    // Files and tabs
    // ------------------------------------------------------------------

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
        if desired.kind == ActorKind::None {
            return;
        }
        let mut keep_field_path = false;
        if desired.kind == ActorKind::Robot {
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
                self.active = self.tabs.len() - 1;
                self.explorer_dir = canon
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| self.explorer_dir.clone());
                self.explorer_dirty = true;
                self.status = format!("Открыт {}", canon.display());
                self.recent.retain(|p| p != &canon);
                self.recent.insert(0, canon.clone());
                self.recent.truncate(12);
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
                self.bottom_tab = BottomTab::Errors;
                self.show_bottom = true;
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
            self.show_bottom = true;
            self.status = "Проверка завершена: ошибок нет".to_string();
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
        if self.breakpoints.is_empty() {
            // Nothing special.
        }
        self.output.clear();
        self.errors.clear();
        self.output.push_str(&format!("=== Запуск: {title} ===\n"));
        self.bottom_tab = BottomTab::Output;
        self.show_bottom = true;
        self.awaiting_input = false;
        self.run_line = 0;
        self.variables.clear();
        self.run_path = path.clone();
        // Keep the field that is currently shown (including manual wall/colour
        // edits). Recreating it here was the reason the layout reset on every
        // run.
        let desired = ActorState::for_modules(&program.modules);
        let current = self.actor.lock().ok().map(|a| a.clone());
        let actor_state = match current {
            Some(cur) if desired.kind != ActorKind::None && cur.kind == desired.kind => cur,
            _ => {
                let mut fresh = desired;
                if fresh.kind == ActorKind::Robot {
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
        let has_actor = actor_state.kind != ActorKind::None;
        if actor_state.kind == ActorKind::Robot {
            self.field_before_run = Some(actor_state.robot.clone());
            self.field_path_before_run = self.field_path.clone();
        }
        self.actor = Arc::new(Mutex::new(actor_state));
        if has_actor {
            self.show_right = true;
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
                    self.show_bottom = true;
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
                            self.bottom_tab = BottomTab::Errors;
                            self.show_bottom = true;
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
                    if let Some(text) = read_clipboard() {
                        ctx.input_mut(|i| i.events.push(egui::Event::Paste(text)));
                    }
                }
                Action::ZoomIn => {
                    self.font_size = (self.font_size + 1.0).min(26.0);
                    theme::apply_style(ctx, &self.palette, self.font_size);
                }
                Action::ZoomOut => {
                    self.font_size = (self.font_size - 1.0).max(9.0);
                    theme::apply_style(ctx, &self.palette, self.font_size);
                }
                Action::ToggleTheme => {
                    self.dark = !self.dark;
                    self.palette = if self.dark {
                        Palette::dark()
                    } else {
                        Palette::light()
                    };
                    theme::apply_style(ctx, &self.palette, self.font_size);
                }
                Action::ToggleLeft => self.show_left = !self.show_left,
                Action::ToggleRight => self.show_right = !self.show_right,
                Action::ToggleBottom => self.show_bottom = !self.show_bottom,
                Action::ShowHelp => self.show_help = true,
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
        let mut actions = Vec::new();
        egui::Panel::top("menu")
            .frame(
                Frame::NONE
                    .fill(palette.panel)
                    .inner_margin(Margin::symmetric(8, 4))
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
                        ui.separator();
                        if ui.button("Найти и заменить…\tCtrl+F").clicked() {
                            actions.push(Action::Find);
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
                        if ui.button("Тёмная тема").clicked() {
                            actions.push(Action::ToggleTheme);
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Проводник").clicked() {
                            actions.push(Action::ToggleLeft);
                            ui.close();
                        }
                        if ui.button("Структура программы").clicked() {
                            actions.push(Action::ToggleRight);
                            ui.close();
                        }
                        if ui.button("Панель вывода").clicked() {
                            actions.push(Action::ToggleBottom);
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Увеличить шрифт").clicked() {
                            actions.push(Action::ZoomIn);
                            ui.close();
                        }
                        if ui.button("Уменьшить шрифт").clicked() {
                            actions.push(Action::ZoomOut);
                            ui.close();
                        }
                    });
                    ui.menu_button("Исполнитель", |ui| {
                        if ui.button("Показать исполнителя").clicked() {
                            actions.push(Action::ShowActor);
                            ui.close();
                        }
                        if ui.button("Открыть поле Робота (.fil)…").clicked() {
                            actions.push(Action::OpenField);
                            ui.close();
                        }
                    });
                    ui.menu_button("Справка", |ui| {
                        if ui.button("Язык Кумир — краткий справочник").clicked()
                        {
                            actions.push(Action::ShowHelp);
                            ui.close();
                        }
                        if ui.button("Настройки").clicked() {
                            actions.push(Action::ShowSettings);
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
                    .fill(palette.panel_alt)
                    .inner_margin(Margin::symmetric(8, 4))
                    .stroke(Stroke::new(1.0, palette.border)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let run_btn =
                        egui::Button::new(RichText::new("▶ Запуск").color(Color32::WHITE).strong())
                            .fill(Color32::from_rgb(46, 125, 50));
                    if ui.add(run_btn).on_hover_text("Запустить (F5)").clicked() {
                        actions.push(Action::Run);
                    }
                    let stop_btn = egui::Button::new("■").fill(Color32::from_rgb(150, 45, 45));
                    if ui
                        .add(stop_btn)
                        .on_hover_text("Остановить (Shift+F5)")
                        .clicked()
                    {
                        actions.push(Action::Stop);
                    }
                    if ui.button("⏸").on_hover_text("Пауза (F6)").clicked() {
                        actions.push(Action::Pause);
                    }
                    if ui.button("⏭").on_hover_text("Шаг (F10)").clicked() {
                        actions.push(Action::Step);
                    }
                    ui.separator();
                    if ui
                        .button("✓ Проверить")
                        .on_hover_text("Проверить синтаксис (Ctrl+B)")
                        .clicked()
                    {
                        actions.push(Action::Check);
                    }
                    if ui
                        .button("🎓 Оценить")
                        .on_hover_text("Запустить скрытый алгоритм @тестирование")
                        .clicked()
                    {
                        actions.push(Action::Evaluate);
                    }
                    if ui.button("🔴 Точка останова").on_hover_text("F9").clicked() {
                        actions.push(Action::ToggleBreakpoint);
                    }
                    ui.separator();
                    if ui
                        .button("💾")
                        .on_hover_text("Сохранить (Ctrl+S)")
                        .clicked()
                    {
                        actions.push(Action::Save);
                    }
                    if ui.button("📂").on_hover_text("Открыть (Ctrl+O)").clicked() {
                        actions.push(Action::OpenDialog);
                    }
                    ui.separator();
                    if ui.button("🔍").on_hover_text("Найти (Ctrl+F)").clicked() {
                        actions.push(Action::Find);
                    }
                    if ui.button("🤖").on_hover_text("Исполнитель (F7)").clicked() {
                        actions.push(Action::ShowActor);
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let state = if self.run.is_some() {
                            if self
                                .run
                                .as_ref()
                                .map(|r| r.shared.is_paused())
                                .unwrap_or(false)
                            {
                                "⏸ Пауза"
                            } else {
                                "▶ Выполняется"
                            }
                        } else {
                            "● Готово"
                        };
                        ui.label(RichText::new(state).color(palette.muted).small());
                        if let Some(tab) = self.active_tab() {
                            ui.label(
                                RichText::new(format!(
                                    "{}  •  строка {}",
                                    tab.title, tab.cursor_line
                                ))
                                .color(palette.muted)
                                .small(),
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
            .min_size(170.0)
            .frame(
                Frame::NONE
                    .fill(palette.panel)
                    .inner_margin(Margin::same(8))
                    .stroke(Stroke::new(1.0, palette.border)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("ПРОВОДНИК")
                            .small()
                            .strong()
                            .color(palette.muted),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("⟳").on_hover_text("Обновить").clicked() {
                            self.explorer_dirty = true;
                        }
                        if ui.small_button("↑").on_hover_text("Выше").clicked() {
                            if let Some(p) = self.explorer_dir.parent() {
                                self.explorer_dir = p.to_path_buf();
                                self.explorer_dirty = true;
                            }
                        }
                    });
                });
                ui.label(
                    RichText::new(self.explorer_dir.to_string_lossy())
                        .small()
                        .color(palette.muted),
                );
                ui.separator();
                ScrollArea::vertical()
                    .id_salt("explorer_list")
                    .auto_shrink([false, false])
                    .max_height(ui.available_height() - 150.0)
                    .show(ui, |ui| {
                        let entries = self.explorer_entries.clone();
                        for path in entries {
                            let is_dir = path.is_dir();
                            let name = path
                                .file_name()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_default();
                            let label = if is_dir {
                                format!("📁  {name}")
                            } else if path
                                .extension()
                                .and_then(|e| e.to_str())
                                .map(|e| e.eq_ignore_ascii_case("kod"))
                                .unwrap_or(false)
                            {
                                format!("⚙  {name}")
                            } else if path
                                .extension()
                                .and_then(|e| e.to_str())
                                .map(|e| e.eq_ignore_ascii_case("fil"))
                                .unwrap_or(false)
                            {
                                format!("▦  {name}")
                            } else {
                                format!("📄  {name}")
                            };
                            let selected = self
                                .active_tab()
                                .and_then(|t| t.path.as_ref())
                                .map(|p| p == &path)
                                .unwrap_or(false);
                            let resp = ui.selectable_label(selected, label);
                            if resp.clicked() {
                                if is_dir {
                                    self.explorer_dir = path.clone();
                                    self.explorer_dirty = true;
                                } else if path
                                    .extension()
                                    .and_then(|e| e.to_str())
                                    .map(|e| e.eq_ignore_ascii_case("kum"))
                                    .unwrap_or(false)
                                {
                                    actions.push(Action::OpenPath(path.clone()));
                                } else if path
                                    .extension()
                                    .and_then(|e| e.to_str())
                                    .map(|e| e.eq_ignore_ascii_case("fil"))
                                    .unwrap_or(false)
                                {
                                    actions.push(Action::LoadField(path.clone()));
                                }
                            }
                        }
                    });
                ui.separator();
                ui.label(
                    RichText::new("НЕДАВНИЕ")
                        .small()
                        .strong()
                        .color(palette.muted),
                );
                let recent = self.recent.clone();
                ScrollArea::vertical()
                    .id_salt("recent_list")
                    .auto_shrink([false, false])
                    .max_height(110.0)
                    .show(ui, |ui| {
                        if recent.is_empty() {
                            ui.label(RichText::new("Пока пусто").small().color(palette.muted));
                        }
                        for p in recent {
                            let name = p
                                .file_name()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_default();
                            if ui
                                .selectable_label(false, format!("◷  {name}"))
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
        let actions = Vec::new();
        let mut jump: Option<u32> = None;
        let mut remove_bp: Option<u32> = None;
        egui::Panel::right("structure")
            .resizable(true)
            .default_size(300.0)
            .min_size(210.0)
            .frame(
                Frame::NONE
                    .fill(palette.panel)
                    .inner_margin(Margin::same(8))
                    .stroke(Stroke::new(1.0, palette.border)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(self.right_tab == RightTab::Structure, "Структура")
                        .clicked()
                    {
                        self.right_tab = RightTab::Structure;
                    }
                    if ui
                        .selectable_label(self.right_tab == RightTab::Actor, "Исполнитель")
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
                ui.label(
                    RichText::new("СТРУКТУРА ПРОГРАММЫ")
                        .small()
                        .strong()
                        .color(palette.muted),
                );
                ui.separator();
                ScrollArea::vertical()
                    .id_salt("structure_scroll")
                    .auto_shrink([false, false])
                    .max_height(ui.available_height() * 0.58)
                    .show(ui, |ui| {
                        let Some(tab) = self.tabs.get(self.active) else {
                            return;
                        };
                        if !tab.outline_errors.is_empty() {
                            ui.colored_label(
                                palette.error,
                                format!("Ошибок: {}", tab.outline_errors.len()),
                            );
                            for e in tab.outline_errors.iter().take(8) {
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
                                ui.label(
                                    RichText::new(format!(
                                        "Используется: {}",
                                        program.modules.join(", ")
                                    ))
                                    .small()
                                    .color(palette.muted),
                                );
                            }
                            ui.label(RichText::new("Алгоритмы").strong());
                            for (i, alg) in program.algorithms.iter().enumerate() {
                                let prefix = if Some(i) == program.main {
                                    "★ "
                                } else {
                                    "алг "
                                };
                                let name = if alg.name.is_empty() {
                                    "<основная программа>"
                                } else {
                                    &alg.name
                                };
                                let label = format!("{prefix}{name}");
                                if ui
                                    .selectable_label(
                                        false,
                                        RichText::new(label).color(palette.text),
                                    )
                                    .on_hover_text(format!("Строка {}", alg.line))
                                    .clicked()
                                {
                                    jump = Some(alg.line);
                                }
                            }
                            if !program.globals.is_empty() {
                                ui.add_space(6.0);
                                ui.label(RichText::new("Глобальные величины").strong());
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
                    ui.label(
                        RichText::new("ПЕРЕМЕННЫЕ")
                            .small()
                            .strong()
                            .color(palette.muted),
                    );
                    ScrollArea::vertical()
                        .id_salt("vars_scroll")
                        .auto_shrink([false, false])
                        .max_height(160.0)
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
                ui.label(
                    RichText::new("ТОЧКИ ОСТАНОВА")
                        .small()
                        .strong()
                        .color(palette.muted),
                );
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
                        if ui.small_button("×").clicked() {
                            remove_bp = Some(line);
                        }
                        if ui
                            .selectable_label(false, format!("строка {line}"))
                            .clicked()
                        {
                            jump = Some(line);
                        }
                    });
                }
                ui.add_space(4.0);
                ui.separator();
                ui.label(
                    RichText::new("СОСТОЯНИЕ")
                        .small()
                        .strong()
                        .color(palette.muted),
                );
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
        self.actions.extend(actions);
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
                                ui.label(RichText::new("●").small().color(palette.warning));
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
            .unwrap_or_else(|| "field.fil".to_string());
        self.field_save_dialog = Some(FileDialog::with_extension(Mode::Save, start, name, "fil"));
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
        let is_robot = matches!(state.kind, ActorKind::Robot);
        let edit_on = is_robot && self.actor_edit && !running;

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

        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(format!("Исполнитель: {}", state.kind.russian_name()))
                    .strong()
                    .color(palette.text),
            );
            if is_robot {
                ui.separator();
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
                    .add_enabled(!running, egui::Button::new("Применить размер"))
                    .on_hover_text("Создать пустое поле указанного размера")
                    .clicked()
                {
                    apply_size = true;
                }
                if ui
                    .add_enabled(!running, egui::Button::new("📂 .fil"))
                    .on_hover_text("Открыть поле Робота")
                    .clicked()
                {
                    open_field = true;
                }
                if ui
                    .add_enabled(!running, egui::Button::new("💾 Сохранить .fil"))
                    .on_hover_text("Сохранить поле в файл")
                    .clicked()
                {
                    save_field = true;
                }
            }
        });

        ui.horizontal_wrapped(|ui| {
            if ui.button("➖").on_hover_text("Уменьшить масштаб").clicked() {
                self.actor_zoom = (self.actor_zoom / 1.25).clamp(0.2, 10.0);
            }
            if ui.button("➕").on_hover_text("Увеличить масштаб").clicked() {
                self.actor_zoom = (self.actor_zoom * 1.25).clamp(0.2, 10.0);
            }
            if ui
                .button("⤢ По размеру")
                .on_hover_text("Сбросить масштаб и сдвиг")
                .clicked()
            {
                self.actor_zoom = 1.0;
                self.actor_pan = Vec2::ZERO;
            }
            ui.add_sized(
                [48.0, 16.0],
                egui::Label::new(
                    RichText::new(format!("{:.0}%", self.actor_zoom * 100.0))
                        .small()
                        .color(palette.muted),
                )
                .truncate(),
            );
            if is_robot {
                ui.separator();
                if ui
                    .add_enabled(
                        !running,
                        egui::Checkbox::new(&mut self.actor_edit, "✎ Редактировать поле"),
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
                if ui
                    .add_enabled(!running, egui::Button::new("Снять закраску"))
                    .on_hover_text("Убрать только цвета, сохранив стены и метки")
                    .clicked()
                {
                    clear_colors = true;
                }
                if ui
                    .add_enabled(!running, egui::Button::new("Очистить всё"))
                    .on_hover_text("Сбросить цвета, стены, метки и символы")
                    .clicked()
                {
                    clear_field = true;
                }
                if ui
                    .add_enabled(!running, egui::Button::new("Робот на старт"))
                    .on_hover_text("Вернуть Робота в стартовую клетку")
                    .clicked()
                {
                    reset_robot = true;
                }
                let restore_label = if self.field_before_run.is_some() {
                    "⟲ До последнего запуска"
                } else {
                    "⟲ Исходное поле"
                };
                if ui
                    .add_enabled(
                        !running
                            && (self.field_before_run.is_some() || self.field_initial.is_some()),
                        egui::Button::new(restore_label),
                    )
                    .on_hover_text("Полностью вернуть поле к состоянию перед последним запуском")
                    .clicked()
                {
                    restore_initial = true;
                }
            }
            ui.separator();
            ui.checkbox(&mut self.actor_animate, "Анимация");
            ui.add_enabled(
                self.actor_animate,
                egui::DragValue::new(&mut self.actor_delay_ms)
                    .range(0..=150)
                    .suffix(" мс")
                    .speed(1.0),
            )
            .on_hover_text("Пауза после каждого шага исполнителя");
        });

        if is_robot {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Инструмент:").small().color(palette.muted));
                ui.label(
                    RichText::new("(изменения поля применяются сразу)")
                        .small()
                        .color(palette.muted),
                );
                let tools = [
                    (FieldTool::Color, "🎨 Цвет"),
                    (FieldTool::Wall, "🧱 Стена"),
                    (FieldTool::Mark, "📍 Метка"),
                    (FieldTool::Robot, "🤖 Робот"),
                    (FieldTool::Pan, "✋ Сдвиг"),
                ];
                for (tool, label) in tools {
                    if ui
                        .add_enabled(
                            !running && self.actor_edit,
                            egui::Button::selectable(self.field_tool == tool, label),
                        )
                        .clicked()
                    {
                        self.field_tool = tool;
                    }
                }
                ui.separator();
                if ui
                    .add_enabled(
                        !running && !self.field_undo.is_empty(),
                        egui::Button::new("↶"),
                    )
                    .on_hover_text("Отменить изменение поля (Ctrl+Z)")
                    .clicked()
                {
                    undo_field = true;
                }
                if ui
                    .add_enabled(
                        !running && !self.field_redo.is_empty(),
                        egui::Button::new("↷"),
                    )
                    .on_hover_text("Повторить изменение поля (Ctrl+Y)")
                    .clicked()
                {
                    redo_field = true;
                }
            });
        }

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
            state.kind = ActorKind::Robot;
            self.field_initial = Some(state.robot.clone());
            self.field_before_run = None;
            self.field_path_before_run = None;
            state.status = format!("Новое поле {}×{}", new_size.0, new_size.1);
            self.actor_zoom = 1.0;
            self.actor_pan = Vec2::ZERO;
            self.actor_edit = true;
            self.status = format!("Размер поля: {}×{}", new_size.0, new_size.1);
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
            state.status = "Закраска снята (стены и метки сохранены)".to_string();
            self.actor_edit = true;
        }
        if reset_robot {
            self.push_field_undo(&state.robot);
            state.robot.robot_x = state.robot.start_x;
            state.robot.robot_y = state.robot.start_y;
            state.status = "Робот возвращён на стартовую клетку".to_string();
            self.field_path = None;
            self.actor_edit = true;
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
                state.status = if from_run {
                    "Поле возвращено к состоянию перед последним запуском".to_string()
                } else {
                    "Поле возвращено к исходному состоянию".to_string()
                };
                self.actor_edit = true;
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

        ui.separator();
        if matches!(state.kind, ActorKind::None) {
            ui.add_space(8.0);
            ui.label(
                RichText::new(
                    "Подключите исполнителя строкой «использовать Робот», «использовать Черепаха» или «использовать Чертежник».",
                )
                .small()
                .color(palette.muted),
            );
            if let Ok(mut a) = self.actor.lock() {
                *a = state;
            }
            return;
        }

        let avail = ui.available_size();
        let viewport = Vec2::new(avail.x.max(40.0), avail.y.max(90.0));
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
        // Щипок двумя пальцами над полем — масштаб только поля (как на ПК).
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
        let cell_under = pointer.and_then(|p| cell_at(rect, &state.robot, zoom, pan, p));
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

        // Undo/redo from the buttons or keyboard while the field is hovered.
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

        // Dragging the robot works even outside the edit mode.
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

        match state.kind {
            ActorKind::Robot => draw_robot(ui, rect, &state.robot, &palette, zoom, pan),
            ActorKind::Turtle => draw_vector_actor(
                ui,
                rect,
                &state.turtle.segments,
                &[],
                (state.turtle.x, state.turtle.y),
                &palette,
                state.turtle.pen_down,
                zoom,
                pan,
            ),
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
                egui::pos2(rect.right() - 172.0, rect.top() + 8.0),
                Vec2::new(164.0, 20.0),
            );
            painter.rect_filled(badge, CornerRadius::same(4), palette.accent_weak);
            painter.rect_stroke(
                badge,
                CornerRadius::same(4),
                Stroke::new(1.0, palette.accent),
                egui::StrokeKind::Inside,
            );
            painter.text(
                badge.center(),
                Align2::CENTER_CENTER,
                "✎ РЕДАКТИРОВАНИЕ ПОЛЯ",
                FontId::proportional(10.5),
                palette.accent,
            );
        }

        let hint = if is_robot && edit_on {
            let tool_hint = match self.field_tool {
                FieldTool::Color => "🎨 ЛКМ — закрасить/очистить, тащите для рисования",
                FieldTool::Wall => "🧱 ЛКМ — поставить/убрать стену (призрак показывает место)",
                FieldTool::Mark => "📍 ЛКМ — поставить/убрать метку",
                FieldTool::Robot => "🤖 ЛКМ — поставить робота, тащите робота",
                FieldTool::Pan => "✋ Перетаскивание — сдвиг поля",
            };
            let cell = ghost_target
                .map(|t| format!(" · клетка ({}, {})", t.x, t.y))
                .unwrap_or_default();
            format!("{tool_hint}{cell} · колесо — масштаб · Ctrl+Z — отмена")
        } else {
            if is_robot {
                "Колесо — масштаб · перетаскивание робота — перемещение · фон — сдвиг · двойной клик — сброс".to_string()
            } else {
                "Колесо — масштаб · перетаскивание — сдвиг · двойной клик — сброс".to_string()
            }
        };
        ui.painter_at(rect).text(
            rect.left_bottom() + Vec2::new(8.0, -6.0),
            Align2::LEFT_BOTTOM,
            hint,
            FontId::proportional(11.0),
            palette.muted,
        );

        if !running {
            if let Ok(mut a) = self.actor.lock() {
                a.robot = state.robot;
                a.kind = state.kind;
                a.status = state.status;
            }
        }
    }

    fn bottom_panel(&mut self, ui: &mut Ui) {
        if !self.show_bottom {
            return;
        }
        let palette = self.palette;
        egui::Panel::bottom("output_panel")
            .resizable(true)
            .default_size(240.0)
            .min_size(90.0)
            .frame(
                Frame::NONE
                    .fill(palette.panel)
                    .inner_margin(Margin::same(6))
                    .stroke(Stroke::new(1.0, palette.border)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (tab, label) in [
                        (BottomTab::Output, "Вывод"),
                        (BottomTab::Errors, "Ошибки"),
                        (BottomTab::Reference, "Справка"),
                    ] {
                        if ui.selectable_label(self.bottom_tab == tab, label).clicked() {
                            self.bottom_tab = tab;
                        }
                    }
                    ui.separator();
                    if ui.small_button("Очистить").clicked() {
                        match self.bottom_tab {
                            BottomTab::Output => self.output.clear(),
                            BottomTab::Errors => self.errors.clear(),
                            BottomTab::Reference => {}
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.run.is_some() {
                            ui.label(
                                RichText::new("● программа работает")
                                    .small()
                                    .color(palette.ok),
                            );
                        }
                    });
                });
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
                                        .color(
                                            if text.is_empty() {
                                                palette.muted
                                            } else {
                                                palette.text
                                            },
                                        ),
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
                    BottomTab::Reference => {
                        ScrollArea::vertical()
                            .id_salt("reference_scroll")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.label(RichText::new(QUICK_REFERENCE).monospace().small());
                            });
                    }
                }
            });
    }

    // ------------------------------------------------------------------
    // Central editor
    // ------------------------------------------------------------------

    fn central(&mut self, ui: &mut Ui) {
        let palette = self.palette;
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
                if let Some(tab) = self.tabs.get_mut(active) {
                    let toggled = draw_editor(
                        ui,
                        tab,
                        &palette,
                        font_size,
                        show_line_numbers,
                        run_line,
                        &mut breakpoints,
                    );
                    if let Some(line) = toggled {
                        if !self.breakpoints.remove(&line) {
                            self.breakpoints.insert(line);
                        }
                        if let Some(run) = &self.run {
                            run.shared.set_breakpoints(self.breakpoints.clone());
                        }
                    }
                }
            });
    }

    fn tab_bar(&mut self, ui: &mut Ui) {
        let palette = self.palette;
        Frame::NONE
            .fill(palette.panel_alt)
            .inner_margin(Margin::symmetric(6, 3))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let mut close: Option<usize> = None;
                    let mut select: Option<usize> = None;
                    for (i, tab) in self.tabs.iter().enumerate() {
                        let active = i == self.active;
                        let text = if tab.dirty {
                            format!("● {}", tab.title)
                        } else {
                            tab.title.clone()
                        };
                        let btn = egui::Button::new(RichText::new(text).color(if active {
                            palette.text
                        } else {
                            palette.muted
                        }))
                        .fill(if active {
                            palette.bg
                        } else {
                            Color32::TRANSPARENT
                        })
                        .stroke(if active {
                            Stroke::new(1.0, palette.border)
                        } else {
                            Stroke::NONE
                        });
                        if ui.add(btn).clicked() {
                            select = Some(i);
                        }
                        if ui
                            .small_button("×")
                            .on_hover_text("Закрыть вкладку")
                            .clicked()
                        {
                            close = Some(i);
                        }
                        ui.add_space(2.0);
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .small_button("+")
                            .on_hover_text("Новая программа")
                            .clicked()
                        {
                            self.actions.push(Action::New);
                        }
                    });
                    if let Some(i) = select {
                        self.active = i;
                    }
                    if let Some(i) = close {
                        self.close_tab(i);
                    }
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
                        if ui.button("◀").on_hover_text("Предыдущее").clicked() {
                            prev = true;
                        }
                        if ui.button("▶").on_hover_text("Следующее").clicked() {
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
                if ui.button("📂 Открыть файл…").clicked() {
                    self.actions.push(Action::OpenDialog);
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
            egui::Window::new("Язык Кумир — краткий справочник")
                .open(&mut self.show_help)
                .default_width(720.0)
                .default_height(560.0)
                .show(ctx, |ui| {
                    ScrollArea::vertical()
                        .id_salt("help_scroll")
                        .show(ui, |ui| {
                            ui.label(RichText::new(QUICK_REFERENCE).monospace().size(13.0));
                        });
                });
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
                         Ядро языка, стандартная библиотека и формат .kum перенесены\n\
                         с оригинального проекта (GPLv3). Интерфейс написан заново\n\
                         на egui/eframe.",
                    );
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("Горячие клавиши: F5, Shift+F5, F6, F10, F9, Ctrl+F, Ctrl+S")
                            .small()
                            .color(palette.muted),
                    );
                });
        }
        if self.show_settings {
            let mut open = self.show_settings;
            let mut changed = false;
            egui::Window::new("Настройки")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .default_width(360.0)
                .show(ctx, |ui| {
                    ui.label("Размер шрифта");
                    changed |= ui
                        .add(egui::Slider::new(&mut self.font_size, 9.0..=40.0).integer())
                        .changed();
                    changed |= ui
                        .checkbox(&mut self.show_line_numbers, "Номера строк")
                        .changed();
                    let mut dark = self.dark;
                    if ui.checkbox(&mut dark, "Тёмная тема").changed() {
                        self.dark = dark;
                        changed = true;
                    }
                    ui.separator();
                    ui.label("Масштаб интерфейса (или щипок двумя пальцами)");
                    let mut scale = ctx.pixels_per_point();
                    if ui
                        .add(
                            egui::Slider::new(&mut scale, 0.55..=3.0)
                                .text("")
                                .suffix("×"),
                        )
                        .changed()
                    {
                        ctx.set_pixels_per_point(scale);
                    }
                    ui.separator();
                    ui.label(
                        RichText::new("Изменения применяются сразу.")
                            .small()
                            .color(palette.muted),
                    );
                });
            self.show_settings = open;
            if changed {
                self.palette = if self.dark {
                    Palette::dark()
                } else {
                    Palette::light()
                };
                theme::apply_style(ctx, &self.palette, self.font_size);
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
                self.allow_close = true;
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

        if self.run.is_some() {
            let actor_active = self
                .actor
                .lock()
                .map(|a| a.kind != ActorKind::None)
                .unwrap_or(false);
            ctx.request_repaint_after(Duration::from_millis(if actor_active { 16 } else { 40 }));
        }
    }
}

// ----------------------------------------------------------------------
// Editor widget and syntax layout
// ----------------------------------------------------------------------

fn draw_editor(
    ui: &mut Ui,
    tab: &mut EditorTab,
    palette: &Palette,
    font_size: f32,
    show_line_numbers: bool,
    run_line: u32,
    breakpoints: &mut BTreeSet<u32>,
) -> Option<u32> {
    let ctx = ui.ctx().clone();
    let id = Id::unique(("kumir_editor", tab.id));
    let font_id = FontId::monospace(font_size);
    let focused = ctx.memory(|m| m.has_focus(id));

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
    let text_for_layout = tab.text.clone();
    let _ = &text_for_layout;
    let palette_copy = *palette;
    let mut toggled_bp = None;

    let mut layouter = |ui: &Ui, buf: &dyn egui::TextBuffer, _wrap: f32| {
        let job = build_layout_job(
            buf.as_str(),
            &highlights,
            &find_ranges,
            font_id.clone(),
            &palette_copy,
            run_line,
            &protected,
        );
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
                    .desired_width(f32::INFINITY)
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
                    } else if *line as u32 == cursor_line {
                        painter.rect_filled(
                            Rect::from_min_max(
                                egui::pos2(galley_pos.x, *y),
                                egui::pos2(text_right, y + h),
                            ),
                            CornerRadius::ZERO,
                            Color32::from_rgba_unmultiplied(255, 255, 255, 9),
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
                        && (tab.completion.manual || prefix.chars().count() >= 2)
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
    toggled_bp
}

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
            italics: color == palette.comment,
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
    painter.rect_filled(rect, CornerRadius::same(6), palette.bg);

    let w = field.width.max(1) as f32;
    let h = field.height.max(1) as f32;
    let (gx, gy, cell) = robot_layout(rect, field, zoom, pan);
    let grid_w = cell * w;
    let grid_h = cell * h;
    let field_rect = Rect::from_min_size(egui::pos2(gx, gy), Vec2::new(grid_w, grid_h));
    painter.rect_filled(field_rect, CornerRadius::ZERO, palette.panel_alt);

    let wall_stroke = Stroke::new((cell * 0.09).max(1.5), palette.text);
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
                    painter.rect_filled(
                        r.shrink(1.0),
                        CornerRadius::ZERO,
                        Color32::from_rgb(72, 130, 88),
                    );
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
                        palette.warning,
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
        Stroke::new(2.0, palette.text),
        egui::StrokeKind::Inside,
    );

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
            "Поле {}×{}   Робот: ({}, {})   Масштаб: {:.0}%",
            field.width,
            field.height,
            field.robot_x,
            field.robot_y,
            zoom * 100.0
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
        let color = actor_color(&s.color, palette.call);
        painter.line_segment([map(s.x1, s.y1), map(s.x2, s.y2)], Stroke::new(2.0, color));
    }
    for c in captions {
        painter.text(
            map(c.x, c.y) + Vec2::new(4.0, -4.0),
            Align2::LEFT_BOTTOM,
            &c.text,
            FontId::proportional(12.0),
            palette.text,
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

fn actor_color(s: &str, fallback: Color32) -> Color32 {
    let t = s.trim().to_lowercase();
    if let Some(hex) = t.strip_prefix('#') {
        if hex.len() >= 6 {
            if let Ok(v) = u32::from_str_radix(&hex[..6], 16) {
                return Color32::from_rgb(
                    ((v >> 16) & 0xFF) as u8,
                    ((v >> 8) & 0xFF) as u8,
                    (v & 0xFF) as u8,
                );
            }
        }
    }
    match t.as_str() {
        "red" | "красный" => Color32::from_rgb(220, 80, 80),
        "green" | "зеленый" | "зелёный" => Color32::from_rgb(70, 160, 90),
        "blue" | "синий" => Color32::from_rgb(80, 120, 220),
        "black" | "черный" | "чёрный" => Color32::from_rgb(20, 20, 20),
        "white" | "белый" => Color32::from_rgb(240, 240, 240),
        "yellow" | "желтый" | "жёлтый" => Color32::from_rgb(220, 200, 60),
        _ => fallback,
    }
}

#[cfg(target_os = "android")]
fn read_clipboard() -> Option<String> {
    None
}

#[cfg(not(target_os = "android"))]
fn read_clipboard() -> Option<String> {
    arboard::Clipboard::new()
        .and_then(|mut c| c.get_text())
        .ok()
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
