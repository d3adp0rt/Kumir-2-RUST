mod app;
mod completion;
mod file_dialog;
mod runner;
mod theme;

const APP_TITLE: &str = "КуМир";

#[cfg(not(target_os = "android"))]
pub fn run_native() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 840.0])
            .with_min_inner_size([860.0, 560.0])
            .with_title("КуМир — среда программирования"),
        ..Default::default()
    };
    eframe::run_native(
        APP_TITLE,
        options,
        Box::new(|cc| Ok(Box::new(app::KumirApp::new(cc)))),
    )
}

#[cfg(target_os = "android")]
pub fn run_native(
    android_app: Option<winit::platform::android::activity::AndroidApp>,
) -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default(),
        android_app,
        ..Default::default()
    };
    eframe::run_native(
        APP_TITLE,
        options,
        Box::new(|cc| Ok(Box::new(app::KumirApp::new(cc)))),
    )
}

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(android_app: winit::platform::android::activity::AndroidApp) {
    use winit::platform::android::activity::WindowManagerFlags;

    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info),
    );

    // Не рисуем под статус-баром: без этого верхний ряд UI перекрывается
    // системной панелью и не кликается.
    android_app.set_window_flags(
        WindowManagerFlags::FORCE_NOT_FULLSCREEN | WindowManagerFlags::LAYOUT_IN_SCREEN,
        WindowManagerFlags::FULLSCREEN,
    );

    crate::android_platform::register(android_app.clone());
    run_native(Some(android_app)).expect("eframe error");
}

#[cfg(target_os = "android")]
pub mod android_platform {
    use std::sync::{Mutex, OnceLock};
    use winit::platform::android::activity::AndroidApp;

    static APP: OnceLock<AndroidApp> = OnceLock::new();
    static INSETS_PX: Mutex<Option<[i32; 4]>> = Mutex::new(None);
    static LAST_SIZE: Mutex<Option<[f32; 2]>> = Mutex::new(None);
    static LAST_QUERY: Mutex<Option<std::time::Instant>> = Mutex::new(None);
    static DIAG: Mutex<String> = Mutex::new(String::new());

    pub fn register(app: AndroidApp) {
        let _ = APP.set(app);
    }

    fn activity_object<'local>(
        app: &AndroidApp,
        env: &mut jni::Env<'local>,
    ) -> jni::objects::JObject<'local> {
        unsafe { jni::objects::JObject::from_raw(env, app.activity_as_ptr() as jni::sys::jobject) }
    }

    fn query_insets() -> Option<[i32; 4]> {
        let app = APP.get()?;
        let vm = jni::JavaVM::singleton().ok()?;
        let result: jni::errors::Result<[i32; 4]> = vm.attach_current_thread(|env| {
            let activity = activity_object(app, env);
            let window = env
                .call_method(
                    &activity,
                    jni::jni_str!("getWindow"),
                    jni::jni_sig!("()Landroid/view/Window;"),
                    &[],
                )?
                .l()?;
            let insets = env
                .call_method(
                    &window,
                    jni::jni_str!("getDecorView"),
                    jni::jni_sig!("()Landroid/view/View;"),
                    &[],
                )?
                .l()
                .and_then(|decor| {
                    env.call_method(
                        &decor,
                        jni::jni_str!("getRootWindowInsets"),
                        jni::jni_sig!("()Landroid/view/WindowInsets;"),
                        &[],
                    )
                    .and_then(|v| v.l())
                })?;
            if AndroidApp::sdk_version() >= 30 {
                // Типовой системные панели — БЕЗ встава клавиатуры (IME),
                // иначе при открытой клавиатуре панель растёт на её высоту.
                let mask = env
                    .call_static_method(
                        jni::jni_str!("android/view/WindowInsets$Type"),
                        jni::jni_str!("systemBars"),
                        jni::jni_sig!("()I"),
                        &[],
                    )?
                    .i()?;
                let sys = env
                    .call_method(
                        &insets,
                        jni::jni_str!("getInsets"),
                        jni::jni_sig!("(I)Landroid/graphics/Insets;"),
                        &[(mask).into()],
                    )?
                    .l()?;
                let left = env
                    .get_field(&sys, jni::jni_str!("left"), jni::jni_sig!("I"))?
                    .i()?;
                let top = env
                    .get_field(&sys, jni::jni_str!("top"), jni::jni_sig!("I"))?
                    .i()?;
                let right = env
                    .get_field(&sys, jni::jni_str!("right"), jni::jni_sig!("I"))?
                    .i()?;
                let bottom = env
                    .get_field(&sys, jni::jni_str!("bottom"), jni::jni_sig!("I"))?
                    .i()?;
                return Ok([left, top, right, bottom]);
            }
            let top = env
                .call_method(
                    &insets,
                    jni::jni_str!("getSystemWindowInsetTop"),
                    jni::jni_sig!("()I"),
                    &[],
                )?
                .i()?;
            let left = env
                .call_method(
                    &insets,
                    jni::jni_str!("getSystemWindowInsetLeft"),
                    jni::jni_sig!("()I"),
                    &[],
                )?
                .i()?;
            let right = env
                .call_method(
                    &insets,
                    jni::jni_str!("getSystemWindowInsetRight"),
                    jni::jni_sig!("()I"),
                    &[],
                )?
                .i()?;
            let bottom = env
                .call_method(
                    &insets,
                    jni::jni_str!("getSystemWindowInsetBottom"),
                    jni::jni_sig!("()I"),
                    &[],
                )?
                .i()?;
            Ok([left, top, right, bottom])
        });
        let mut diag = DIAG.lock().unwrap();
        match result {
            Ok(v) => {
                *diag = format!("insets L={} T={} R={} B={}", v[0], v[1], v[2], v[3]);
                Some(v)
            }
            Err(err) => {
                *diag = format!("insets-err: {err}");
                None
            }
        }
    }

    /// Скрывает статус-бар и навигационную панель (immersive).
    /// Публикуется на главный Java-поток — вызовы View должны идти оттуда.
    pub fn hide_system_bars() {
        let Some(app) = APP.get().cloned() else {
            return;
        };
        app_for_thread_helper(app);
    }

    fn app_for_thread_helper(app: AndroidApp) {
        app.clone().run_on_java_main_thread(Box::new(move || {
            let result: jni::errors::Result<()> = (|| {
                let vm = jni::JavaVM::singleton()?;
                vm.attach_current_thread(|env| {
                    let activity = activity_object(&app, env);
                    let window = env
                        .call_method(
                            &activity,
                            jni::jni_str!("getWindow"),
                            jni::jni_sig!("()Landroid/view/Window;"),
                            &[],
                        )?
                        .l()?;
                    let decor = env
                        .call_method(
                            &window,
                            jni::jni_str!("getDecorView"),
                            jni::jni_sig!("()Landroid/view/View;"),
                            &[],
                        )?
                        .l()?;
                    if AndroidApp::sdk_version() >= 30 {
                        if let Ok(controller) = env
                            .call_method(
                                &window,
                                jni::jni_str!("getInsetsController"),
                                jni::jni_sig!("()Landroid/view/WindowInsetsController;"),
                                &[],
                            )
                            .and_then(|v| v.l())
                        {
                            let status = env
                                .call_static_method(
                                    jni::jni_str!("android/view/WindowInsets$Type"),
                                    jni::jni_str!("statusBars"),
                                    jni::jni_sig!("()I"),
                                    &[],
                                )?
                                .i()?;
                            let nav = env
                                .call_static_method(
                                    jni::jni_str!("android/view/WindowInsets$Type"),
                                    jni::jni_str!("navigationBars"),
                                    jni::jni_sig!("()I"),
                                    &[],
                                )?
                                .i()?;
                            let _ = env.call_method(
                                &controller,
                                jni::jni_str!("hide"),
                                jni::jni_sig!("(I)V"),
                                &[(status | nav).into()],
                            );
                        }
                    }
                    // Legacy immersive sticky
                    let legacy: i32 = 0x100 | 0x200 | 0x400 | 0x2 | 0x4 | 0x800 | 0x1000;
                    let _ = env.call_method(
                        &decor,
                        jni::jni_str!("setSystemUiVisibility"),
                        jni::jni_sig!("(I)V"),
                        &[legacy.into()],
                    );
                    Ok(())
                })
            })();
            if let Err(err) = result {
                log::warn!("hide_system_bars failed: {err}");
                *DIAG.lock().unwrap() = format!("hide-err: {err}");
            }
        }));
    }

    /// Отступы системных панелей (обводки/статус-бара) в лог. точках egui.
    /// Запрашивает JNI только когда изменился размер окна (поворот и т.п.).
    pub fn safe_area(ctx: &egui::Context) -> Option<[f32; 4]> {
        let size = ctx.input(|i| {
            let r = i.viewport_rect();
            [r.width(), r.height()]
        });
        let mut insets_lock = INSETS_PX.lock().unwrap();
        let mut size_changed = false;
        {
            let mut last_lock = LAST_SIZE.lock().unwrap();
            let same_size = last_lock.map(|s| s == size).unwrap_or(false);
            if !same_size {
                size_changed = true;
                *last_lock = Some(size);
                // hide() дёргает IME на некоторых прошивках: вызываем только
                // при смене ориентации/первый раз.
                let orient: u8 = if size[0] >= size[1] { 1 } else { 0 };
                let need_hide = {
                    static LAST_ORIENT: Mutex<Option<u8>> = Mutex::new(None);
                    let mut o = LAST_ORIENT.lock().unwrap();
                    let prev = *o;
                    *o = Some(orient);
                    prev != Some(orient)
                };
                if need_hide {
                    *LAST_QUERY.lock().unwrap() = Some(std::time::Instant::now());
                    hide_system_bars();
                }
            }
        }
        // Периодический перезапрос insets БЕЗ hide(): частые вызовы
        // InsetsController.hide() на некоторых прошивках гасят IME.
        let due = {
            let mut q = LAST_QUERY.lock().unwrap();
            let need = q
                .as_ref()
                .map(|t| t.elapsed() > std::time::Duration::from_secs(2))
                .unwrap_or(true);
            if need {
                *q = Some(std::time::Instant::now());
            }
            need
        };
        if due || size_changed {
            if let Some(v) = query_insets() {
                *insets_lock = Some(v);
            }
        }
        let px = (*insets_lock)?;
        if px.iter().sum::<i32>() == 0 {
            return None;
        }
        let ppt = ctx.pixels_per_point();
        Some([
            px[0] as f32 / ppt,
            px[1] as f32 / ppt,
            px[2] as f32 / ppt,
            px[3] as f32 / ppt,
        ])
    }

    /// Есть ли у приложения доступ к общей памяти (Android 11+: «Все файлы»).
    pub fn has_file_access() -> bool {
        let Some(app) = APP.get().cloned() else {
            return false;
        };
        let vm = match jni::JavaVM::singleton() {
            Ok(vm) => vm,
            Err(_) => return false,
        };
        let result: jni::errors::Result<bool> = vm.attach_current_thread(|env| {
            let activity = activity_object(&app, env);
            env.call_method(
                &activity,
                jni::jni_str!("hasFileAccess"),
                jni::jni_sig!("()Z"),
                &[],
            )?
            .z()
        });
        result.unwrap_or(false)
    }

    /// Открывает системный запрос доступа к файлам (настройки приложения).
    pub fn request_file_access() {
        let Some(app) = APP.get().cloned() else {
            return;
        };
        app.clone().run_on_java_main_thread(Box::new(move || {
            let result: jni::errors::Result<()> = (|| {
                let vm = jni::JavaVM::singleton()?;
                vm.attach_current_thread(|env| {
                    let activity = activity_object(&app, env);
                    env.call_method(
                        &activity,
                        jni::jni_str!("requestFileAccess"),
                        jni::jni_sig!("()V"),
                        &[],
                    )?;
                    Ok(())
                })
            })();
            if let Err(err) = result {
                log::warn!("request_file_access failed: {err}");
            }
        }));
    }

    /// Каталог по умолчанию для «Сохранить/Открыть» на Android.
    pub fn storage_dir() -> Option<std::path::PathBuf> {
        let app = APP.get()?.clone();
        let vm = jni::JavaVM::singleton().ok()?;
        let result: jni::errors::Result<String> = vm.attach_current_thread(|env| {
            let activity = activity_object(&app, env);
            let js = env
                .call_method(
                    &activity,
                    jni::jni_str!("getStorageDir"),
                    jni::jni_sig!("()Ljava/lang/String;"),
                    &[],
                )?
                .l()?;
            let js = unsafe { jni::objects::JString::from_raw(env, js.into_raw()) };
            let s = js.try_to_string(env)?;
            Ok(s)
        });
        result.ok().map(std::path::PathBuf::from)
    }

    /// Диагностика: возвращается, только если отступы не получены/нулевые —
    /// тогда приложение рисует мелкую строку внизу слева.
    pub fn last_diag() -> Option<String> {
        let diag = DIAG.lock().unwrap().clone();
        if diag.is_empty() {
            return None;
        }
        let insets = *INSETS_PX.lock().unwrap();
        match insets {
            Some(v) if v[1] > 0 => None,
            _ => Some(diag),
        }
    }

    static LAST_TEXT: Mutex<Option<String>> = Mutex::new(None);

    /// Мелкая IME-диагностика: хвост trace.log (пишется Java-классом в
    /// internal-хранилище; app.internal_data_path даёт тот же каталог).
    pub fn ime_diag() -> Option<String> {
        static LAST_READ: Mutex<Option<std::time::Instant>> = Mutex::new(None);
        {
            let mut q = LAST_READ.lock().unwrap();
            let due = q
                .as_ref()
                .map(|t| t.elapsed() > std::time::Duration::from_secs(1))
                .unwrap_or(true);
            if !due {
                return None;
            }
            *q = Some(std::time::Instant::now());
        }
        let app = APP.get()?.clone();
        let dir = app.internal_data_path()?;
        let lines = std::fs::read_to_string(dir.join("trace.log")).ok()?;
        let mut picks: Vec<String> = Vec::new();
        for l in lines.split('\n').rev() {
            if picks.len() >= 3 {
                break;
            }
            let t = l.trim();
            if t.starts_with("ime_") || t.starts_with("editor_action") || t.starts_with("enter_") {
                picks.push(t.to_string());
            }
        }
        picks.reverse();
        if picks.is_empty() {
            None
        } else {
            Some(picks.join(" | "))
        }
    }

    /// Дифф IME-буфера GameTextInput: winit на Android не транслирует
    /// TextInputState, поэтому конвертируем сами в события egui.
    pub fn inject_text_input(ctx: &egui::Context) {
        let Some(app) = APP.get().cloned() else {
            return;
        };

        // EditorInfo: многострочный ввод; Enter = перенос строки, а не «Готово».
        // Задаем сразу после старта рендера (первые кадры) — IME учитывает это
        // при СОЗДАНИИ input connection. Повторно НЕ вызываем: на некоторых
        // прошивках это перезапускает ввод и прячет клавиатуру.
        {
            use winit::platform::android::activity::input::{
                ImeOptions, InputType, TextInputAction,
            };
            static ATTEMPTS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let n = ATTEMPTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if n < 5 {
                let mut opts = ImeOptions::empty();
                opts.set_action(TextInputAction::None);
                opts |= ImeOptions::IME_FLAG_NO_ENTER_ACTION;
                // Не даём IME уходить в полноэкранный режим (в landscape
                // GBoard иначе рисует свою панель поверх всего приложения).
                opts |= ImeOptions::IME_FLAG_NO_FULLSCREEN;
                opts |= ImeOptions::IMG_FLAG_NO_EXTRACT_UI;
                let input_type = InputType::TYPE_CLASS_TEXT
                    | InputType::TYPE_TEXT_VARIATION_LONG_MESSAGE
                    | InputType::TYPE_TEXT_FLAG_MULTI_LINE
                    | InputType::TYPE_TEXT_FLAG_NO_SUGGESTIONS;
                app.set_ime_editor_info(input_type, TextInputAction::None, opts);
            }
        }

        let wants = ctx.egui_wants_keyboard_input();

        // Пока egui ждёт текст — держим перерисовку: egui в реактивном
        // режиме перерисовывается только по внешним событиям, а IME-ввод
        // в события winit не попадает.
        if wants {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }

        let polled = wants || {
            // Пока клавиатура не нужна — синхронизируем буфер редко,
            // чтобы не нести JNI-накладные расходы на каждом кадре.
            static LAST_SYNC: Mutex<Option<std::time::Instant>> = Mutex::new(None);
            let mut q = LAST_SYNC.lock().unwrap();
            let due = q
                .as_ref()
                .map(|t| t.elapsed() > std::time::Duration::from_millis(400))
                .unwrap_or(true);
            if due {
                *q = Some(std::time::Instant::now());
            }
            due
        };
        if !polled {
            return;
        }
        let state = app.text_input_state();
        let text = state.text;
        let delta = {
            let mut last = LAST_TEXT.lock().unwrap();
            match last.clone() {
                None => {
                    // Первое наблюдение — считаем точкой отсчёта.
                    *last = Some(text);
                    None
                }
                Some(old) if old == text => None,
                Some(old) => {
                    let oc: Vec<char> = old.chars().collect();
                    let nc: Vec<char> = text.chars().collect();
                    let mut p = 0usize;
                    while p < oc.len() && p < nc.len() && oc[p] == nc[p] {
                        p += 1;
                    }
                    let mut s = 0usize;
                    while s < oc.len() - p
                        && s < nc.len() - p
                        && oc[oc.len() - 1 - s] == nc[nc.len() - 1 - s]
                    {
                        s += 1;
                    }
                    let removed = oc.len() - p - s;
                    let inserted: String = nc[p..nc.len() - s].iter().collect();
                    // Клавиатура закрылась/сбросила буфер в пустоту — не считаем удалением.
                    let reset = inserted.is_empty() && text.is_empty() && oc.len() > 8;
                    *last = Some(text);
                    if reset {
                        None
                    } else if removed == 0 && inserted.is_empty() {
                        None
                    } else {
                        Some((removed, inserted))
                    }
                }
            }
        };
        let Some((removed, inserted)) = delta else {
            return;
        };
        if !ctx.egui_wants_keyboard_input() {
            // Буфер синхронизирован (delta-блок уже обновил LAST_TEXT), события не нужны.
            return;
        }
        ctx.input_mut(|i| {
            for _ in 0..removed {
                i.events.push(egui::Event::Key {
                    key: egui::Key::Backspace,
                    physical_key: Some(egui::Key::Backspace),
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::default(),
                });
                i.events.push(egui::Event::Key {
                    key: egui::Key::Backspace,
                    physical_key: Some(egui::Key::Backspace),
                    pressed: false,
                    repeat: false,
                    modifiers: egui::Modifiers::default(),
                });
            }
            // Enter не проходит через Text (egui отфильтрует \n) — идём посимвольно:
            // Enter — как key-событие, остальное — как Text.
            let mut chunk = String::new();
            for c in inserted.chars() {
                if c == '\n' {
                    if !chunk.is_empty() {
                        i.events.push(egui::Event::Text(std::mem::take(&mut chunk)));
                    }
                    i.events.push(egui::Event::Key {
                        key: egui::Key::Enter,
                        physical_key: Some(egui::Key::Enter),
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::default(),
                    });
                    i.events.push(egui::Event::Key {
                        key: egui::Key::Enter,
                        physical_key: Some(egui::Key::Enter),
                        pressed: false,
                        repeat: false,
                        modifiers: egui::Modifiers::default(),
                    });
                } else {
                    chunk.push(c);
                }
            }
            if !chunk.is_empty() {
                i.events.push(egui::Event::Text(chunk));
            }
        });
    }
}
#[cfg(not(target_os = "android"))]
#[allow(dead_code)]
mod android_platform {
    pub fn safe_area(_ctx: &egui::Context) -> Option<[f32; 4]> {
        None
    }
    pub fn last_diag() -> Option<String> {
        None
    }
    pub fn ime_diag() -> Option<String> {
        None
    }
    pub fn inject_text_input(_ctx: &egui::Context) {}
    pub fn has_file_access() -> bool {
        true
    }
    pub fn request_file_access() {}
    pub fn storage_dir() -> Option<std::path::PathBuf> {
        None
    }
}
