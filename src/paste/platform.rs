use std::{
    ffi::c_void,
    mem::size_of,
    sync::atomic::{AtomicIsize, Ordering},
    thread,
    time::Duration,
};

use gpui_kit::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::{
    Foundation::{HWND, POINT, RECT},
    Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
    },
    System::Threading::GetCurrentProcessId,
    UI::{
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT,
            KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY, VK_CONTROL, VK_MENU, VK_SHIFT, VK_V,
        },
        WindowsAndMessaging::{
            GetCursorPos, GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId, IsIconic,
            IsWindow, IsWindowVisible, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER,
            SetForegroundWindow, SetWindowPos, ShowWindowAsync, SW_HIDE, SW_RESTORE, SW_SHOW,
        },
    },
};

static MAIN_WINDOW: AtomicIsize = AtomicIsize::new(0);
static LAST_TARGET: AtomicIsize = AtomicIsize::new(0);

pub fn start_foreground_tracker() {
    thread::Builder::new()
        .name("quick-paste-foreground".to_owned())
        .spawn(|| loop {
            // SAFETY: GetForegroundWindow 只读取当前前台窗口句柄。
            let foreground = unsafe { GetForegroundWindow() };
            if !foreground.0.is_null() && !is_current_process(foreground) {
                LAST_TARGET.store(foreground.0 as isize, Ordering::Relaxed);
            }
            thread::sleep(Duration::from_millis(120));
        })
        .ok();
}

pub fn window_hwnd(window: &Window) -> Option<isize> {
    let handle = HasWindowHandle::window_handle(window).ok()?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return None;
    };
    Some(handle.hwnd.get())
}

pub fn set_main_window(hwnd: isize) {
    MAIN_WINDOW.store(hwnd, Ordering::Relaxed);
}

/// 把主窗口移动到鼠标指针所在显示器的工作区中央。
pub fn move_main_window_to_cursor_display() -> bool {
    let hwnd = MAIN_WINDOW.load(Ordering::Relaxed);
    if hwnd == 0 {
        return false;
    }

    let handle = HWND(hwnd as *mut c_void);
    // SAFETY: hwnd 是本进程记录的窗口句柄；坐标与矩形都写入栈上变量。
    unsafe {
        let mut cursor = POINT::default();
        if GetCursorPos(&mut cursor).is_err() {
            return false;
        }
        let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
        if monitor.0.is_null() {
            return false;
        }

        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return false;
        }

        let mut rect = RECT::default();
        if GetWindowRect(handle, &mut rect).is_err() {
            return false;
        }
        let width = rect.right.saturating_sub(rect.left);
        let height = rect.bottom.saturating_sub(rect.top);
        if width <= 0 || height <= 0 {
            return false;
        }
        let work = info.rcWork;
        let x = work.left + (work.right - work.left - width).max(0) / 2;
        let y = work.top + (work.bottom - work.top - height).max(0) / 2;

        SetWindowPos(
            handle,
            None,
            x,
            y,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        )
        .is_ok()
    }
}

pub fn hide_window(window: &Window) {
    let Some(hwnd) = window_hwnd(window) else {
        return;
    };
    // SAFETY: hwnd 来自当前 GPUI 窗口，ShowWindowAsync 只修改该窗口的显示状态。
    unsafe {
        let _ = ShowWindowAsync(HWND(hwnd as *mut c_void), SW_HIDE);
    }
    crate::memory_trim::schedule_hidden_trim();
}

pub fn hide_main_window() {
    let hwnd = MAIN_WINDOW.load(Ordering::Relaxed);
    if hwnd == 0 {
        return;
    }
    // SAFETY: hwnd 是本进程记录的窗口句柄。
    unsafe {
        let _ = ShowWindowAsync(HWND(hwnd as *mut c_void), SW_HIDE);
    }
    crate::memory_trim::schedule_hidden_trim();
}

pub fn show_main_window() {
    let hwnd = MAIN_WINDOW.load(Ordering::Relaxed);
    if hwnd == 0 {
        return;
    }
    let handle = HWND(hwnd as *mut c_void);
    // SAFETY: hwnd 是本进程记录的窗口句柄。
    unsafe {
        let command = if IsIconic(handle).as_bool() {
            SW_RESTORE
        } else {
            SW_SHOW
        };
        let _ = ShowWindowAsync(handle, command);
        let _ = SetForegroundWindow(handle);
    }
    crate::memory_trim::cancel_hidden_trim();
}

pub fn is_main_window_foreground() -> bool {
    let main = MAIN_WINDOW.load(Ordering::Relaxed);
    if main == 0 {
        return false;
    }
    // SAFETY: GetForegroundWindow 只读取当前前台窗口句柄。
    let foreground = unsafe { GetForegroundWindow() };
    foreground.0 as isize == main
}

pub fn is_main_window_visible() -> bool {
    let main = MAIN_WINDOW.load(Ordering::Relaxed);
    if main == 0 {
        return false;
    }
    // SAFETY: hwnd 是本进程记录的窗口句柄，IsWindowVisible 只查询显示状态。
    unsafe { IsWindowVisible(HWND(main as *mut c_void)).as_bool() }
}

/// 隐藏本窗口并把 Ctrl+V 发到刚才的前台窗口。没有可用目标时返回 `false`。
pub fn hide_then_paste(window: &Window) -> bool {
    let Some(target) = valid_target() else {
        return false;
    };

    // 先把前台交还给目标窗口，再隐藏自己：本进程此刻仍持有前台权限，
    // 比隐藏之后再调用 SetForegroundWindow 更可靠。
    let handle = HWND(target as *mut c_void);
    // SAFETY: target 已通过 IsWindow 校验，SetForegroundWindow 只切换前台窗口。
    unsafe {
        let _ = SetForegroundWindow(handle);
    }
    hide_window(window);

    let spawned = thread::Builder::new()
        .name("quick-paste-paste".to_owned())
        .spawn(move || {
            // 热键可能还按着 Ctrl/Shift，等物理按键松开再模拟，避免再次触发本程序热键。
            if !wait_for_modifiers_released() {
                return;
            }
            thread::sleep(Duration::from_millis(60));

            if !wait_until_foreground(target) {
                return;
            }

            // SAFETY: 模拟的组合键只发给当前前台窗口。
            unsafe {
                let _ = send_ctrl_v();
            }
        })
        .is_ok();

    if !spawned {
        show_main_window();
        return false;
    }
    true
}

fn valid_target() -> Option<isize> {
    let target = LAST_TARGET.load(Ordering::Relaxed);
    if target == 0 {
        return None;
    }
    // SAFETY: target 来自 GetForegroundWindow，IsWindow 只查询它是否仍然有效。
    let valid = unsafe { IsWindow(Some(HWND(target as *mut c_void))).as_bool() };
    valid.then_some(target)
}

fn is_current_process(hwnd: HWND) -> bool {
    let mut process_id = 0u32;
    // SAFETY: hwnd 来自 GetForegroundWindow，进程 ID 写入栈上变量。
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut process_id));
    }
    // SAFETY: GetCurrentProcessId 无参数，仅返回本进程 ID。
    process_id == unsafe { GetCurrentProcessId() }
}

fn wait_for_modifiers_released() -> bool {
    for _ in 0..25 {
        // SAFETY: GetAsyncKeyState 只读取按键状态。
        let control = unsafe { GetAsyncKeyState(VK_CONTROL.0 as i32) } < 0;
        let shift = unsafe { GetAsyncKeyState(VK_SHIFT.0 as i32) } < 0;
        let alt = unsafe { GetAsyncKeyState(VK_MENU.0 as i32) } < 0;
        if !control && !shift && !alt {
            return true;
        }
        thread::sleep(Duration::from_millis(20));
    }
    false
}

fn wait_until_foreground(target: isize) -> bool {
    for _ in 0..10 {
        // SAFETY: GetForegroundWindow 只读取当前前台窗口句柄。
        let foreground = unsafe { GetForegroundWindow() };
        if foreground.0 as isize == target {
            return true;
        }
        thread::sleep(Duration::from_millis(30));
    }
    false
}

unsafe fn send_ctrl_v() -> Result<(), ()> {
    let down = [
        keyboard_input(VK_CONTROL, false),
        keyboard_input(VK_V, false),
    ];
    let up = [keyboard_input(VK_V, true), keyboard_input(VK_CONTROL, true)];
    let size = std::mem::size_of::<INPUT>() as i32;

    // SAFETY: down 与 up 都是完整的 INPUT 数组，size 与结构体大小一致。
    let sent_down = unsafe { SendInput(&down, size) };
    thread::sleep(Duration::from_millis(8));
    let sent_up = unsafe { SendInput(&up, size) };

    if sent_down == down.len() as u32 && sent_up == up.len() as u32 {
        Ok(())
    } else {
        Err(())
    }
}

fn keyboard_input(key: VIRTUAL_KEY, key_up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: 0,
                dwFlags: if key_up {
                    KEYEVENTF_KEYUP
                } else {
                    KEYBD_EVENT_FLAGS::default()
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}
