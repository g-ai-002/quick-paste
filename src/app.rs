use std::{
    fs::{self, OpenOptions},
    io::Write as _,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use async_channel::Receiver;
use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
    hotkey::{Code, HotKey, Modifiers as HotkeyModifiers},
};
use gpui_kit::{
    App, AppContext as _, Entity, Global, SharedString, TitlebarOptions, WindowBounds,
    WindowOptions, px, size,
};

use crate::{
    memo, paste,
    single_instance::SingleInstance,
    ui::MemoApp,
};

#[derive(Clone)]
pub struct HotkeyStatus {
    pub label: Option<SharedString>,
    pub error: Option<SharedString>,
}

impl HotkeyStatus {
    fn ready(label: &str) -> Self {
        Self {
            label: Some(label.into()),
            error: None,
        }
    }

    fn failed(message: impl Into<SharedString>) -> Self {
        Self {
            label: None,
            error: Some(message.into()),
        }
    }
}

#[derive(Clone)]
struct AppPaths {
    data_dir: PathBuf,
    memos: PathBuf,
    crash_log: PathBuf,
    runtime_dir: PathBuf,
}

impl AppPaths {
    fn discover() -> Self {
        let data_dir = memo::data_directory();
        Self {
            memos: data_dir.join("memos.yaml"),
            crash_log: data_dir.join("crash.log"),
            runtime_dir: data_dir.join("runtime"),
            data_dir,
        }
    }
}

struct InstanceGuard(#[allow(dead_code)] SingleInstance);

impl Global for InstanceGuard {}

struct HotkeyManagerHolder(#[allow(dead_code)] GlobalHotKeyManager);

impl Global for HotkeyManagerHolder {}

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

    paste::start_foreground_tracker();

    let memos_path = paths.memos.clone();

    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            cx.set_global(InstanceGuard(instance));

            let (hotkey_status, hotkey_receiver) = install_hotkey(cx);
            let options = window_options(cx);
            let (_, view) = gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| {
                    MemoApp::new(window, cx, memos_path.clone(), hotkey_status.clone())
                })
            })
            .unwrap_or_else(|error| panic!("无法创建主窗口：{error}"));

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

fn install_hotkey(cx: &mut App) -> (HotkeyStatus, Receiver<()>) {
    let (sender, receiver) = async_channel::unbounded();
    GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
        if event.state == HotKeyState::Pressed {
            let _ = sender.try_send(());
        }
    }));

    let manager = match GlobalHotKeyManager::new() {
        Ok(manager) => manager,
        Err(error) => {
            return (
                HotkeyStatus::failed(format!("全局快捷键初始化失败：{error}")),
                receiver,
            );
        }
    };

    let candidates = [
        (
            HotKey::new(
                Some(HotkeyModifiers::CONTROL | HotkeyModifiers::SHIFT),
                Code::KeyX,
            ),
            "Ctrl+Shift+X",
        ),
        (
            HotKey::new(
                Some(HotkeyModifiers::CONTROL | HotkeyModifiers::ALT),
                Code::KeyV,
            ),
            "Ctrl+Alt+V",
        ),
    ];

    for (hotkey, label) in candidates {
        if manager.register(hotkey).is_ok() {
            cx.set_global(HotkeyManagerHolder(manager));
            return (HotkeyStatus::ready(label), receiver);
        }
    }

    (
        HotkeyStatus::failed("全局快捷键注册失败，可再次启动程序唤回窗口"),
        receiver,
    )
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
