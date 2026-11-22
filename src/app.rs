use std::{
    fs::{self, OpenOptions},
    io::Write as _,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use async_channel::Receiver;
use gpui_kit::{
    App, AppContext as _, Entity, Global, TitlebarOptions, WindowBounds, WindowOptions, px, size,
};

use crate::{
    hotkey, memo, memory_trim, paste, settings, settings_window,
    single_instance::SingleInstance,
    ui::MemoApp,
};

#[derive(Clone)]
struct AppPaths {
    data_dir: PathBuf,
    source: PathBuf,
    memos: PathBuf,
    settings: PathBuf,
    crash_log: PathBuf,
    runtime_dir: PathBuf,
}

impl AppPaths {
    fn discover() -> Self {
        let data_dir = memo::data_directory();
        let memos = data_dir.join("memos.yaml");
        let source = memo::external_source_path()
            .filter(|path| path.as_path() != memos.as_path())
            .unwrap_or_else(|| memos.clone());
        Self {
            source,
            memos,
            settings: data_dir.join("settings.yaml"),
            crash_log: data_dir.join("crash.log"),
            runtime_dir: data_dir.join("runtime"),
            data_dir,
        }
    }
}

struct InstanceGuard(#[allow(dead_code)] SingleInstance);

impl Global for InstanceGuard {}

pub fn run() {
    let paths = AppPaths::discover();

    if let Err(error) = fs::create_dir_all(&paths.data_dir) {
        append_log(
            &std::env::temp_dir().join("quick-paste-startup.log"),
            &format!("无法创建数据目录 {:?}：{error}", paths.data_dir),
        );
        return;
    }
    install_panic_hook(paths.crash_log.clone());

    let instance = match SingleInstance::acquire(&paths.runtime_dir) {
        Ok(Some(instance)) => instance,
        Ok(None) => return,
        Err(error) => {
            append_log(&paths.crash_log, &format!("单实例检查失败：{error}"));
            return;
        }
    };
    let activation_receiver = instance.activation_receiver();

    let initial_settings = match settings::load(&paths.settings) {
        Ok(settings) => settings,
        Err(error) => {
            append_log(&paths.crash_log, &format!("读取设置失败：{error}"));
            settings::Settings::default()
        }
    };

    paste::start_foreground_tracker();

    let source_path = paths.source.clone();
    let memos_path = paths.memos.clone();
    let settings_path = paths.settings.clone();

    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            settings::install(cx, settings_path, initial_settings.clone());
            settings::apply_theme(initial_settings.theme, None, cx);
            settings_window::install(cx);
            cx.set_global(InstanceGuard(instance));

            let hotkey_receiver = hotkey::install(cx, initial_settings.hotkey.clone());
            let options = window_options(cx);
            let (_, view) = gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| {
                    MemoApp::new(
                        window,
                        cx,
                        source_path.clone(),
                        memos_path.clone(),
                    )
                })
            })
            .unwrap_or_else(|error| panic!("无法创建主窗口：{error}"));

            memory_trim::schedule_startup_cleanup();
            if initial_settings.start_hidden {
                paste::hide_main_window();
            }
            spawn_hotkey_task(cx, view.clone(), hotkey_receiver);
            spawn_activation_task(cx, view, activation_receiver);
        });
}

fn window_options(cx: &mut App) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::centered(size(px(460.0), px(640.0)), cx)),
        titlebar: Some(TitlebarOptions {
            title: Some("快速粘贴".into()),
            ..Default::default()
        }),
        window_min_size: Some(size(px(420.0), px(520.0))),
        app_id: Some("com.g-ai-002.quick-paste".to_owned()),
        ..Default::default()
    }
}

fn spawn_hotkey_task(cx: &mut App, view: Entity<MemoApp>, receiver: Receiver<()>) {
    cx.spawn(async move |cx| {
        let mut last = Instant::now() - Duration::from_secs(5);
        while receiver.recv().await.is_ok() {
            if last.elapsed() < Duration::from_millis(250) {
                continue;
            }
            last = Instant::now();
            let _ = view.update(cx, |view, cx| view.on_hotkey(cx));
        }
    })
    .detach();
}

fn spawn_activation_task(cx: &mut App, view: Entity<MemoApp>, receiver: Receiver<()>) {
    cx.spawn(async move |cx| {
        while receiver.recv().await.is_ok() {
            let _ = view.update(cx, |view, cx| view.on_show_request(cx));
            while receiver.try_recv().is_ok() {}
        }
    })
    .detach();
}

fn install_panic_hook(path: PathBuf) {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        append_log(&path, &format!("panic: {info}"));
        default_hook(info);
    }));
}

fn append_log(path: &Path, message: &str) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "[{}] {message}", unix_seconds());
    }
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
