//! 程序设置：持久化、运行时全局状态与主题应用。

use std::{
    fs,
    io::Write as _,
    path::{Path, PathBuf},
};

use atomic_write_file::AtomicWriteFile;
use gpui_kit::{App, Global, Window};
use serde::{Deserialize, Serialize};

use crate::hotkey::HotkeyChoice;

const HEADER: &str = "\
# 快速粘贴 · 设置
# 由程序自动维护；手动修改后重启程序生效。
";

pub const TAG_SIDEBAR_WIDTH_DEFAULT: f32 = 128.0;
pub const TAG_SIDEBAR_WIDTH_MIN: f32 = 80.0;
pub const TAG_SIDEBAR_WIDTH_MAX: f32 = 240.0;
pub const TAG_SIDEBAR_WIDTH_STEP: f32 = 8.0;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeChoice {
    pub const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];

    pub const fn label(self) -> &'static str {
        match self {
            Self::System => "跟随系统",
            Self::Light => "浅色",
            Self::Dark => "深色",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClickAction {
    #[default]
    DoubleClick,
    SingleClick,
}

impl ClickAction {
    pub const ALL: [Self; 2] = [Self::DoubleClick, Self::SingleClick];

    pub const fn label(self) -> &'static str {
        match self {
            Self::DoubleClick => "双击",
            Self::SingleClick => "单击",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PasteAction {
    #[default]
    AutoPaste,
    CopyOnly,
}

impl PasteAction {
    pub const ALL: [Self; 2] = [Self::AutoPaste, Self::CopyOnly];

    pub const fn label(self) -> &'static str {
        match self {
            Self::AutoPaste => "自动粘贴",
            Self::CopyOnly => "仅复制",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// 唤出窗口时移动到鼠标所在的显示器。
    pub show_on_cursor_display: bool,
    /// 程序启动后不显示主窗口，只驻留后台。
    pub start_hidden: bool,
    /// 主窗口失去焦点时自动隐藏。
    pub auto_hide_on_blur: bool,
    /// 未完整显示的备忘录支持悬停预览。
    pub hover_preview: bool,
    pub hover_preview_delay_ms: u64,
    pub max_text_lines: usize,
    pub theme: ThemeChoice,
    pub card_radius: f32,
    pub tag_sidebar_width: f32,
    pub smooth_scroll: bool,
    pub hotkey: HotkeyChoice,
    pub click_action: ClickAction,
    pub paste_action: PasteAction,
    pub track_usage: bool,
    pub clear_search_after_paste: bool,
    pub enter_pastes_first: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            show_on_cursor_display: true,
            start_hidden: false,
            auto_hide_on_blur: true,
            hover_preview: true,
            hover_preview_delay_ms: 400,
            max_text_lines: 3,
            theme: ThemeChoice::System,
            card_radius: 6.0,
            tag_sidebar_width: TAG_SIDEBAR_WIDTH_DEFAULT,
            smooth_scroll: true,
            hotkey: HotkeyChoice::default(),
            click_action: ClickAction::DoubleClick,
            paste_action: PasteAction::AutoPaste,
            track_usage: true,
            clear_search_after_paste: true,
            enter_pastes_first: true,
        }
    }
}

impl Settings {
    pub fn normalized(mut self) -> Self {
        self.hover_preview_delay_ms = self.hover_preview_delay_ms.clamp(200, 1_200);
        self.max_text_lines = self.max_text_lines.clamp(2, 5);
        self.card_radius = self.card_radius.clamp(0.0, 24.0);
        self.tag_sidebar_width = self
            .tag_sidebar_width
            .clamp(TAG_SIDEBAR_WIDTH_MIN, TAG_SIDEBAR_WIDTH_MAX)
            .round();
        self
    }
}

struct SettingsStore {
    path: PathBuf,
    settings: Settings,
}

impl Global for SettingsStore {}

pub fn install(cx: &mut App, path: PathBuf, settings: Settings) {
    cx.set_global(SettingsStore {
        path,
        settings: settings.normalized(),
    });
}

pub fn current(cx: &App) -> Settings {
    cx.try_global::<SettingsStore>()
        .map(|store| store.settings.clone())
        .unwrap_or_default()
}

pub fn commit(cx: &mut App, settings: Settings) -> Result<(), String> {
    let settings = settings.normalized();
    let Some(path) = cx
        .try_global::<SettingsStore>()
        .map(|store| store.path.clone())
    else {
        return Err("设置存储尚未初始化".to_owned());
    };
    save(&path, &settings)?;
    cx.set_global(SettingsStore {
        path,
        settings,
    });
    cx.refresh_windows();
    Ok(())
}

pub fn load(path: &Path) -> Result<Settings, String> {
    if !path.exists() {
        return Ok(Settings::default());
    }
    let raw = fs::read_to_string(path).map_err(|error| {
        format!("无法读取设置文件 {}：{error}", path.display())
    })?;
    let content = raw.strip_prefix('\u{feff}').unwrap_or(&raw);
    let only_comments = content
        .lines()
        .all(|line| line.trim().is_empty() || line.trim_start().starts_with('#'));
    if only_comments {
        return Ok(Settings::default());
    }
    let settings: Settings = serde_yaml::from_str(content)
        .map_err(|error| format!("设置文件格式无效：{error}"))?;
    Ok(settings.normalized())
}

pub fn save(path: &Path, settings: &Settings) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!("无法创建设置目录 {}：{error}", parent.display())
        })?;
    }
    let yaml = serde_yaml::to_string(settings)
        .map_err(|error| format!("无法序列化设置：{error}"))?;
    let content = format!("{HEADER}{yaml}");
    let mut file = AtomicWriteFile::open(path)
        .map_err(|error| format!("无法写入设置文件 {}：{error}", path.display()))?;
    file.write_all(content.as_bytes())
        .map_err(|error| format!("无法写入设置文件 {}：{error}", path.display()))?;
    file.as_file()
        .sync_all()
        .map_err(|error| format!("无法同步设置文件 {}：{error}", path.display()))?;
    file.commit()
        .map_err(|error| format!("无法保存设置文件 {}：{error}", path.display()))?;
    Ok(())
}

pub fn apply_theme(choice: ThemeChoice, window: Option<&mut Window>, cx: &mut App) {
    use gpui_kit::WindowAppearance;
    use gpui_kit::component::{Theme, ThemeMode};

    cx.set_window_appearance(match choice {
        ThemeChoice::System => None,
        ThemeChoice::Light => Some(WindowAppearance::Light),
        ThemeChoice::Dark => Some(WindowAppearance::Dark),
    });

    match choice {
        ThemeChoice::System => Theme::sync_system_appearance(None, cx),
        ThemeChoice::Light => Theme::change(ThemeMode::Light, window, cx),
        ThemeChoice::Dark => Theme::change(ThemeMode::Dark, window, cx),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_stable() {
        let settings = Settings::default();
        assert!(settings.show_on_cursor_display);
        assert!(settings.auto_hide_on_blur);
        assert!(settings.hover_preview);
        assert_eq!(settings.max_text_lines, 3);
        assert_eq!(settings.theme, ThemeChoice::System);
        assert_eq!(settings.tag_sidebar_width, TAG_SIDEBAR_WIDTH_DEFAULT);
        assert_eq!(settings.hotkey, HotkeyChoice::CtrlShiftX);
        assert!(settings.clear_search_after_paste);
    }

    #[test]
    fn normalizes_out_of_range_values() {
        let settings = Settings {
            hover_preview_delay_ms: 10,
            max_text_lines: 99,
            card_radius: -8.0,
            tag_sidebar_width: 999.0,
            ..Settings::default()
        }
        .normalized();

        assert_eq!(settings.hover_preview_delay_ms, 200);
        assert_eq!(settings.max_text_lines, 5);
        assert_eq!(settings.card_radius, 0.0);
        assert_eq!(settings.tag_sidebar_width, TAG_SIDEBAR_WIDTH_MAX);

        let settings = Settings {
            tag_sidebar_width: 10.0,
            ..Settings::default()
        }
        .normalized();
        assert_eq!(settings.tag_sidebar_width, TAG_SIDEBAR_WIDTH_MIN);
    }

    #[test]
    fn saves_and_loads_round_trip() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("settings.yaml");
        let settings = Settings {
            show_on_cursor_display: false,
            start_hidden: true,
            hover_preview_delay_ms: 800,
            max_text_lines: 5,
            theme: ThemeChoice::Dark,
            card_radius: 10.0,
            tag_sidebar_width: 176.0,
            hotkey: HotkeyChoice::Custom("ctrl+alt+c".into()),
            click_action: ClickAction::SingleClick,
            paste_action: PasteAction::CopyOnly,
            track_usage: false,
            clear_search_after_paste: true,
            enter_pastes_first: false,
            ..Settings::default()
        };

        save(&path, &settings).expect("save");
        assert_eq!(load(&path).expect("load"), settings);

        let content = fs::read_to_string(&path).expect("read");
        assert!(content.starts_with("# 快速粘贴"));
    }

    #[test]
    fn missing_file_uses_defaults() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("missing.yaml");
        assert_eq!(load(&path).expect("load"), Settings::default());
        assert!(!path.exists());
    }

    #[test]
    fn missing_field_uses_defaults() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("settings.yaml");
        fs::write(&path, "theme: dark\n").expect("write");

        let settings = load(&path).expect("load");
        assert_eq!(settings.theme, ThemeChoice::Dark);
        assert_eq!(settings.tag_sidebar_width, TAG_SIDEBAR_WIDTH_DEFAULT);
    }
}
