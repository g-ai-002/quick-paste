//! 把备忘录文本粘贴回刚才的前台窗口。
//!
//! 思路：后台线程持续记录“不属于本进程的前台窗口”，粘贴时先把前台交还给
//! 那个窗口，再隐藏本窗口，最后模拟 Ctrl+V。

#[cfg(target_os = "windows")]
mod platform;

#[cfg(target_os = "windows")]
pub use platform::*;

#[cfg(not(target_os = "windows"))]
mod stub {
    use gpui_kit::Window;

    pub fn start_foreground_tracker() {}
    pub fn window_hwnd(_window: &Window) -> Option<isize> {
        None
    }
    pub fn set_main_window(_hwnd: isize) {}
    pub fn move_main_window_to_cursor_display() -> bool {
        false
    }
    pub fn show_preview_window(_hwnd: isize, _x: i32, _y: i32, _width: i32, _height: i32) {}
    pub fn hide_preview_window(_hwnd: isize) {}
    pub fn hide_window(_window: &Window) {}
    pub fn hide_main_window() {}
    pub fn show_main_window() {}
    pub fn is_main_window_visible() -> bool {
        false
    }
    pub fn is_main_window_foreground() -> bool {
        false
    }
    pub fn hide_then_paste(_window: &Window) -> bool {
        false
    }
}

#[cfg(not(target_os = "windows"))]
pub use stub::*;
