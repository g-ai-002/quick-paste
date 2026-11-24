//! 应用名称与品牌标识。

use std::sync::{Arc, LazyLock};

use gpui_kit::{AnyElement, Image, ImageFormat, IntoElement as _, Styled as _, img, px};

pub const NAME: &str = "备忘快贴";

static LOGO_IMAGE: LazyLock<Arc<Image>> = LazyLock::new(|| {
    Arc::new(Image::from_bytes(
        ImageFormat::Svg,
        include_bytes!("../assets/logo.svg").to_vec(),
    ))
});

/// 备忘纸与置顶图钉融合的 logo，按需缩放到给定边长。
///
/// 用 `img` 而不是 `svg`：GPUI 的 `svg()` 只在 `text.color` 存在时绘制，
/// 并且按单色 alpha mask 渲染，会丢掉 logo 的蓝底、白纸和橙色图钉。
pub fn logo(size: f32) -> AnyElement {
    img((*LOGO_IMAGE).clone())
        .flex_shrink_0()
        .w(px(size))
        .h(px(size))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    #[test]
    fn logo_is_valid_svg() {
        let svg = include_str!("../assets/logo.svg");
        assert!(svg.contains("<svg"));
        assert!(svg.contains("viewBox=\"0 0 64 64\""));
    }
}
