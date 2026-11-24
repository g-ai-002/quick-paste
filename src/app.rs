use std::{
    fs::{self, OpenOptions},
    io::Write as _,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use async_channel::Receiver;
use gpui_kit::{
    App, AppContext as _, Entity, Global, WindowBounds, WindowOptions, px, size,
};

use crate::{
    autostart, brand, elevation, hotkey, logging, memo, memory_trim, paste, preview, settings,
    settings_window,
    single_instance::{self, SingleInstance},
    source, tray, themes,
    ui::MemoApp,
};

#[derive(Clone)]
struct AppPaths {
    data_dir: PathBuf,
    memos: PathBuf,
    settings: PathBuf,
    app_log: PathBuf,
    crash_log: PathBuf,
    runtime_dir: PathBuf,
}

impl AppPaths {
    fn discover() -> Self {
        let data_dir = memo::data_directory();
        let memos = data_dir.join("memos.yaml");
        Self {
            memos,
            settings: data_dir.join("settings.yaml"),
            app_log: data_dir.join("app.log"),
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
    logging::init(paths.app_log.clone());
    logging::write(format!(
        "启动 {} v{}（日志时间为 UTC）",
        brand::NAME,
        env!("CARGO_PKG_VERSION")
    ));
    logging::write(format!(
        "工作目录：{}",
        std::env::current_dir()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|error| format!("<无法获取：{error}>"))
    ));
    logging::write(format!(
        "可执行文件：{}",
        std::env::current_exe()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|error| format!("<无法获取：{error}>"))
    ));
    logging::write(format!("数据目录：{}", paths.data_dir.display()));
    logging::write(format!("本地状态：{}", paths.memos.display()));
    logging::write(format!("设置文件：{}", paths.settings.display()));
    install_panic_hook(paths.crash_log.clone());

    let instance = match SingleInstance::acquire(
        &paths.runtime_dir,
        std::env::args()
            .any(|arg| arg == single_instance::WAIT_FOR_INSTANCE_ARGUMENT),
    ) {
        Ok(Some(instance)) => instance,
        Ok(None) => {
            logging::write("已有实例在运行，本次启动退出并唤回已有窗口。");
            return;
        }
        Err(error) => {
            let message = format!("单实例检查失败：{error}");
            append_log(&paths.crash_log, &message);
            logging::write(&message);
            return;
        }
    };
    let activation_receiver = instance.activation_receiver();

    let initial_settings = match settings::load(&paths.settings) {
        Ok(settings) => {
            logging::write("设置读取成功。");
            settings
        }
        Err(error) => {
            let message = format!("读取设置失败，使用默认设置：{error}");
            append_log(&paths.crash_log, &message);
            logging::write(&message);
            settings::Settings::default()
        }
    };

    // 要求管理员权限时先请求提权重启；新实例带 --wait-instance 等本实例退出锁文件。
    if initial_settings.run_as_admin
        && let Ok(false) = elevation::is_elevated()
    {
        logging::write("设置要求管理员权限，正在请求提权重启…");
        match elevation::relaunch_elevated() {
            Ok(()) => return,
            Err(error) => {
                let message = format!("请求管理员权限失败，继续以当前权限运行：{error}");
                append_log(&paths.crash_log, &message);
                logging::write(&message);
            }
        }
    }
    // 让注册表自启项与设置保持一致（exe 路径变化或注册表被清理后自动修复）。
    if let Err(error) = autostart::set_enabled(initial_settings.auto_start) {
        logging::write(format!("同步开机自启设置失败：{error}"));
    }

    let data_source = initial_settings.data_source.clone();
    let content_source = source::ContentSource::resolve(&data_source, &paths.memos);
    logging::write(format!("数据源：{}", content_source.location()));
    if let Some(error) = data_source.validation_error() {
        logging::write(format!("数据源配置无效：{error}"));
    }

    paste::start_foreground_tracker();

    let memos_path = paths.memos.clone();
    let settings_path = paths.settings.clone();
    let crash_log_path = paths.crash_log.clone();

    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            themes::install(cx);
            settings::install(cx, settings_path, initial_settings.clone());
            settings::apply_theme(&initial_settings.theme, None, cx);
            settings_window::install(cx);
            cx.set_global(InstanceGuard(instance));

            let hotkey_receiver = hotkey::install(cx, initial_settings.hotkey.clone());
            let (tray_sender, tray_receiver) = async_channel::unbounded();
            tray::install(cx, tray_sender, initial_settings.tray_icon);
            let options = window_options(cx, &initial_settings);
            let (_, view) = gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| {
                    MemoApp::new(
                        window,
                        cx,
                        data_source.clone(),
                        memos_path.clone(),
                    )
                })
            })
            .unwrap_or_else(|error| panic!("无法创建主窗口：{error}"));
            logging::write("主窗口创建成功。");
            // 窗口以 show: false 创建：先去掉任务栏样式再显示，避免任务栏出现按钮。
            paste::apply_main_window_taskbar_style();
            // 隐藏创建的窗口原生尺寸不可靠，显示前按设置的逻辑尺寸显式定位。
            paste::position_main_window(
                initial_settings.window_width,
                initial_settings.window_height,
            );
            if !initial_settings.start_hidden {
                paste::show_main_window();
            }

            if let Err(error) = preview::install(cx) {
                let message = format!("创建预览窗口失败：{error}");
                append_log(&crash_log_path, &message);
                logging::write(&message);
            }
            memory_trim::schedule_startup_cleanup();
            spawn_hotkey_task(cx, view.clone(), hotkey_receiver);
            spawn_tray_task(
                cx,
                view.clone(),
                tray_receiver,
                memos_path.clone(),
            );
            spawn_activation_task(cx, view, activation_receiver);
            logging::write("启动完成。");
        });
}

fn window_options(cx: &mut App, settings: &settings::Settings) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::centered(
            size(px(settings.window_width), px(settings.window_height)),
            cx,
        )),
        titlebar: None,
        window_min_size: Some(size(
            px(settings::WINDOW_WIDTH_MIN),
            px(settings::WINDOW_HEIGHT_MIN),
        )),
        // 必须等 apply_main_window_taskbar_style 调整完扩展样式再显示，
        // 避免 Shell 提前登记任务栏按钮。
        show: false,
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

/// 分发托盘命令；数据源按最新设置解析，保证设置窗口拿到当前值。
fn spawn_tray_task(
    cx: &mut App,
    view: Entity<MemoApp>,
    receiver: Receiver<tray::TrayCommand>,
    memos_path: PathBuf,
) {
    cx.spawn(async move |cx| {
        while let Ok(command) = receiver.recv().await {
            match command {
                tray::TrayCommand::ShowMainWindow => {
                    let _ = view.update(cx, |view, cx| view.on_show_request(cx));
                }
                tray::TrayCommand::OpenSettings => {
                    let _ = cx.update(|cx| {
                        let data_source = settings::current(cx).data_source;
                        let content = source::ContentSource::resolve(&data_source, &memos_path);
                        if let Err(error) =
                            settings_window::open(cx, content, memos_path.clone())
                        {
                            logging::write(format!("打开设置窗口失败：{error}"));
                        }
                    });
                }
                tray::TrayCommand::Quit => {
                    let _ = cx.update(|cx| cx.quit());
                    break;
                }
            }
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
        let message = format!("panic: {info}");
        append_log(&path, &message);
        logging::write(&message);
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
