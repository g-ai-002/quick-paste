use std::path::PathBuf;

use gpui_kit::{
    AnyElement, AppContext as _, ClickEvent, ClipboardItem, Context, Entity, Focusable as _,
    InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, Render, SharedString,
    Styled as _, Subscription, Window, div,
    component::{
        ActiveTheme as _, Icon, IconName, Sizable as _, WindowExt as _,
        button::{Button, ButtonVariant, ButtonVariants},
        h_flex,
        input::{Input, InputEvent, InputState, Textarea, TextareaState},
        v_flex,
    },
    prelude::FluentBuilder as _,
};

use crate::{app::HotkeyStatus, memo, paste};

pub struct MemoApp {
    data_path: PathBuf,
    memos: Vec<String>,
    search: Entity<InputState>,
    filter: String,
    selected: Option<usize>,
    editor: Option<MemoEditor>,
    notice: Option<SharedString>,
    hotkey: HotkeyStatus,
    _subscriptions: Vec<Subscription>,
}

struct MemoEditor {
    index: Option<usize>,
    input: Entity<TextareaState>,
    error: Option<SharedString>,
}

impl MemoApp {
    pub fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
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
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => {
                    this.filter = state.read(cx).value().to_string();
                    if this.editor.is_none()
                        && let Some(index) = this.filtered_indices().first().copied()
                    {
                        this.paste_index(index, window, cx);
                    }
                }
                _ => {}
            },
        );

        let mut notice = None;
        let memos = match memo::load_or_create(&data_path) {
            Ok(memos) => memos,
            Err(error) => {
                notice = Some(format!("无法读取备忘录：{error}").into());
                Vec::new()
            }
        };

        window.on_window_should_close(cx, |window, _cx| {
            paste::hide_window(window);
            false
        });

        let app = Self {
            data_path,
            memos,
            search,
            filter: String::new(),
            selected: None,
            editor: None,
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
            .gap_2()
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
            .child(
                Button::new("new-memo")
                    .primary()
                    .small()
                    .icon(IconName::Plus)
                    .label("新建")
                    .on_click(cx.listener(Self::new_memo)),
            )
            .into_any_element()
    }

    fn render_editor(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(editor) = self.editor.as_ref() else {
            return div().into_any_element();
        };

        let danger = cx.theme().danger;
        let muted = cx.theme().muted;
        let border = cx.theme().border;

        v_flex()
            .w_full()
            .gap_2()
            .px_3()
            .py_3()
            .bg(muted)
            .border_b_1()
            .border_color(border)
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .child(if editor.index.is_some() {
                        "编辑备忘录"
                    } else {
                        "新建备忘录"
                    }),
            )
            .child(Textarea::new(&editor.input))
            .children(
                editor
                    .error
                    .clone()
                    .map(|error| div().text_xs().text_color(danger).child(error)),
            )
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("cancel-editor")
                            .small()
                            .label("取消")
                            .on_click(cx.listener(Self::cancel_editor)),
                    )
                    .child(
                        Button::new("save-editor")
                            .small()
                            .primary()
                            .label("保存")
                            .on_click(cx.listener(Self::save_clicked)),
                    ),
            )
            .into_any_element()
    }

    fn render_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let indices = self.filtered_indices();
        let content = if self.memos.is_empty() {
            render_empty_state(
                IconName::FileText,
                "还没有备忘录",
                "点击「新建」添加一条纯文本，双击条目即可粘贴",
                cx,
            )
        } else if indices.is_empty() {
            render_empty_state(
                IconName::Search,
                "没有匹配的备忘录",
                "换个关键词试试",
                cx,
            )
        } else {
            v_flex()
                .w_full()
                .p_2()
                .gap_1()
                .children(
                    indices
                        .into_iter()
                        .map(|index| self.render_row(index, cx)),
                )
                .into_any_element()
        };

        div()
            .id("memo-list")
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_y_scroll()
            .child(content)
            .into_any_element()
    }

    fn render_row(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(text) = self.memos.get(index) else {
            return div().into_any_element();
        };

        let text: SharedString = text.clone().into();
        let selected = self.selected == Some(index);
        let selected_bg = cx.theme().muted;
        let hover_bg = cx.theme().muted;
        let muted_foreground = cx.theme().muted_foreground;
        let danger = cx.theme().danger;

        h_flex()
            .id(("memo-row", index))
            .w_full()
            .items_start()
            .gap_3()
            .rounded_md()
            .px_3()
            .py_3()
            .when(selected, |this| this.bg(selected_bg))
            .hover(move |this| this.bg(hover_bg))
            .cursor_pointer()
            .on_click(cx.listener(move |this, event, window, cx| {
                if event.click_count() == 2 {
                    this.paste_index(index, window, cx);
                } else {
                    this.selected = Some(index);
                    cx.notify();
                }
            }))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(div().line_clamp(2).text_sm().child(text))
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted_foreground)
                            .child("双击粘贴"),
                    ),
            )
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(
                        Button::new(("memo-edit", index))
                            .ghost()
                            .xsmall()
                            .label("编辑")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                let text = this.memos.get(index).cloned().unwrap_or_default();
                                this.open_editor(Some(index), text, window, cx);
                            })),
                    )
                    .child(
                        Button::new(("memo-delete", index))
                            .ghost()
                            .xsmall()
                            .label("删除")
                            .text_color(danger)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.confirm_delete(index, window, cx);
                            })),
                    ),
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

    fn new_memo(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.open_editor(None, String::new(), window, cx);
    }

    fn open_editor(
        &mut self,
        index: Option<usize>,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .default_value(text)
                .auto_grow(3, 8)
                .placeholder("输入要保存的纯文本…")
        });
        input.focus_handle(cx).focus(window, cx);
        self.editor = Some(MemoEditor {
            index,
            input,
            error: None,
        });
        cx.notify();
    }

    fn cancel_editor(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.close_editor(window, cx);
    }

    fn close_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor = None;
        self.focus_search(window, cx);
        cx.notify();
    }

    fn save_clicked(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_editor(window, cx);
    }

    fn commit_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (index, raw) = match self.editor.as_ref() {
            Some(editor) => (editor.index, editor.input.read(cx).value().to_string()),
            None => return,
        };
        let text = raw.trim_end().to_string();

        if text.trim().is_empty() {
            if let Some(editor) = self.editor.as_mut() {
                editor.error = Some("内容不能为空".into());
            }
            cx.notify();
            return;
        }

        let mut next = self.memos.clone();
        let selected = match index {
            Some(index) if index < next.len() => {
                next[index] = text;
                index
            }
            Some(_) => {
                self.editor = None;
                self.focus_search(window, cx);
                cx.notify();
                return;
            }
            None => {
                next.insert(0, text);
                0
            }
        };

        match memo::save(&self.data_path, &next) {
            Ok(()) => {
                self.memos = next;
                self.selected = Some(selected);
                self.editor = None;
                self.notice = None;
                self.focus_search(window, cx);
            }
            Err(error) => {
                let message: SharedString = format!("保存失败：{error}").into();
                self.notice = Some(message.clone());
                if let Some(editor) = self.editor.as_mut() {
                    editor.error = Some(message);
                }
            }
        }
        cx.notify();
    }

    fn reload_clicked(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.reload(cx);
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        match memo::load_or_create(&self.data_path) {
            Ok(memos) => {
                self.memos = memos;
                self.notice = None;
                if self.selected.is_some_and(|index| index >= self.memos.len()) {
                    self.selected = None;
                }
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

    fn confirm_delete(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(text) = self.memos.get(index) else {
            return;
        };
        let preview = preview_text(text, 80);
        let entity = cx.entity();

        window.open_alert_dialog(cx, move |alert, _, _| {
            let entity = entity.clone();
            alert
                .title("删除这条备忘录？")
                .description(preview.clone())
                .confirm()
                .ok_text("删除")
                .ok_variant(ButtonVariant::Danger)
                .on_ok(move |_, _, cx| {
                    entity.update(cx, |this, cx| this.delete_index(index, cx));
                    true
                })
        });
    }

    fn delete_index(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.memos.len() {
            return;
        }

        let mut next = self.memos.clone();
        next.remove(index);

        match memo::save(&self.data_path, &next) {
            Ok(()) => {
                self.memos = next;
                self.notice = None;
                if self.selected == Some(index) {
                    self.selected = None;
                } else if self.selected.is_some_and(|selected| selected > index) {
                    self.selected = self.selected.map(|selected| selected - 1);
                }

                let mut close_editor = false;
                if let Some(editor) = self.editor.as_mut() {
                    match editor.index {
                        Some(editing) if editing == index => close_editor = true,
                        Some(editing) if editing > index => {
                            editor.index = Some(editing - 1);
                        }
                        _ => {}
                    }
                }
                if close_editor {
                    self.editor = None;
                }
            }
            Err(error) => {
                self.notice = Some(format!("删除失败：{error}").into());
            }
        }
        cx.notify();
    }

    fn paste_index(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self.memos.get(index).cloned() else {
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

        self.notice = None;

        if !paste::hide_then_paste(window) {
            self.notice =
                Some("已复制到剪贴板；未找到可粘贴的窗口，请手动粘贴".into());
            cx.notify();
        }
    }

    fn focus_search(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.search.focus_handle(cx).focus(window, cx);
    }

    fn filtered_indices(&self) -> Vec<usize> {
        memo::matching_indices(&self.memos, &self.filter)
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            return;
        }

        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;

        if key == "escape" {
            if self.editor.is_some() {
                self.close_editor(window, cx);
            } else if !self.filter.is_empty() {
                self.filter.clear();
                self.search
                    .update(cx, |state, cx| state.set_value("", window, cx));
                cx.notify();
            } else {
                paste::hide_window(window);
            }
            cx.stop_propagation();
        } else if modifiers.control && !modifiers.alt && !modifiers.shift && key == "n" {
            self.open_editor(None, String::new(), window, cx);
            cx.stop_propagation();
        } else if modifiers.control && !modifiers.alt && !modifiers.shift && key == "f" {
            self.focus_search(window, cx);
            cx.stop_propagation();
        } else if modifiers.control
            && !modifiers.alt
            && !modifiers.shift
            && key == "enter"
            && self.editor.is_some()
        {
            self.commit_editor(window, cx);
            cx.stop_propagation();
        }
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
        }

        let toolbar = self.render_toolbar(cx);
        let editor = self
            .editor
            .as_ref()
            .map(|_| self.render_editor(cx));
        let list = self.render_list(cx);
        let footer = self.render_footer(cx);

        v_flex()
            .id("memo-app")
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(toolbar)
            .children(editor)
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

fn preview_text(text: &str, limit: usize) -> String {
    let mut chars = text.chars();
    let mut preview: String = chars.by_ref().take(limit).collect();
    if chars.next().is_some() {
        preview.push('…');
    }
    preview
}
