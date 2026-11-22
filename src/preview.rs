//! 悬停预览使用的独立弹出窗口。
//!
//! 预览放在主窗口右侧，避免像内嵌浮层那样盖住下方卡片。窗口使用
//! `WindowKind::PopUp`：置顶、不进入任务栏、创建与显示都不抢焦点。

use gpui_kit::{
    App, AppContext as _, Bounds, Context, Entity, Global, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _, Window,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, div, point, px, size,
    component::{ActiveTheme as _, StyledExt as _, h_flex, v_flex},
};

use crate::paste;

const PREVIEW_WIDTH: f32 = 380.0;
const PREVIEW_HEIGHT: f32 = 420.0;

pub struct PreviewContent {
    pub text: SharedString,
    pub tags: Option<SharedString>,
    pub usage_count: u64,
    pub pinned: bool,
}

struct PreviewWindow {
    content: Option<PreviewContent>,
}

impl Render for PreviewWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let content = self.content.as_ref();
        let text = content
            .map(|content| content.text.clone())
            .unwrap_or_else(|| SharedString::from(""));
        let tags = content.and_then(|content| content.tags.clone());
        let usage_count = content.map_or(0, |content| content.usage_count);
        let pinned = content.is_some_and(|content| content.pinned);

        v_flex()
            .id("preview-window")
            .size_full()
            .gap_3()
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .text_color(theme.popover_foreground)
            .p_4()
            .child(
                div()
                    .id("preview-text")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .text_sm()
                    .line_height(px(20.0))
                    .child(text),
            )
            .children(tags.map(|tags| {
                div()
                    .id("preview-tags")
                    .max_h(px(56.0))
                    .overflow_y_scroll()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(tags)
            }))
            .child(
                h_flex()
                    .gap_3()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(format!("使用 {usage_count} 次"))
                    .children(pinned.then(|| div().child("已置顶"))),
            )
    }
}

struct PreviewWindowService {
    view: Entity<PreviewWindow>,
    hwnd: isize,
}

impl Global for PreviewWindowService {}

pub fn install(cx: &mut App) -> Result<(), String> {
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
            point(px(0.0), px(0.0)),
            size(px(PREVIEW_WIDTH), px(PREVIEW_HEIGHT)),
        ))),
        titlebar: None,
        focus: false,
        show: false,
        kind: WindowKind::PopUp,
        is_movable: false,
        is_resizable: false,
        is_minimizable: false,
        window_background: WindowBackgroundAppearance::Opaque,
        window_decorations: None,
        ..Default::default()
    };

    let mut hwnd = 0;
    let (_, view) = gpui_kit::open_window(options, cx, |window, cx| {
        hwnd = paste::window_hwnd(window).unwrap_or(0);
        cx.new(|_| PreviewWindow { content: None })
    })
    .map_err(|error| format!("无法创建预览窗口：{error}"))?;

    cx.set_global(PreviewWindowService { view, hwnd });
    Ok(())
}

/// 更新预览内容但不移动/显示窗口。返回预览窗口的原生句柄。
///
/// 这个函数在 GPUI 更新期间调用；窗口的移动和显示必须等更新返回后再做，
/// 否则 Windows 会在 `SetWindowPos` 里同步回调并重入 GPUI。
pub fn prepare(content: PreviewContent, cx: &mut App) -> Option<isize> {
    let (view, hwnd) = {
        let service = cx.try_global::<PreviewWindowService>()?;
        (service.view.clone(), service.hwnd)
    };
    view.update(cx, |view, cx| {
        view.content = Some(content);
        cx.notify();
    });
    Some(hwnd)
}

pub fn show_window(hwnd: isize, x: i32, y: i32, width: i32, height: i32) {
    paste::show_preview_window(hwnd, x, y, width, height);
}

pub fn hide(cx: &mut App) {
    let Some((view, hwnd)) = cx
        .try_global::<PreviewWindowService>()
        .map(|service| (service.view.clone(), service.hwnd))
    else {
        return;
    };
    if view.read(cx).content.is_none() {
        return;
    }
    view.update(cx, |view, cx| {
        view.content = None;
        cx.notify();
    });
    paste::hide_preview_window(hwnd);
}

pub fn window_size() -> (f32, f32) {
    (PREVIEW_WIDTH, PREVIEW_HEIGHT)
}
