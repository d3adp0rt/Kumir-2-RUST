//! A small in-app file chooser. Keeps the IDE dependency-free and works on
//! systems without a native file-dialog portal.

use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Open,
    Save,
}

pub enum DialogOutcome {
    Open(PathBuf),
    Save(PathBuf),
    Cancelled,
}

struct Entry {
    path: PathBuf,
    name: String,
    is_dir: bool,
}

pub struct FileDialog {
    mode: Mode,
    pub dir: PathBuf,
    filename: String,
    entries: Vec<Entry>,
    error: Option<String>,
    need_refresh: bool,
    extension: String,
}

impl FileDialog {
    pub fn new(mode: Mode, start: PathBuf, filename: impl Into<String>) -> Self {
        let mut d = Self {
            mode,
            dir: start,
            filename: filename.into(),
            entries: Vec::new(),
            error: None,
            need_refresh: true,
            extension: "kum".to_string(),
        };
        d.refresh();
        d
    }

    pub fn with_extension(
        mode: Mode,
        start: PathBuf,
        filename: impl Into<String>,
        extension: impl Into<String>,
    ) -> Self {
        let mut d = Self::new(mode, start, filename);
        d.extension = extension.into();
        d
    }

    fn refresh(&mut self) {
        self.entries.clear();
        self.error = None;
        let rd = match std::fs::read_dir(&self.dir) {
            Ok(r) => r,
            Err(e) => {
                self.error = Some(format!("Не удалось прочитать каталог: {e}"));
                return;
            }
        };
        let mut dirs = Vec::new();
        let mut files = Vec::new();
        for entry in rd.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let is_dir = path.is_dir();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            if !is_dir
                && !matches!(
                    ext.as_str(),
                    "kum" | "kod" | "fil" | "txt" | "md" | "vod" | "kz" | "png" | "svg"
                )
            {
                continue;
            }
            let e = Entry { path, name, is_dir };
            if is_dir {
                dirs.push(e);
            } else {
                files.push(e);
            }
        }
        dirs.sort_by_key(|a| a.name.to_lowercase());
        files.sort_by_key(|a| a.name.to_lowercase());
        self.entries = dirs;
        self.entries.extend(files);
        self.need_refresh = false;
    }

    fn go_up(&mut self) {
        if let Some(parent) = self.dir.parent() {
            self.dir = parent.to_path_buf();
            self.need_refresh = true;
        }
    }

    fn selected_path(&self) -> PathBuf {
        let mut p = PathBuf::from(self.filename.trim());
        if p.as_os_str().is_empty() {
            p = self.dir.clone();
        } else if !p.is_absolute() {
            p = self.dir.join(p);
        }
        if self.mode == Mode::Save && p.extension().is_none() {
            p.set_extension(self.extension.clone());
        }
        p
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        palette: &crate::theme::Palette,
    ) -> Option<DialogOutcome> {
        if self.need_refresh {
            self.refresh();
        }
        let title = match self.mode {
            Mode::Open => "Открыть программу",
            Mode::Save => "Сохранить программу как",
        };
        let mut outcome = None;
        let mut close = false;
        egui::Window::new(title)
            .collapsible(false)
            .resizable(true)
            .default_width(680.0)
            .default_height(500.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                #[cfg(target_os = "android")]
                ui.horizontal(|ui| {
                    if crate::android_platform::has_file_access() {
                        ui.label(
                            egui::RichText::new("Доступ к файлам: есть")
                                .small()
                                .color(palette.muted),
                        );
                    } else {
                        ui.label(
                            egui::RichText::new("Нет доступа к общей памяти")
                                .small()
                                .color(palette.error),
                        );
                        if ui.button("Разрешить доступ к файлам").clicked() {
                            crate::android_platform::request_file_access();
                        }
                    }
                    if ui.button("Документы").clicked() {
                        if let Some(dir) = crate::android_platform::storage_dir() {
                            if dir != self.dir {
                                self.dir = dir;
                                self.need_refresh = true;
                            }
                        }
                    }
                });
                ui.horizontal(|ui| {
                    if crate::ui::icon_button(
                        ui,
                        palette,
                        crate::icons::Icon::FolderUp,
                        false,
                        true,
                        "Выше",
                    )
                    .clicked()
                    {
                        self.go_up();
                    }
                    if crate::ui::icon_button(
                        ui,
                        palette,
                        crate::icons::Icon::FolderOpen,
                        false,
                        true,
                        "Домашний каталог",
                    )
                    .clicked()
                    {
                        let home = std::env::var_os("HOME")
                            .map(PathBuf::from)
                            .or_else(crate::android_platform::storage_dir);
                        if let Some(home) = home {
                            self.dir = home;
                            self.need_refresh = true;
                        }
                    }
                    let mut dir_text = self.dir.to_string_lossy().to_string();
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut dir_text)
                                .desired_width(f32::INFINITY)
                                .hint_text("Каталог"),
                        )
                        .lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    {
                        self.dir = PathBuf::from(dir_text);
                        self.need_refresh = true;
                    }
                });
                if let Some(err) = &self.error {
                    ui.colored_label(palette.error, err);
                }
                let list_height = (ui.available_height() - 100.0).max(160.0);
                egui::ScrollArea::vertical()
                    .id_salt("file_dialog_list")
                    .max_height(list_height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if crate::ui::list_row(
                            ui,
                            palette,
                            crate::icons::Icon::FolderUp,
                            "..",
                            false,
                            true,
                        )
                        .on_hover_text("Родительский каталог")
                        .clicked()
                        {
                            self.go_up();
                        }
                        let entries: Vec<(PathBuf, String, bool)> = self
                            .entries
                            .iter()
                            .map(|e| (e.path.clone(), e.name.clone(), e.is_dir))
                            .collect();
                        for (path, name, is_dir) in entries {
                            let selected = !is_dir && self.dir.join(&self.filename) == path;
                            let icon = if is_dir {
                                crate::icons::Icon::Folder
                            } else {
                                crate::icons::Icon::File
                            };
                            let resp =
                                crate::ui::list_row(ui, palette, icon, &name, selected, false);
                            if resp.clicked() {
                                if is_dir {
                                    if resp.double_clicked() {
                                        self.dir = path.clone();
                                        self.need_refresh = true;
                                    }
                                } else {
                                    self.filename = name.clone();
                                }
                            }
                            if resp.double_clicked() && !is_dir {
                                outcome = Some(match self.mode {
                                    Mode::Open => DialogOutcome::Open(path.clone()),
                                    Mode::Save => DialogOutcome::Save(path.clone()),
                                });
                                close = true;
                            }
                        }
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Имя файла:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.filename)
                            .desired_width(f32::INFINITY)
                            .hint_text("программа.kum"),
                    );
                });
                ui.horizontal(|ui| {
                    let ok_label = match self.mode {
                        Mode::Open => "Открыть",
                        Mode::Save => "Сохранить",
                    };
                    if ui.button(ok_label).clicked() && !self.filename.trim().is_empty() {
                        let p = self.selected_path();
                        outcome = Some(match self.mode {
                            Mode::Open => DialogOutcome::Open(p),
                            Mode::Save => DialogOutcome::Save(p),
                        });
                        close = true;
                    }
                    if ui.button("Отмена").clicked() {
                        outcome = Some(DialogOutcome::Cancelled);
                        close = true;
                    }
                });
            });
        if close || outcome.is_some() {
            if matches!(outcome, Some(DialogOutcome::Cancelled)) {
                return Some(DialogOutcome::Cancelled);
            }
            return outcome;
        }
        None
    }
}
