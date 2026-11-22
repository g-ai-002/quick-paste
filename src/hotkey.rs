//! 全局快捷键：注册、切换与状态展示。

use async_channel::Receiver;
use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
    hotkey::{Code, HotKey, Modifiers as HotkeyModifiers},
};
use gpui_kit::{App, Global, SharedString};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HotkeyChoice {
    #[default]
    CtrlShiftX,
    CtrlAltV,
    CtrlShiftV,
    CtrlShiftC,
    /// 用户录制的组合键，例如 `ctrl+shift+k`。
    Custom(String),
}

impl HotkeyChoice {
    pub fn label(&self) -> String {
        match self {
            Self::CtrlShiftX => "Ctrl+Shift+X".to_owned(),
            Self::CtrlAltV => "Ctrl+Alt+V".to_owned(),
            Self::CtrlShiftV => "Ctrl+Shift+V".to_owned(),
            Self::CtrlShiftC => "Ctrl+Shift+C".to_owned(),
            Self::Custom(binding) => pretty_binding(&binding.to_ascii_lowercase()),
        }
    }

    fn hotkey(&self) -> Option<HotKey> {
        match self {
            Self::CtrlShiftX => Some(HotKey::new(
                Some(HotkeyModifiers::CONTROL | HotkeyModifiers::SHIFT),
                Code::KeyX,
            )),
            Self::CtrlAltV => Some(HotKey::new(
                Some(HotkeyModifiers::CONTROL | HotkeyModifiers::ALT),
                Code::KeyV,
            )),
            Self::CtrlShiftV => Some(HotKey::new(
                Some(HotkeyModifiers::CONTROL | HotkeyModifiers::SHIFT),
                Code::KeyV,
            )),
            Self::CtrlShiftC => Some(HotKey::new(
                Some(HotkeyModifiers::CONTROL | HotkeyModifiers::SHIFT),
                Code::KeyC,
            )),
            Self::Custom(binding) => binding.parse().ok(),
        }
    }
}

/// 把一次按键转换成可持久化的组合键字符串。
///
/// 至少需要一个 Ctrl / Alt / Shift 修饰键；返回的字符串可直接被
/// `global_hotkey::HotKey` 解析。
pub fn capture(ctrl: bool, alt: bool, shift: bool, key: &str) -> Option<String> {
    if !ctrl && !alt && !shift {
        return None;
    }
    let key = key.trim().to_ascii_lowercase();
    if key.is_empty() {
        return None;
    }

    let mut binding = String::new();
    if ctrl {
        binding.push_str("ctrl+");
    }
    if alt {
        binding.push_str("alt+");
    }
    if shift {
        binding.push_str("shift+");
    }
    binding.push_str(&key);

    binding.parse::<HotKey>().ok().map(|_| binding)
}

fn pretty_binding(binding: &str) -> String {
    binding
        .split('+')
        .map(|part| match part {
            "ctrl" | "control" => "Ctrl".to_owned(),
            "alt" | "option" => "Alt".to_owned(),
            "shift" => "Shift".to_owned(),
            "meta" | "super" | "cmd" | "command" => "Win".to_owned(),
            "space" => "Space".to_owned(),
            "enter" | "return" => "Enter".to_owned(),
            "tab" => "Tab".to_owned(),
            "backspace" => "Backspace".to_owned(),
            "delete" => "Delete".to_owned(),
            "insert" => "Insert".to_owned(),
            "home" => "Home".to_owned(),
            "end" => "End".to_owned(),
            "pageup" => "PageUp".to_owned(),
            "pagedown" => "PageDown".to_owned(),
            "up" | "arrowup" => "Up".to_owned(),
            "down" | "arrowdown" => "Down".to_owned(),
            "left" | "arrowleft" => "Left".to_owned(),
            "right" | "arrowright" => "Right".to_owned(),
            "backquote" => "`".to_owned(),
            "backslash" => "\\".to_owned(),
            "bracketleft" => "[".to_owned(),
            "bracketright" => "]".to_owned(),
            "comma" => ",".to_owned(),
            "period" => ".".to_owned(),
            "quote" => "'".to_owned(),
            "semicolon" => ";".to_owned(),
            "slash" => "/".to_owned(),
            "minus" => "-".to_owned(),
            "equal" => "=".to_owned(),
            "escape" | "esc" => "Esc".to_owned(),
            other if other.len() == 1 => other.to_ascii_uppercase(),
            other if other.starts_with('f')
                && other[1..].chars().all(|ch| ch.is_ascii_digit()) =>
            {
                other.to_ascii_uppercase()
            }
            other => {
                let mut chars = other.chars();
                match chars.next() {
                    Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                    None => String::new(),
                }
            }
        })
        .collect::<Vec<_>>()
        .join("+")
}

struct HotkeyManager {
    manager: GlobalHotKeyManager,
    current: Option<HotKey>,
}

impl Global for HotkeyManager {}

#[derive(Clone)]
pub struct HotkeyState {
    pub choice: HotkeyChoice,
    pub label: Option<SharedString>,
    pub error: Option<SharedString>,
}

impl Global for HotkeyState {}

pub fn state(cx: &App) -> HotkeyState {
    cx.try_global::<HotkeyState>()
        .cloned()
        .unwrap_or(HotkeyState {
            choice: HotkeyChoice::default(),
            label: None,
            error: Some("全局快捷键尚未初始化".into()),
        })
}

pub fn install(cx: &mut App, preferred: HotkeyChoice) -> Receiver<()> {
    let (sender, receiver) = async_channel::unbounded();
    GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
        if event.state == HotKeyState::Pressed {
            let _ = sender.try_send(());
        }
    }));

    let manager = match GlobalHotKeyManager::new() {
        Ok(manager) => manager,
        Err(error) => {
            cx.set_global(HotkeyState {
                choice: preferred,
                label: None,
                error: Some(format!("全局快捷键初始化失败：{error}").into()),
            });
            return receiver;
        }
    };

    cx.set_global(HotkeyManager {
        manager,
        current: None,
    });
    match register(preferred.clone(), cx) {
        Ok(label) => {
            cx.set_global(HotkeyState {
                choice: preferred,
                label: Some(label),
                error: None,
            });
        }
        Err(error) => {
            cx.set_global(HotkeyState {
                choice: preferred,
                label: None,
                error: Some(error.into()),
            });
        }
    }
    receiver
}

pub fn set_choice(choice: HotkeyChoice, cx: &mut App) -> Result<(), String> {
    let previous = state(cx);
    let previous_hotkey = cx
        .try_global::<HotkeyManager>()
        .and_then(|manager| manager.current);
    if !cx.has_global::<HotkeyManager>() {
        return Err("全局快捷键管理器未初始化".to_owned());
    }
    if previous.choice == choice && previous.error.is_none() {
        return Ok(());
    }

    match register(choice.clone(), cx) {
        Ok(label) => {
            cx.set_global(HotkeyState {
                choice,
                label: Some(label),
                error: None,
            });
            cx.refresh_windows();
            Ok(())
        }
        Err(error) => {
            if let Some(hotkey) = previous_hotkey {
                let manager = cx.global_mut::<HotkeyManager>();
                if manager.manager.register(hotkey).is_ok() {
                    manager.current = Some(hotkey);
                }
            }
            cx.set_global(HotkeyState {
                choice: previous.choice,
                label: previous.label,
                error: Some(error.clone().into()),
            });
            cx.refresh_windows();
            Err(error)
        }
    }
}

fn register(choice: HotkeyChoice, cx: &mut App) -> Result<SharedString, String> {
    let label = choice.label();
    let hotkey = choice
        .hotkey()
        .ok_or_else(|| format!("无法识别快捷键：{label}"))?;

    let manager = cx.global_mut::<HotkeyManager>();
    if let Some(current) = manager.current.take() {
        let _ = manager.manager.unregister(current);
    }

    match manager.manager.register(hotkey) {
        Ok(()) => {
            manager.current = Some(hotkey);
            Ok(SharedString::from(label))
        }
        Err(error) => Err(format!("无法注册 {label}：{error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_modifier_combination() {
        assert_eq!(
            capture(true, false, true, "k"),
            Some("ctrl+shift+k".to_owned())
        );
        assert_eq!(
            capture(false, true, true, "F8"),
            Some("alt+shift+f8".to_owned())
        );
    }

    #[test]
    fn rejects_bare_key_and_invalid_binding() {
        assert_eq!(capture(false, false, false, "k"), None);
        assert_eq!(capture(true, false, false, "control"), None);
        assert!(HotkeyChoice::Custom("ctrl+shift+control".into())
            .hotkey()
            .is_none());
    }

    #[test]
    fn custom_label_is_human_readable() {
        assert_eq!(
            HotkeyChoice::Custom("ctrl+alt+f8".into()).label(),
            "Ctrl+Alt+F8"
        );
        assert_eq!(
            HotkeyChoice::Custom("ctrl+shift+;".into()).label(),
            "Ctrl+Shift+;"
        );
    }
}
