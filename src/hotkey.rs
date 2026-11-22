//! 全局快捷键：注册、切换与状态展示。

use async_channel::Receiver;
use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
    hotkey::{Code, HotKey, Modifiers as HotkeyModifiers},
};
use gpui_kit::{App, Global, SharedString};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HotkeyChoice {
    #[default]
    CtrlShiftX,
    CtrlAltV,
    CtrlShiftV,
    CtrlShiftC,
}

impl HotkeyChoice {
    pub const ALL: [Self; 4] = [
        Self::CtrlShiftX,
        Self::CtrlAltV,
        Self::CtrlShiftV,
        Self::CtrlShiftC,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::CtrlShiftX => "Ctrl+Shift+X",
            Self::CtrlAltV => "Ctrl+Alt+V",
            Self::CtrlShiftV => "Ctrl+Shift+V",
            Self::CtrlShiftC => "Ctrl+Shift+C",
        }
    }

    fn hotkey(self) -> HotKey {
        let modifiers = match self {
            Self::CtrlShiftX | Self::CtrlShiftV | Self::CtrlShiftC => {
                HotkeyModifiers::CONTROL | HotkeyModifiers::SHIFT
            }
            Self::CtrlAltV => HotkeyModifiers::CONTROL | HotkeyModifiers::ALT,
        };
        let code = match self {
            Self::CtrlShiftX => Code::KeyX,
            Self::CtrlAltV | Self::CtrlShiftV => Code::KeyV,
            Self::CtrlShiftC => Code::KeyC,
        };
        HotKey::new(Some(modifiers), code)
    }
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
    match register(preferred, cx) {
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

    match register(choice, cx) {
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
    let manager = cx.global_mut::<HotkeyManager>();
    if let Some(current) = manager.current.take() {
        let _ = manager.manager.unregister(current);
    }

    let hotkey = choice.hotkey();
    match manager.manager.register(hotkey) {
        Ok(()) => {
            manager.current = Some(hotkey);
            Ok(choice.label().into())
        }
        Err(error) => Err(format!("无法注册 {}：{error}", choice.label())),
    }
}
