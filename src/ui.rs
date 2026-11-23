mod smooth_scroll;

use std::{
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};

use gpui_kit::{
    AnyElement, App, AppContext as _, ClickEvent, ClipboardItem, Context, Entity,
    Focusable as _, InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _,
    Pixels, Render, ScrollEvent, SharedString, Size,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window, div, point, px, size,
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
use crate::{
    hotkey, logging, memo, paste, preview, settings, settings_window, source,
};

const ROW_LINE_HEIGHT: f32 = 22.0;
const ROW_TAG_HEIGHT: f32 = 20.0;
const ROW_MARGIN_X: f32 = 8.0;
const ROW_PADDING_X: f32 = 12.0;
const ROW_PADDING_Y: f32 = 12.0;
const ROW_CONTENT_GAP: f32 = 4.0;
const ROW_GAP: f32 = 8.0;
const PIN_SIZE: f32 = 24.0;
const PIN_INSET: f32 = 6.0;
const ROW_TEXT_RIGHT: f32 = PIN_SIZE + PIN_INSET + 6.0;
const TAG_ROW_HEIGHT: f32 = 30.0;
// 卡片正文按约 48 个半角字符估算换行；列表再窄会让估算失准，因此标签栏不能挤占到这里以下。
const MIN_MEMO_LIST_WIDTH: f32 = 410.0;

pub struct MemoApp {
    source: source::ContentSource,
    data_source: source::DataSourceConfig,
    data_path: PathBuf,
    memos: Vec<memo::Memo>,
    tags: Vec<String>,
    active_tag: Option<String>,
    settings: settings::Settings,
    filtered: Rc<Vec<usize>>,
    row_sizes: Rc<Vec<Size<Pixels>>>,
    list_scroll: VirtualListScrollHandle,
    smooth_scroll: SmoothScroll,
    search: Entity<InputState>,
    filter: String,
    hovered: Option<usize>,
    notice: Option<SharedString>,
    window_active: bool,
    suppress_blur_until: Option<Instant>,
    preview_generation: u64,
    loading: bool,
    pending_reload: bool,
    load_generation: u64,
    pending_window_size: Option<(f32, f32)>,
    window_size_generation: u64,
    _subscriptions: Vec<Subscription>,
}

impl MemoApp {
    pub fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        data_source: source::DataSourceConfig,
        data_path: PathBuf,
    ) -> Self {
        let app_settings = settings::current(cx);
        let content_source = source::ContentSource::resolve(&data_source, &data_path);
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
                    if !this.settings.enter_pastes_first {
                        return;
                    }
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
        let mut loading = false;
        let mut pending_reload = false;
        let mut memos = if content_source.is_remote() {
            pending_reload = true;
            loading = true;
            match memo::load(&data_path) {
                Ok(mut cached) => {
                    memo::sort_for_display(&mut cached);
                    cached
                }
                Err(error) if data_path.exists() => {
                    logging::write(format!("启动时读取本地缓存失败：{error}"));
                    notice = Some(format!("无法读取本地缓存：{error}").into());
                    Vec::new()
                }
                Err(_) => Vec::new(),
            }
        } else {
            match source::load_merged(&content_source, &data_source, &data_path) {
                Ok(memos) => {
                    if let Err(error) = memo::save(&data_path, &memos) {
                        logging::write(format!("启动时写入本地状态失败：{error}"));
                        notice = Some(format!("无法写入本地状态：{error}").into());
                    }
                    memos
                }
                Err(error) => {
                    logging::write(format!("启动时读取备忘录失败：{error}"));
                    let mut cached = memo::load(&data_path).unwrap_or_default();
                    if cached.is_empty() {
                        notice = Some(format!("无法读取备忘录：{error}").into());
                    } else {
                        memo::sort_for_display(&mut cached);
                        notice = Some(
                            format!("无法读取备忘录：{error}；已显示本地缓存").into(),
                        );
                    }
                    cached
                }
            }
        };
        if content_source.is_remote() && notice.is_none() {
            notice = Some("正在加载远端数据源…".into());
        }
        let tags = memo::sorted_tags(&memos);
        let filtered = Rc::new(memo::matching_indices(&memos, ""));
        let row_sizes = Rc::new(Self::compute_row_sizes(
            &memos,
            filtered.as_slice(),
            &app_settings,
        ));

        window.on_window_should_close(cx, |window, cx| {
            save_window_size(window, cx);
            preview::hide(cx);
            paste::hide_window(window);
            false
        });

        let activation_subscription = cx.observe_window_activation(window, |this, window, cx| {
            let active = window.is_window_active();
            let was_active = this.window_active;
            this.window_active = active;
            if was_active && !active {
                this.remember_window_size(window);
                this.commit_window_size(cx);
                this.cancel_preview(cx);
                let suppress = this
                    .suppress_blur_until
                    .is_some_and(|until| Instant::now() < until);
                if suppress {
                    this.suppress_blur_until = None;
                    this.window_active = true;
                } else if this.settings.auto_hide_on_blur {
                    this.cancel_preview(cx);
                    paste::hide_window(window);
                }
            }
            cx.notify();
        });
        let bounds_subscription = cx.observe_window_bounds(window, |this, window, cx| {
            this.on_window_resized(window, cx);
        });
        let appearance_subscription = cx.observe_window_appearance(window, |_, _window, cx| {
            if settings::current(cx).theme == settings::ThemeChoice::System {
                gpui_kit::component::Theme::sync_system_appearance(None, cx);
            }
        });

        let app = Self {
            source: content_source,
            data_source,
            data_path,
            memos,
            tags,
            active_tag: None,
            settings: app_settings,
            filtered,
            row_sizes,
            list_scroll: VirtualListScrollHandle::new(),
            smooth_scroll: SmoothScroll::default(),
            search,
            filter: String::new(),
            hovered: None,
            notice,
            window_active: window.is_window_active(),
            suppress_blur_until: None,
            preview_generation: 0,
            loading,
            pending_reload,
            load_generation: 0,
            pending_window_size: None,
            window_size_generation: 0,
            _subscriptions: vec![
                subscription,
                activation_subscription,
                bounds_subscription,
                appearance_subscription,
            ],
        };
        if let Some(hwnd) = paste::window_hwnd(window) {
            paste::set_main_window(hwnd);
        }
        app.search.focus_handle(cx).focus(window, cx);
        app
    }

    pub fn on_hotkey(&mut self, cx: &mut Context<Self>) {
        if paste::is_main_window_foreground() {
            self.commit_window_size(cx);
            self.window_active = false;
            self.cancel_preview(cx);
            paste::hide_main_window();
        } else {
            self.on_show_request(cx);
        }
    }

    pub fn on_show_request(&mut self, cx: &mut Context<Self>) {
        self.commit_window_size(cx);
        self.cancel_preview(cx);
        self.sync_settings(cx);
        if self.settings.theme == settings::ThemeChoice::System {
            gpui_kit::component::Theme::sync_system_appearance(None, cx);
        }
        if self.settings.show_on_cursor_display {
            Self::schedule_show_on_cursor_display(cx);
        } else {
            paste::show_main_window();
        }
        self.window_active = true;
        self.suppress_blur_until = None;
        self.reload(cx);
        cx.notify();
    }

    fn schedule_show_on_cursor_display(cx: &mut Context<Self>) {
        cx.spawn(async move |_this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1))
                .await;
            paste::move_main_window_to_cursor_display();
            paste::show_main_window();
        })
        .detach();
    }

    fn schedule_preview(&mut self, index: usize, window: &Window, cx: &mut Context<Self>) {
        // 换到另一张卡片时先收起旧预览，避免旧内容在延迟期间继续停留。
        self.cancel_preview(cx);
        let Some(memo) = self.memos.get(index) else {
            return;
        };
        let text = SharedString::from(memo.text.clone());
        let tags = if memo.tags.is_empty() {
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
        let tags_truncated = tags.as_ref().is_some_and(|tags| {
            tags.as_ref()
                .chars()
                .map(|ch| if ch.is_ascii() { 1 } else { 2 })
                .sum::<usize>()
                > 60
        });
        let truncated = memo::is_truncated(text.as_ref(), self.settings.max_text_lines);
        if !self.settings.hover_preview || !(truncated || tags_truncated) {
            self.cancel_preview(cx);
            return;
        }
        let Some((x, y, width, height)) = self.preview_physical_bounds(window, cx) else {
            self.cancel_preview(cx);
            return;
        };

        let content = preview::PreviewContent {
            text,
            tags,
            usage_count: memo.usage_count,
            pinned: memo.is_pinned,
        };
        let delay = self.settings.hover_preview_delay_ms;
        self.preview_generation = self.preview_generation.wrapping_add(1);
        let generation = self.preview_generation;
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(delay))
                .await;
            let hwnd = weak
                .update(cx, |this, cx| {
                    if this.preview_generation != generation
                        || this.hovered != Some(index)
                        || this.smooth_scroll.is_active()
                    {
                        return None;
                    }
                    preview::prepare(content, cx)
                })
                .ok()
                .flatten();
            if let Some(hwnd) = hwnd {
                preview::show_window(hwnd, x, y, width, height);
            }
        })
        .detach();
    }

    fn cancel_preview(&mut self, cx: &mut Context<Self>) {
        self.preview_generation = self.preview_generation.wrapping_add(1);
        preview::hide(cx);
    }

    fn preview_physical_bounds(
        &self,
        window: &Window,
        cx: &App,
    ) -> Option<(i32, i32, i32, i32)> {
        let main = window.bounds();
        let center = main.center();
        let displays = cx.displays();
        let display = displays
            .iter()
            .find(|display| display.bounds().contains(&center))
            .cloned()
            .or_else(|| displays.first().cloned())?;
        let work = display.visible_bounds();
        let gap = px(8.0);
        let (width, height) = preview::window_size();
        let width = px(width)
            .min(work.size.width - gap * 2.0)
            .max(px(160.0));
        let height = px(height)
            .min(work.size.height - gap * 2.0)
            .max(px(120.0));
        let work_left = work.origin.x;
        let work_top = work.origin.y;
        let work_right = work_left + work.size.width;
        let work_bottom = work_top + work.size.height;

        let mut x = main.origin.x + main.size.width + gap;
        if x + width > work_right {
            x = main.origin.x - gap - width;
        }
        let max_x = (work_right - width - gap).max(work_left + gap);
        let x = x.max(work_left + gap).min(max_x);
        let max_y = (work_bottom - height - gap).max(work_top + gap);
        let y = (main.origin.y + gap)
            .max(work_top + gap)
            .min(max_y);

        let scale = window.scale_factor();
        Some((
            (f32::from(x) * scale).round() as i32,
            (f32::from(y) * scale).round() as i32,
            (f32::from(width) * scale).round() as i32,
            (f32::from(height) * scale).round() as i32,
        ))
    }

    fn sync_settings(&mut self, cx: &App) {
        let current = settings::current(cx);
        if current != self.settings {
            let layout_changed = current.max_text_lines != self.settings.max_text_lines;
            let source_changed = current.data_source != self.settings.data_source;
            self.settings = current;
            if source_changed {
                self.data_source = self.settings.data_source.clone();
                self.source = source::ContentSource::resolve(&self.data_source, &self.data_path);
                self.load_generation = self.load_generation.wrapping_add(1);
                self.loading = false;
                self.pending_reload = true;
                self.notice = Some(
                    format!(
                        "数据源已切换为 {}，正在重新加载…",
                        self.source.kind().label()
                    )
                    .into(),
                );
            }
            if layout_changed {
                self.refresh_filter();
            }
        }
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

    fn render_sidebar(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let mut items = Vec::with_capacity(self.tags.len() + 1);
        items.push(self.render_tag_item(None, "全部", cx));
        for tag in &self.tags {
            items.push(self.render_tag_item(Some(tag.clone()), tag, cx));
        }

        let window_width = f32::from(window.bounds().size.width);
        let width = self
            .settings
            .tag_sidebar_width
            .min(window_width - MIN_MEMO_LIST_WIDTH)
            .max(settings::TAG_SIDEBAR_WIDTH_MIN);
        let theme = cx.theme();
        v_flex()
            .w(px(width))
            .h_full()
            .flex_shrink_0()
            .border_r_1()
            .border_color(theme.border)
            .bg(theme.title_bar)
            .child(
                v_flex()
                    .id("tag-list-scroll")
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .gap_1()
                    .p_2()
                    .overflow_y_scroll()
                    .children(items),
            )
            .into_any_element()
    }

    fn render_tag_item(
        &self,
        tag: Option<String>,
        label: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self.active_tag == tag;
        let (selected_bg, selected_fg, sidebar_fg, hover_bg, hover_fg) = {
            let theme = cx.theme();
            (
                theme.sidebar_primary,
                theme.sidebar_primary_foreground,
                theme.sidebar_foreground,
                theme.sidebar_accent,
                theme.sidebar_accent_foreground,
            )
        };
        let id = match &tag {
            Some(tag) => SharedString::from(format!("tag-item-{tag}")),
            None => SharedString::from("tag-item-all"),
        };
        let label: SharedString = match &tag {
            Some(tag) => format!("#{tag}").into(),
            None => label.to_owned().into(),
        };

        h_flex()
            .id(id)
            .w_full()
            .h(px(TAG_ROW_HEIGHT))
            .flex_shrink_0()
            .items_center()
            .px_2()
            .rounded(px(6.0))
            .text_sm()
            .cursor_pointer()
            .when(selected, |this| {
                this.bg(selected_bg).text_color(selected_fg)
            })
            .when(!selected, |this| {
                this.text_color(sidebar_fg).hover(move |this| {
                    this.bg(hover_bg).text_color(hover_fg)
                })
            })
            .on_click(cx.listener(move |this, _, _window, cx| {
                this.select_tag(tag.clone(), cx);
            }))
            .child(div().w_full().min_w_0().truncate().child(label))
            .into_any_element()
    }

    fn render_list(&self, cx: &mut Context<Self>) -> AnyElement {
        // 右侧列表在横排容器里：flex_1 只分配宽度，高度必须显式撑满，否则会塌缩。
        if self.filtered.is_empty() {
            let content = if self.memos.is_empty() {
                let hint = if self.source.is_remote() {
                    "在远端 memos.yaml 中添加记录，再点「重新加载」"
                } else {
                    "在内容源 memos.yaml 中添加记录，再点「重新加载」"
                };
                render_empty_state(IconName::FileText, "还没有备忘录", hint, cx)
            } else {
                render_empty_state(
                    IconName::Search,
                    "没有匹配的备忘录",
                    "换个关键词或标签试试",
                    cx,
                )
            };
            return div()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .w_full()
                .h_full()
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
        .min_w_0()
        .min_h_0()
        .w_full();

        let scroll_height: f32 = self
            .filtered
            .iter()
            .filter_map(|index| self.memos.get(*index))
            .map(|memo| Self::row_height(memo, &self.settings))
            .sum();
        let scroll_size = size(px(0.0), px(scroll_height));

        div()
            .id("memo-list-scroll")
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .w_full()
            .h_full()
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
        let max_lines = self.settings.max_text_lines;
        let line_count = memo::display_line_count(text.as_ref(), max_lines);
        let pinned = memo.is_pinned;
        let row_height = Self::row_height(memo, &self.settings);
        let card_height = row_height - ROW_GAP;
        let card_radius = self.settings.card_radius;
        let click_action = self.settings.click_action;

        let scrolling = self.smooth_scroll.is_active();
        let hovered = !scrolling && self.hovered == Some(index);
        let card_bg = cx.theme().background;
        let primary = cx.theme().primary;
        let pin_hover_bg = cx.theme().muted;
        let muted_foreground = cx.theme().muted_foreground;

        let card = v_flex()
            .id(("memo-card", index))
            .relative()
            .w_full()
            .h(px(card_height))
            .gap(px(ROW_CONTENT_GAP))
            .overflow_hidden()
            .rounded(px(card_radius))
            .bg(card_bg)
            .px(px(ROW_PADDING_X))
            .py(px(ROW_PADDING_Y))
            .when(hovered, |this| this.shadow_sm())
            .on_hover(cx.listener(move |this, is_hovered: &bool, window, cx| {
                let next = if *is_hovered {
                    Some(index)
                } else if this.hovered == Some(index) {
                    None
                } else {
                    return;
                };
                if this.hovered != next {
                    this.hovered = next;
                    if this.smooth_scroll.is_active() {
                        this.cancel_preview(cx);
                    } else if next.is_some() {
                        this.schedule_preview(index, window, cx);
                    } else {
                        this.cancel_preview(cx);
                    }
                    if !this.smooth_scroll.is_active() {
                        cx.notify();
                    }
                }
            }))
            .cursor_pointer()
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                let should_paste = match click_action {
                    settings::ClickAction::DoubleClick => event.click_count() == 2,
                    settings::ClickAction::SingleClick => event.click_count() >= 1,
                };
                if should_paste {
                    this.paste_index(index, window, cx);
                }
            }))
            .child(
                div()
                    .w_full()
                    .flex_shrink_0()
                    .h(px(line_count as f32 * ROW_LINE_HEIGHT))
                    .overflow_hidden()
                    .pr(px(ROW_TEXT_RIGHT))
                    .text_sm()
                    .line_height(px(ROW_LINE_HEIGHT))
                    .line_clamp(line_count)
                    .child(text),
            )
            .children(tags.map(|tags| {
                div()
                    .w_full()
                    .flex_shrink_0()
                    .h(px(ROW_TAG_HEIGHT))
                    .truncate()
                    .text_xs()
                    .line_height(px(ROW_TAG_HEIGHT))
                    .text_color(muted_foreground)
                    .child(tags)
            }))
            .children((pinned || hovered).then(|| {
                div()
                    .id(("memo-pin", index))
                    .absolute()
                    .top(px(PIN_INSET))
                    .right(px(PIN_INSET))
                    .w(px(PIN_SIZE))
                    .h(px(PIN_SIZE))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .when(hovered, |this| {
                        this.hover(move |this| this.bg(pin_hover_bg))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _window, cx| {
                                cx.stop_propagation();
                                this.toggle_pin(index, cx);
                            }))
                    })
                    .child(
                        Icon::new(if pinned {
                            IconName::StarFill
                        } else {
                            IconName::Star
                        })
                        .small()
                        .text_color(primary),
                    )
            }));

        v_flex()
            .id(("memo-row", index))
            .w_full()
            .h(px(row_height))
            .px(px(ROW_MARGIN_X))
            .pb(px(ROW_GAP))
            .child(card)
            .into_any_element()
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted_foreground = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let danger = cx.theme().danger;
        let hotkey = hotkey::state(cx);
        let click_hint = match self.settings.click_action {
            settings::ClickAction::DoubleClick => "双击粘贴",
            settings::ClickAction::SingleClick => "单击粘贴",
        };

        let hint: String = match (&hotkey.label, &hotkey.error) {
            (Some(label), _) => {
                format!("{click_hint} · {label} 显示/隐藏 · 关闭窗口后仍在后台运行")
            }
            (None, Some(error)) => format!("{click_hint} · {error}"),
            (None, None) => click_hint.to_owned(),
        };
        let hint: SharedString = if self.loading {
            format!("{hint} · 正在加载数据源…").into()
        } else {
            hint.into()
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
                            .tooltip("打开本地状态文件")
                            .on_click(cx.listener(Self::open_data_file)),
                    )
                    .child(
                        Button::new("open-settings")
                            .ghost()
                            .xsmall()
                            .icon(IconName::Settings)
                            .tooltip("设置")
                            .on_click(cx.listener(|this, _, _, cx| this.open_settings(cx))),
                    )
                    .child(
                        Button::new("quit-app")
                            .ghost()
                            .xsmall()
                            .label("退出")
                            .on_click(cx.listener(Self::quit)),
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
        self.cancel_preview(cx);
        if !self.settings.smooth_scroll {
            self.smooth_scroll.reset();
            let mut delta = event.delta.pixel_delta(window.line_height());
            if event.modifiers.shift && delta.y == px(0.0) {
                delta.y = delta.x;
            }
            if delta.y == px(0.0) {
                return;
            }
            let current = self.list_scroll.as_ref().offset();
            let max_offset = self.list_scroll.as_ref().max_offset().y;
            let offset = (current.y + delta.y).clamp(-max_offset, px(0.0));
            self.list_scroll
                .as_ref()
                .set_offset(point(current.x, offset));
            self.hovered = None;
            cx.stop_propagation();
            cx.notify();
            return;
        }
        let was_scrolling = self.smooth_scroll.is_active();
        if self
            .smooth_scroll
            .handle_wheel(&self.list_scroll, event, window)
        {
            cx.stop_propagation();
            cx.notify();
        } else if was_scrolling && !self.smooth_scroll.is_active() {
            cx.notify();
        }
    }

    fn reload_clicked(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.reload(cx);
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        self.sync_settings(cx);
        self.start_load(cx);
        cx.notify();
    }

    fn start_load(&mut self, cx: &mut Context<Self>) {
        self.pending_reload = false;
        self.load_generation = self.load_generation.wrapping_add(1);
        let generation = self.load_generation;
        self.loading = true;
        if self.source.is_remote() {
            self.notice = Some("正在加载远端数据源…".into());
        }

        let content_source = self.source.clone();
        let data_source = self.data_source.clone();
        let data_path = self.data_path.clone();
        let (sender, receiver) =
            async_channel::unbounded::<Result<Vec<memo::Memo>, String>>();
        std::thread::spawn(move || {
            let result = source::load_merged(&content_source, &data_source, &data_path)
                .map_err(|error| error.to_string());
            let _ = sender.try_send(result);
        });

        let weak = cx.entity().downgrade();
        cx.spawn(async move |_this, cx| {
            let Ok(result) = receiver.recv().await else {
                return;
            };
            let _ = weak.update(cx, |this, cx| this.finish_load(generation, result, cx));
        })
        .detach();
    }

    fn finish_load(
        &mut self,
        generation: u64,
        result: Result<Vec<memo::Memo>, String>,
        cx: &mut Context<Self>,
    ) {
        if generation != self.load_generation {
            return;
        }
        self.loading = false;

        match result {
            Ok(mut memos) => {
                // 后台读取期间用户可能刚粘贴或置顶，保留界面上的最新状态。
                for memo in &mut memos {
                    if let Some(current) = self
                        .memos
                        .iter()
                        .find(|current| current.text == memo.text)
                    {
                        memo.usage_count = current.usage_count;
                        memo.is_pinned = current.is_pinned;
                    }
                }
                memo::sort_for_display(&mut memos);

                let save_error = memo::save(&self.data_path, &memos).err();
                if let Some(error) = &save_error {
                    logging::write(format!("重新加载后写入本地状态失败：{error}"));
                }

                self.memos = memos;
                self.tags = memo::sorted_tags(&self.memos);
                let active_tag = self.active_tag.take().and_then(|active| {
                    self.tags
                        .iter()
                        .find(|tag| tag.to_lowercase() == active.to_lowercase())
                        .cloned()
                });
                self.active_tag = active_tag;
                self.notice = save_error.map(|error| {
                    SharedString::from(format!("已重新加载，但写入本地状态失败：{error}"))
                });
                self.refresh_filter();
            }
            Err(error) => {
                logging::write(format!("重新加载备忘录失败：{error}"));
                self.notice = Some(format!("加载数据源失败：{error}").into());
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

    fn open_settings(&mut self, cx: &mut Context<Self>) {
        self.suppress_blur_until = Some(Instant::now() + Duration::from_millis(800));
        if let Err(error) =
            settings_window::open(cx, self.source.clone(), self.data_path.clone())
        {
            self.suppress_blur_until = None;
            self.notice = Some(format!("无法打开设置：{error}").into());
            cx.notify();
        }
    }

    fn remember_window_size(&mut self, window: &Window) {
        let size = window.bounds().size;
        let width = f32::from(size.width);
        let height = f32::from(size.height);
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return;
        }
        if (width - self.settings.window_width).abs() < 1.0
            && (height - self.settings.window_height).abs() < 1.0
        {
            self.pending_window_size = None;
            return;
        }
        self.pending_window_size = Some((width, height));
    }

    fn on_window_resized(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.remember_window_size(window);
        if self.pending_window_size.is_none() {
            return;
        }
        self.window_size_generation = self.window_size_generation.wrapping_add(1);
        let generation = self.window_size_generation;
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(500))
                .await;
            let _ = weak.update(cx, |this, cx| {
                if this.window_size_generation == generation {
                    this.commit_window_size(cx);
                }
            });
        })
        .detach();
    }

    fn commit_window_size(&mut self, cx: &mut Context<Self>) {
        let Some((width, height)) = self.pending_window_size.take() else {
            return;
        };
        self.window_size_generation = self.window_size_generation.wrapping_add(1);
        if (width - self.settings.window_width).abs() < 1.0
            && (height - self.settings.window_height).abs() < 1.0
        {
            return;
        }
        let mut next = settings::current(cx);
        next.window_width = width;
        next.window_height = height;
        let next = next.normalized();
        match settings::commit(cx, next.clone()) {
            Ok(()) => self.settings = next,
            Err(error) => logging::write(format!("保存窗口尺寸失败：{error}")),
        }
    }

    fn quit(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.remember_window_size(window);
        self.commit_window_size(cx);
        cx.quit();
    }

    fn toggle_pin(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(memo) = self.memos.get(index) else {
            return;
        };
        let pinned = !memo.is_pinned;

        let mut next = self.memos.clone();
        next[index].is_pinned = pinned;
        memo::sort_for_display(&mut next);

        match memo::save(&self.data_path, &next) {
            Ok(()) => {
                self.memos = next;
                self.notice = None;
                self.refresh_filter();
                if pinned {
                    self.scroll_to_top();
                }
            }
            Err(error) => {
                logging::write(format!("保存置顶状态失败：{error}"));
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
        self.cancel_preview(cx);
        let track_usage = self.settings.track_usage;
        let paste_action = self.settings.paste_action;
        let clear_search = self.settings.clear_search_after_paste;

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

        if track_usage
            && let Err(error) = self.record_usage(index)
        {
            self.notice = Some(format!("记录使用次数失败：{error}").into());
            cx.notify();
            return;
        }

        self.notice = None;

        match paste_action {
            settings::PasteAction::AutoPaste => {
                if !paste::hide_then_paste(window) {
                    self.notice =
                        Some("已复制到剪贴板；未找到可粘贴的窗口，请手动粘贴".into());
                    cx.notify();
                    return;
                }
            }
            settings::PasteAction::CopyOnly => {
                paste::hide_window(window);
            }
        }

        if clear_search {
            self.clear_search(window, cx);
        }
    }

    fn clear_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.filter.clear();
        self.search
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.refresh_filter();
    }

    fn record_usage(&mut self, index: usize) -> Result<(), memo::MemoError> {
        if self.memos.get(index).is_none() {
            return Ok(());
        }

        let mut next = self.memos.clone();
        next[index].usage_count = next[index].usage_count.saturating_add(1);
        memo::sort_for_display(&mut next);
        memo::save(&self.data_path, &next).inspect_err(|error| {
            logging::write(format!("保存使用次数失败：{error}"));
        })?;

        self.memos = next;
        self.refresh_filter();
        Ok(())
    }

    fn focus_search(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.search.focus_handle(cx).focus(window, cx);
    }

    fn select_tag(&mut self, tag: Option<String>, cx: &mut Context<Self>) {
        if self.active_tag != tag {
            self.active_tag = tag;
            self.cancel_preview(cx);
            self.refresh_filter();
        }
        self.scroll_to_top();
        cx.notify();
    }

    /// 重新计算过滤结果与行高缓存；备忘录或过滤词变化后必须调用。
    fn refresh_filter(&mut self) {
        self.smooth_scroll.reset();
        self.hovered = None;
        let filtered = memo::matching_indices_with_tag(
            &self.memos,
            &self.filter,
            self.active_tag.as_deref(),
        );
        self.row_sizes = Rc::new(Self::compute_row_sizes(
            &self.memos,
            filtered.as_slice(),
            &self.settings,
        ));
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
                self.remember_window_size(window);
                self.commit_window_size(cx);
                paste::hide_window(window);
            }
            cx.stop_propagation();
        } else if modifiers.control && !modifiers.alt && !modifiers.shift && key == "f" {
            self.focus_search(window, cx);
            cx.stop_propagation();
        } else if modifiers.control && !modifiers.alt && !modifiers.shift && key == "," {
            self.open_settings(cx);
            cx.stop_propagation();
        }
    }

    fn compute_row_sizes(
        memos: &[memo::Memo],
        filtered: &[usize],
        settings: &settings::Settings,
    ) -> Vec<Size<Pixels>> {
        filtered
            .iter()
            .filter_map(|index| memos.get(*index))
            .map(|memo| size(px(0.0), px(Self::row_height(memo, settings))))
            .collect()
    }

    fn row_height(memo: &memo::Memo, settings: &settings::Settings) -> f32 {
        let lines = memo::display_line_count(&memo.text, settings.max_text_lines) as f32;
        let tags = if memo.tags.is_empty() {
            0.0
        } else {
            ROW_CONTENT_GAP + ROW_TAG_HEIGHT
        };
        ROW_PADDING_Y * 2.0 + lines * ROW_LINE_HEIGHT + tags + ROW_GAP
    }
}

impl Render for MemoApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(hwnd) = paste::window_hwnd(window) {
            paste::set_main_window(hwnd);
        }
        self.sync_settings(cx);
        if self.pending_reload {
            let weak = cx.entity().downgrade();
            cx.defer(move |cx| {
                let _ = weak.update(cx, |this, cx| {
                    if this.pending_reload {
                        this.start_load(cx);
                        cx.notify();
                    }
                });
            });
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
        let sidebar = self.render_sidebar(window, cx);
        let list = self.render_list(cx);
        let footer = self.render_footer(cx);

        v_flex()
            .id("memo-app")
            .size_full()
            .bg(cx.theme().muted)
            .text_color(cx.theme().foreground)
            .child(toolbar)
            .child(
                h_flex()
                    .w_full()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .child(sidebar)
                    .child(list),
            )
            .child(footer)
            .on_key_down(cx.listener(Self::on_key_down))
    }
}


fn save_window_size(window: &Window, cx: &mut App) {
    let size = window.bounds().size;
    let width = f32::from(size.width);
    let height = f32::from(size.height);
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return;
    }
    let mut next = settings::current(cx);
    if (width - next.window_width).abs() < 1.0
        && (height - next.window_height).abs() < 1.0
    {
        return;
    }
    next.window_width = width;
    next.window_height = height;
    if let Err(error) = settings::commit(cx, next) {
        logging::write(format!("保存窗口尺寸失败：{error}"));
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
