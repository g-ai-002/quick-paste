mod smooth_scroll;

use std::{
    path::{Path, PathBuf},
    rc::Rc,
};

use gpui_kit::{
    AnyElement, AppContext as _, ClickEvent, ClipboardItem, Context, Entity, Focusable as _,
    InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, Pixels, Render,
    ScrollEvent, SharedString, Size, StatefulInteractiveElement as _, Styled as _, Subscription,
    Window, div, point, px, size,
    component::{
        ActiveTheme as _, Icon, IconName, Sizable as _, StyledExt as _,
        VirtualListScrollHandle, WindowExt as _,
        button::{Button, ButtonVariants},
        h_flex,
        input::{Input, InputEvent, InputState},
        scroll::Scrollbar,
        v_flex, v_virtual_list,
    },
    prelude::FluentBuilder as _,
};

use self::smooth_scroll::SmoothScroll;
use crate::{app::HotkeyStatus, memo, paste};

const MAX_TEXT_LINES: usize = 3;
const ROW_LINE_HEIGHT: f32 = 22.0;
const ROW_TAG_HEIGHT: f32 = 20.0;
const ROW_PADDING_Y: f32 = 12.0;
const ROW_CONTENT_GAP: f32 = 4.0;
const ROW_BORDER: f32 = 2.0;
const ROW_GAP: f32 = 8.0;

pub struct MemoApp {
    source_path: PathBuf,
    data_path: PathBuf,
    memos: Vec<memo::Memo>,
    filtered: Rc<Vec<usize>>,
    row_sizes: Rc<Vec<Size<Pixels>>>,
    list_scroll: VirtualListScrollHandle,
    smooth_scroll: SmoothScroll,
    search: Entity<InputState>,
    filter: String,
    selected: Option<usize>,
    notice: Option<SharedString>,
    hotkey: HotkeyStatus,
    _subscriptions: Vec<Subscription>,
}

impl MemoApp {
    pub fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        source_path: PathBuf,
        data_path: PathBuf,
        hotkey: HotkeyStatus,
    ) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("搜索备忘录…"));

        let subscription = cx.subscribe_in(
            &search,
            window,
            |this, state, event, window, cx| match event {
                InputEvent::Change => {
                    this.filter = state.read(cx).value().to_string();
                    this.refresh_filter();
                    this.scroll_to_top();
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => {
                    this.filter = state.read(cx).value().to_string();
                    this.refresh_filter();
                    if let Some(index) = this.filtered.first().copied() {
                        this.paste_index(index, window, cx);
                    }
                }
                _ => {}
            },
        );

        let mut notice: Option<SharedString> = None;
        let memos = match Self::load_merged(&source_path, &data_path) {
            Ok(memos) => {
                if let Err(error) = memo::save(&data_path, &memos) {
                    notice = Some(format!("无法写入本地状态：{error}").into());
                }
                memos
            }
            Err(error) => {
                notice = Some(format!("无法读取备忘录：{error}").into());
                Vec::new()
            }
        };
        let filtered = Rc::new(memo::matching_indices(&memos, ""));
        let row_sizes = Rc::new(Self::compute_row_sizes(&memos, filtered.as_slice()));

        window.on_window_should_close(cx, |window, _cx| {
            paste::hide_window(window);
            false
        });

        let app = Self {
            source_path,
            data_path,
            memos,
            filtered,
            row_sizes,
            list_scroll: VirtualListScrollHandle::new(),
            smooth_scroll: SmoothScroll::default(),
            search,
            filter: String::new(),
            selected: None,
            notice,
            hotkey,
            _subscriptions: vec![subscription],
        };
        if let Some(hwnd) = paste::window_hwnd(window) {
            paste::set_main_window(hwnd);
        }
        app.search.focus_handle(cx).focus(window, cx);
        app
    }

    pub fn on_hotkey(&mut self, cx: &mut Context<Self>) {
        if paste::is_main_window_foreground() {
            paste::hide_main_window();
        } else {
            self.on_show_request(cx);
        }
    }

    pub fn on_show_request(&mut self, cx: &mut Context<Self>) {
        paste::show_main_window();
        self.reload(cx);
        cx.notify();
    }

    fn render_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let border = cx.theme().border;

        h_flex()
            .w_full()
            .px_3()
            .py_3()
            .border_b_1()
            .border_color(border)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        Input::new(&self.search)
                            .cleanable(true)
                            .prefix(Icon::new(IconName::Search).small()),
                    ),
            )
            .into_any_element()
    }

    fn render_list(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.filtered.is_empty() {
            let content = if self.memos.is_empty() {
                render_empty_state(
                    IconName::FileText,
                    "还没有备忘录",
                    "在内容源 memos.yaml 中添加记录，再点「重新加载」",
                    cx,
                )
            } else {
                render_empty_state(
                    IconName::Search,
                    "没有匹配的备忘录",
                    "换个关键词试试",
                    cx,
                )
            };
            return div()
                .flex_1()
                .min_h_0()
                .w_full()
                .child(content)
                .into_any_element();
        }

        let filtered = self.filtered.clone();
        let list = v_virtual_list(
            cx.entity(),
            "memo-list",
            self.row_sizes.clone(),
            move |this, range, _window, cx| {
                range
                    .filter_map(|index| {
                        filtered
                            .get(index)
                            .copied()
                            .map(|memo_index| this.render_row(memo_index, cx))
                    })
                    .collect::<Vec<_>>()
            },
        )
        .track_scroll(&self.list_scroll)
        .overflow_hidden()
        .flex_1()
        .min_h_0()
        .w_full();

        let scroll_height: f32 = self
            .filtered
            .iter()
            .filter_map(|index| self.memos.get(*index))
            .map(Self::row_height)
            .sum();
        let scroll_size = size(px(0.0), px(scroll_height));

        div()
            .id("memo-list-scroll")
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_hidden()
            .pt_2()
            .on_scroll(cx.listener(Self::on_list_scroll))
            .child(list)
            .child(Scrollbar::vertical(&self.list_scroll).scroll_size(scroll_size))
            .into_any_element()
    }

    fn render_row(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(memo) = self.memos.get(index) else {
            return div().into_any_element();
        };

        let text = SharedString::from(memo.text.clone());
        let tags: Option<SharedString> = if memo.tags.is_empty() {
            None
        } else {
            Some(SharedString::from(
                memo.tags
                    .iter()
                    .map(|tag| format!("#{tag}"))
                    .collect::<Vec<_>>()
                    .join("  "),
            ))
        };
        let line_count = memo::display_line_count(text.as_ref(), MAX_TEXT_LINES);
        let pinned = memo.is_pinned;
        let row_height = Self::row_height(memo);
        let card_height = row_height - ROW_GAP;

        let selected = self.selected == Some(index);
        let selected_bg = cx.theme().muted;
        let hover_bg = cx.theme().muted;
        let border = cx.theme().border;
        let muted_foreground = cx.theme().muted_foreground;

        v_flex()
            .id(("memo-row", index))
            .w_full()
            .h(px(row_height))
            .px_2()
            .pb_2()
            .child(
                v_flex()
                    .relative()
                    .w_full()
                    .h(px(card_height))
                    .gap_1()
                    .overflow_hidden()
                    .rounded_md()
                    .border_1()
                    .border_color(border)
                    .bg(cx.theme().background)
                    .px_3()
                    .py_3()
                    .when(selected, |this| this.bg(selected_bg))
                    .hover(move |this| this.bg(hover_bg))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                        if event.click_count() == 2 {
                            this.paste_index(index, window, cx);
                        } else {
                            this.selected = Some(index);
                            cx.notify();
                        }
                    }))
                    .child(
                        div()
                            .w_full()
                            .line_clamp(line_count)
                            .text_sm()
                            .when(selected, |this| this.pr(px(80.0)))
                            .child(text),
                    )
                    .children(tags.map(|tags| {
                        div()
                            .w_full()
                            .truncate()
                            .text_xs()
                            .text_color(muted_foreground)
                            .when(selected, |this| this.pr(px(80.0)))
                            .child(tags)
                    }))
                    .children(selected.then(|| {
                        div().absolute().top(px(8.0)).right(px(8.0)).child(
                            Button::new(("memo-pin", index))
                                .ghost()
                                .xsmall()
                                .label(if pinned { "取消置顶" } else { "置顶" })
                                .on_click(cx.listener(move |this, _, _window, cx| {
                                    cx.stop_propagation();
                                    this.toggle_pin(index, cx);
                                })),
                        )
                    })),
            )
            .into_any_element()
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted_foreground = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let danger = cx.theme().danger;

        let hint: SharedString = match (&self.hotkey.label, &self.hotkey.error) {
            (Some(label), _) => {
                format!("双击粘贴 · {label} 显示/隐藏 · 关闭窗口后仍在后台运行").into()
            }
            (None, Some(error)) => format!("双击粘贴 · {error}").into(),
            (None, None) => "双击粘贴".into(),
        };

        v_flex()
            .w_full()
            .gap_2()
            .px_3()
            .py_2()
            .border_t_1()
            .border_color(border)
            .children(self.notice.clone().map(|notice| {
                h_flex()
                    .gap_2()
                    .items_start()
                    .child(Icon::new(IconName::TriangleAlert).small().text_color(danger))
                    .child(
                        div()
                            .flex_1()
                            .text_xs()
                            .text_color(danger)
                            .child(notice),
                    )
            }))
            .child(
                h_flex()
                    .w_full()
                    .gap_1()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_xs()
                            .text_color(muted_foreground)
                            .child(hint),
                    )
                    .child(
                        Button::new("reload-memos")
                            .ghost()
                            .xsmall()
                            .icon(IconName::RefreshCw)
                            .tooltip("重新加载 YAML")
                            .on_click(cx.listener(Self::reload_clicked)),
                    )
                    .child(
                        Button::new("open-data")
                            .ghost()
                            .xsmall()
                            .icon(IconName::FolderOpen)
                            .tooltip("打开数据文件")
                            .on_click(cx.listener(Self::open_data_file)),
                    )
                    .child(
                        Button::new("quit-app")
                            .ghost()
                            .xsmall()
                            .label("退出")
                            .on_click(|_, _, cx| cx.quit()),
                    ),
            )
            .into_any_element()
    }

    fn on_list_scroll(
        &mut self,
        event: &ScrollEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .smooth_scroll
            .handle_wheel(&self.list_scroll, event, window)
        {
            cx.stop_propagation();
            cx.notify();
        }
    }

    fn reload_clicked(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.reload(cx);
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        match Self::load_merged(&self.source_path, &self.data_path) {
            Ok(memos) => {
                let selected_text = self
                    .selected
                    .and_then(|index| self.memos.get(index))
                    .map(|memo| memo.text.clone());
                let save_error = memo::save(&self.data_path, &memos).err();

                self.memos = memos;
                self.selected = selected_text
                    .and_then(|text| self.memos.iter().position(|memo| memo.text == text));
                self.notice = save_error.map(|error| {
                    SharedString::from(format!("已重新加载，但写入本地状态失败：{error}"))
                });
                self.refresh_filter();
            }
            Err(error) => {
                self.notice = Some(format!("重新加载失败：{error}").into());
            }
        }
        cx.notify();
    }

    fn open_data_file(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        if let Err(error) = memo::reveal(&self.data_path) {
            self.notice = Some(format!("无法打开数据文件：{error}").into());
            cx.notify();
        }
    }

    fn toggle_pin(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(memo) = self.memos.get(index) else {
            return;
        };
        let text = memo.text.clone();
        let pinned = !memo.is_pinned;

        let mut next = self.memos.clone();
        next[index].is_pinned = pinned;
        memo::sort_for_display(&mut next);

        match memo::save(&self.data_path, &next) {
            Ok(()) => {
                self.memos = next;
                self.selected = self.memos.iter().position(|memo| memo.text == text);
                self.notice = None;
                self.refresh_filter();
                if pinned {
                    self.scroll_to_top();
                }
            }
            Err(error) => {
                self.notice = Some(format!("保存失败：{error}").into());
            }
        }
        cx.notify();
    }

    fn paste_index(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self.memos.get(index).map(|memo| memo.text.clone()) else {
            return;
        };
        if text.trim().is_empty() {
            return;
        }

        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
        let copied = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .is_some_and(|value| value == text);
        if !copied {
            self.notice = Some("写入剪贴板失败，请重试".into());
            cx.notify();
            return;
        }

        if let Err(error) = self.record_usage(index) {
            self.notice = Some(format!("记录使用次数失败：{error}").into());
            cx.notify();
            return;
        }

        self.notice = None;

        if !paste::hide_then_paste(window) {
            self.notice =
                Some("已复制到剪贴板；未找到可粘贴的窗口，请手动粘贴".into());
            cx.notify();
        }
    }

    fn record_usage(&mut self, index: usize) -> Result<(), memo::MemoError> {
        let Some(memo) = self.memos.get(index) else {
            return Ok(());
        };
        let text = memo.text.clone();

        let mut next = self.memos.clone();
        next[index].usage_count = next[index].usage_count.saturating_add(1);
        memo::sort_for_display(&mut next);
        memo::save(&self.data_path, &next)?;

        self.memos = next;
        self.selected = self.memos.iter().position(|memo| memo.text == text);
        self.refresh_filter();
        Ok(())
    }

    fn focus_search(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.search.focus_handle(cx).focus(window, cx);
    }

    /// 重新计算过滤结果与行高缓存；备忘录或过滤词变化后必须调用。
    fn refresh_filter(&mut self) {
        self.smooth_scroll.reset();
        let filtered = memo::matching_indices(&self.memos, &self.filter);
        self.row_sizes = Rc::new(Self::compute_row_sizes(&self.memos, filtered.as_slice()));
        self.filtered = Rc::new(filtered);
    }

    fn scroll_to_top(&mut self) {
        self.smooth_scroll.reset();
        self.list_scroll.set_offset(point(px(0.0), px(0.0)));
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            return;
        }

        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;

        if key == "escape" {
            if !self.filter.is_empty() {
                self.filter.clear();
                self.search
                    .update(cx, |state, cx| state.set_value("", window, cx));
                self.refresh_filter();
                self.scroll_to_top();
                cx.notify();
            } else {
                paste::hide_window(window);
            }
            cx.stop_propagation();
        } else if modifiers.control && !modifiers.alt && !modifiers.shift && key == "f" {
            self.focus_search(window, cx);
            cx.stop_propagation();
        }
    }

    fn load_merged(
        source_path: &Path,
        data_path: &Path,
    ) -> Result<Vec<memo::Memo>, memo::MemoError> {
        let mut memos = if source_path != data_path && source_path.exists() {
            let source = memo::load(source_path)?;
            let local = memo::load_or_create(data_path)?;
            memo::merge(&source, &local)
        } else {
            memo::load_or_create(data_path)?
        };
        memo::sort_for_display(&mut memos);
        Ok(memos)
    }

    fn compute_row_sizes(memos: &[memo::Memo], filtered: &[usize]) -> Vec<Size<Pixels>> {
        filtered
            .iter()
            .filter_map(|index| memos.get(*index))
            .map(|memo| size(px(0.0), px(Self::row_height(memo))))
            .collect()
    }

    fn row_height(memo: &memo::Memo) -> f32 {
        let lines = memo::display_line_count(&memo.text, MAX_TEXT_LINES) as f32;
        let tags = if memo.tags.is_empty() {
            0.0
        } else {
            ROW_CONTENT_GAP + ROW_TAG_HEIGHT
        };
        ROW_PADDING_Y * 2.0 + lines * ROW_LINE_HEIGHT + tags + ROW_BORDER + ROW_GAP
    }
}

impl Render for MemoApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(hwnd) = paste::window_hwnd(window) {
            paste::set_main_window(hwnd);
        }

        // 清除按钮等程序化改值不会发 Change 事件，这里按实际内容兜底同步过滤词。
        let query = self.search.read(cx).value().to_string();
        if query != self.filter {
            self.filter = query;
            self.refresh_filter();
            self.scroll_to_top();
        }
        self.smooth_scroll.advance(&self.list_scroll, window);

        let toolbar = self.render_toolbar(cx);
        let list = self.render_list(cx);
        let footer = self.render_footer(cx);

        v_flex()
            .id("memo-app")
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(toolbar)
            .child(list)
            .child(footer)
            .on_key_down(cx.listener(Self::on_key_down))
    }
}

fn render_empty_state(
    icon: IconName,
    title: &'static str,
    hint: &'static str,
    cx: &mut Context<MemoApp>,
) -> AnyElement {
    let muted_foreground = cx.theme().muted_foreground;

    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap_2()
        .p_6()
        .child(Icon::new(icon).large().text_color(muted_foreground))
        .child(div().text_sm().font_medium().child(title))
        .child(
            div()
                .text_xs()
                .text_color(muted_foreground)
                .text_center()
                .child(hint),
        )
        .into_any_element()
}
