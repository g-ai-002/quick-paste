//! 系统托盘：通知区图标、左键切换主窗口、右键原生菜单。
//!
//! 图标常驻但可按设置隐藏；托盘与菜单事件都经由通道转回 GPUI 更新上下文，
//! 避免在 Win32 回调里直接操作窗口。

#[cfg(target_os = "windows")]
mod windows_impl {
    use std::thread;

    use async_channel::Sender;
    use gpui_kit::{App, Global};
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, MenuId};
    use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

    use crate::logging;

    pub const OPEN_MAIN_WINDOW_ID: &str = "quick-paste.tray.open-main-window";
    pub const OPEN_SETTINGS_ID: &str = "quick-paste.tray.open-settings";
    pub const QUIT_ID: &str = "quick-paste.tray.quit";

    const TOOLTIP: &str = "备忘快贴";

    /// 托盘事件翻译成应用命令后交给 app.rs 分发。
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum TrayCommand {
        /// 左键点击：显示或隐藏主窗口。
        ToggleMainWindow,
        /// 菜单「打开主窗口」：只显示主窗口。
        ShowMainWindow,
        OpenSettings,
        Quit,
    }

    pub struct TrayService {
        icon: TrayIcon,
        // 菜单条目由 Menu 持有，与图标同生命周期，保证右键菜单始终可用。
        _menu: Menu,
    }

    impl Global for TrayService {}

    pub fn install(cx: &mut App, sender: Sender<TrayCommand>, visible: bool) {
        let (icon, menu) = match build() {
            Ok(built) => built,
            Err(error) => {
                logging::write(format!("创建托盘图标失败：{error}"));
                return;
            }
        };
        if let Err(error) = icon.set_visible(visible) {
            logging::write(format!("同步托盘图标显示状态失败：{error}"));
        }
        cx.set_global(TrayService { icon, _menu: menu });

        let click_sender = sender.clone();
        TrayIconEvent::set_event_handler(Some(move |event| {
            if let Some(command) = click_command(event) {
                let _ = click_sender.try_send(command);
            }
        }));
        spawn_menu_event_loop(sender);
    }

    /// 按设置显示或隐藏托盘图标。
    pub fn sync_visible(cx: &App, visible: bool) {
        let Some(service) = cx.try_global::<TrayService>() else {
            return;
        };
        if let Err(error) = service.icon.set_visible(visible) {
            logging::write(format!("切换托盘图标显示状态失败：{error}"));
        }
    }

    fn build() -> Result<(TrayIcon, Menu), String> {
        let menu = build_menu()?;
        let icon = TrayIconBuilder::new()
            .with_icon(load_icon()?)
            .with_tooltip(TOOLTIP)
            .with_menu(Box::new(menu.clone()))
            .with_menu_on_left_click(false)
            .with_menu_on_right_click(true)
            .build()
            .map_err(|error| format!("构建托盘图标失败：{error}"))?;
        Ok((icon, menu))
    }

    fn build_menu() -> Result<Menu, String> {
        let menu = Menu::new();
        let open_item = MenuItem::with_id(
            MenuId::new(OPEN_MAIN_WINDOW_ID),
            "打开主窗口",
            true,
            None::<Accelerator>,
        );
        let settings_item = MenuItem::with_id(
            MenuId::new(OPEN_SETTINGS_ID),
            "打开设置",
            true,
            None::<Accelerator>,
        );
        let quit_item = MenuItem::with_id(
            MenuId::new(QUIT_ID),
            "退出程序",
            true,
            None::<Accelerator>,
        );
        menu.append_items(&[&open_item, &settings_item, &quit_item])
            .map_err(|error| format!("构建托盘菜单失败：{error}"))?;
        Ok(menu)
    }

    fn load_icon() -> Result<tray_icon::Icon, String> {
        let image = image::load_from_memory(include_bytes!("../assets/logo.png"))
            .map_err(|error| format!("读取托盘图标失败：{error}"))?
            .into_rgba8();
        // 托盘实际显示 16–32 px，先缩到 64 px 再交给系统，避免整图缩放发糊。
        let scaled = image::imageops::resize(
            &image,
            64,
            64,
            image::imageops::FilterType::Lanczos3,
        );
        tray_icon::Icon::from_rgba(scaled.into_raw(), 64, 64)
            .map_err(|error| format!("转换托盘图标失败：{error}"))
    }

    /// 左键抬起时切换主窗口；右键菜单由系统直接弹出，事件忽略。
    fn click_command(event: TrayIconEvent) -> Option<TrayCommand> {
        let TrayIconEvent::Click {
            button,
            button_state: MouseButtonState::Up,
            ..
        } = event
        else {
            return None;
        };
        match button {
            MouseButton::Left => Some(TrayCommand::ToggleMainWindow),
            _ => None,
        }
    }

    fn spawn_menu_event_loop(sender: Sender<TrayCommand>) {
        thread::Builder::new()
            .name("quick-paste-tray-menu".to_owned())
            .spawn(move || {
                let receiver = MenuEvent::receiver();
                while let Ok(event) = receiver.recv() {
                    let command = match event.id.0.as_str() {
                        OPEN_MAIN_WINDOW_ID => TrayCommand::ShowMainWindow,
                        OPEN_SETTINGS_ID => TrayCommand::OpenSettings,
                        QUIT_ID => TrayCommand::Quit,
                        _ => continue,
                    };
                    if sender.try_send(command).is_err() {
                        break;
                    }
                }
            })
            .ok();
    }
}

#[cfg(target_os = "windows")]
pub use windows_impl::*;

#[cfg(not(target_os = "windows"))]
mod stub {
    use async_channel::Sender;
    use gpui_kit::App;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum TrayCommand {
        ToggleMainWindow,
        ShowMainWindow,
        OpenSettings,
        Quit,
    }

    pub fn install(_cx: &mut App, _sender: Sender<TrayCommand>, _visible: bool) {}

    pub fn sync_visible(_cx: &App, _visible: bool) {}
}

#[cfg(not(target_os = "windows"))]
pub use stub::*;
