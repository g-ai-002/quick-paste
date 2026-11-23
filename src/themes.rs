//! GPUI Kit 内置主题集合。
//!
//! `gpui-kit` 的 crate 只内置默认浅色/深色主题，仓库 `themes/` 下的 20+ 主题
//! 需要随应用一起提供。这里在编译期嵌入 JSON，启动时注册到 `ThemeRegistry`。

use gpui_kit::{App, component::ThemeRegistry};

use crate::logging;

const THEME_SETS: &[(&str, &str)] = &[
    ("adventure", include_str!("themes/adventure.json")),
    ("alduin", include_str!("themes/alduin.json")),
    ("asciinema", include_str!("themes/asciinema.json")),
    ("aurora", include_str!("themes/aurora.json")),
    ("ayu", include_str!("themes/ayu.json")),
    ("catppuccin", include_str!("themes/catppuccin.json")),
    ("everforest", include_str!("themes/everforest.json")),
    ("fahrenheit", include_str!("themes/fahrenheit.json")),
    ("flexoki", include_str!("themes/flexoki.json")),
    ("gruvbox", include_str!("themes/gruvbox.json")),
    ("harper", include_str!("themes/harper.json")),
    ("hybrid", include_str!("themes/hybrid.json")),
    ("jellybeans", include_str!("themes/jellybeans.json")),
    ("kibble", include_str!("themes/kibble.json")),
    ("macos-classic", include_str!("themes/macos-classic.json")),
    ("mellifluous", include_str!("themes/mellifluous.json")),
    ("molokai", include_str!("themes/molokai.json")),
    ("solarized", include_str!("themes/solarized.json")),
    ("spaceduck", include_str!("themes/spaceduck.json")),
    ("tokyonight", include_str!("themes/tokyonight.json")),
    ("twilight", include_str!("themes/twilight.json")),
];

pub fn install(cx: &mut App) {
    let registry = ThemeRegistry::global_mut(cx);
    for (name, json) in THEME_SETS {
        if let Err(error) = registry.load_themes_from_str(json) {
            logging::write(format!("加载内置主题 {name} 失败：{error}"));
        }
    }
}
