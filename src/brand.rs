//! 应用名称与品牌标识。

use gpui_kit::{AnyElement, IntoElement as _, Styled as _, px, svg};

pub const NAME: &str = "备忘快贴";

/// 备忘纸与置顶图钉融合的 logo，按需缩放到给定边长。
pub fn logo(size: f32) -> AnyElement {
    svg()
        .data(include_bytes!("../assets/logo.svg"))
        .flex_shrink_0()
        .w(px(size))
        .h(px(size))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logo_is_valid_svg() {
        let svg = include_str!("../assets/logo.svg");
        assert!(svg.contains("<svg"));
        assert!(svg.contains("viewBox=\"0 0 64 64\""));
    }
}
