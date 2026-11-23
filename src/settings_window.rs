//! 设置窗口：七个设置页、设置项搜索与即时保存。

use std::{
    path::PathBuf,
    process::Command,
};

use gpui_kit::{
    AnyElement, AnyWindowHandle, App, AppContext as _, BorrowAppContext as _, ClickEvent, Context,
    Entity, FocusHandle, Focusable, Global, InteractiveElement as _, IntoElement, KeyDownEvent,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _,
    Subscription, TitlebarOptions, Window, WindowBounds, WindowOptions, div, px, size,
    component::{
        ActiveTheme as _, Icon, IconName, Sizable as _, StyledExt as _, WindowExt as _,
        button::{Button, ButtonVariants},
        h_flex,
        input::{Input, InputEvent, InputState},
        switch::Switch,
        v_flex,
    },
    prelude::FluentBuilder as _,
};

use crate::{
    hotkey::{self, HotkeyChoice},
    logging, memo,
    settings::{self, ClickAction, PasteAction, Settings, ThemeChoice},
    source::{ContentSource, DataSourceConfig, SourceKind},
};

const SETTINGS_WINDOW_WIDTH: f32 = 800.0;
const SETTINGS_WINDOW_HEIGHT: f32 = 600.0;
const SETTINGS_WINDOW_MIN_WIDTH: f32 = 700.0;
const SETTINGS_WINDOW_MIN_HEIGHT: f32 = 500.0;
const SIDEBAR_WIDTH: f32 = 200.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsPage {
    General,
    DataSource,
    Preview,
    Appearance,
    Shortcuts,
    QuickPaste,
    About,
}

impl SettingsPage {
    const ALL: [Self; 7] = [
        Self::General,
        Self::DataSource,
        Self::Preview,
        Self::Appearance,
        Self::Shortcuts,
        Self::QuickPaste,
        Self::About,
    ];

    const fn id(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::DataSource => "data-source",
            Self::Preview => "preview",
            Self::Appearance => "appearance",
            Self::Shortcuts => "shortcuts",
            Self::QuickPaste => "quick-paste",
            Self::About => "about",
        }
    }

    const fn title(self) -> &'static str {
        match self {
            Self::General => "常规",
            Self::DataSource => "数据源",
            Self::Preview => "预览",
            Self::Appearance => "外观",
            Self::Shortcuts => "快捷键",
            Self::QuickPaste => "快速粘贴",
            Self::About => "关于",
        }
    }

    const fn description(self) -> &'static str {
        match self {
            Self::General => "控制窗口如何出现、何时隐藏。",
            Self::DataSource => "选择本地文件或远端仓库中的 memos.yaml。",
            Self::Preview => "决定列表里能看到多少内容。",
            Self::Appearance => "主题、主窗口标签栏宽度、圆角与滚动手感。",
            Self::Shortcuts => "全局唤出键与窗口内操作。",
            Self::QuickPaste => "触发、粘贴与排序。",
            Self::About => "版本、数据文件与项目信息。",
        }
    }

    fn icon_name(self) -> IconName {
        match self {
            Self::General => IconName::Settings,
            Self::DataSource => IconName::FileText,
            Self::Preview => IconName::Eye,
            Self::Appearance => IconName::Palette,
            Self::Shortcuts => IconName::SquareTerminal,
            Self::QuickPaste => IconName::Copy,
            Self::About => IconName::Info,
        }
    }

    const fn is_resettable(self) -> bool {
        !matches!(self, Self::DataSource | Self::About)
    }
}

#[derive(Debug, Clone, Copy)]
struct SettingMeta {
    id: &'static str,
    page: SettingsPage,
    title: &'static str,
    description: &'static str,
    keywords: &'static [&'static str],
}

const SETTING_META: &[SettingMeta] = &[
    SettingMeta {
        id: "data_source_kind",
        page: SettingsPage::DataSource,
        title: "数据源类型",
        description: "本地文件仅本机可用；Gitea / Gitee / GitHub / GitLab 可在多台设备间共享同一份 memos.yaml。",
        keywords: &["数据源", "本地", "Gitea", "Gitee", "GitHub", "GitLab", "仓库", "同步"],
    },
    SettingMeta {
        id: "data_source_local_path",
        page: SettingsPage::DataSource,
        title: "本地 YAML 文件",
        description: "本地文件数据源使用；留空时自动查找当前工作目录和程序目录，也可填写绝对路径。",
        keywords: &["本地", "路径", "YAML", "文件", "memos"],
    },
    SettingMeta {
        id: "data_source_url",
        page: SettingsPage::DataSource,
        title: "远端 YAML 文件 URL",
        description: "远端数据源使用；填写仓库文件页面或 raw 地址，程序会自动转换为原始文件地址。",
        keywords: &["URL", "远端", "raw", "YAML", "文件", "仓库", "GitLab"],
    },
    SettingMeta {
        id: "data_source_token",
        page: SettingsPage::DataSource,
        title: "访问令牌（TOKEN）",
        description: "私有仓库需要；公开仓库可留空。TOKEN 以明文保存在本机 settings.yaml。",
        keywords: &["TOKEN", "令牌", "私有仓库", "认证"],
    },
    SettingMeta {
        id: "data_source_apply",
        page: SettingsPage::DataSource,
        title: "应用数据源",
        description: "保存后主窗口会重新读取；使用次数与置顶状态仍保存在本机。",
        keywords: &["应用", "保存", "重新加载", "缓存"],
    },
    SettingMeta {
        id: "show_on_cursor_display",
        page: SettingsPage::General,
        title: "在鼠标所在屏幕弹出",
        description: "按快捷键唤出窗口时，显示在鼠标指针所在的显示器中央。",
        keywords: &["显示器", "屏幕", "多屏", "鼠标"],
    },
    SettingMeta {
        id: "auto_hide_on_blur",
        page: SettingsPage::General,
        title: "失去焦点时自动隐藏",
        description: "窗口切到后台后立即收起，保持随用随走的工具手感。",
        keywords: &["失焦", "隐藏", "自动关闭"],
    },
    SettingMeta {
        id: "start_hidden",
        page: SettingsPage::General,
        title: "启动时隐藏窗口",
        description: "程序启动后只驻留后台，按快捷键才唤出窗口。",
        keywords: &["启动", "后台", "驻留"],
    },
    SettingMeta {
        id: "hover_preview",
        page: SettingsPage::Preview,
        title: "悬停预览完整内容",
        description: "鼠标停在未完整显示的卡片上时，在卡片右侧弹出完整文本、标签与使用次数。",
        keywords: &["悬停", "预览", "完整"],
    },
    SettingMeta {
        id: "hover_preview_delay_ms",
        page: SettingsPage::Preview,
        title: "悬停停留时间",
        description: "鼠标停留多久后显示完整内容预览；0 秒表示立即显示。",
        keywords: &["延迟", "停留", "时间"],
    },
    SettingMeta {
        id: "max_text_lines",
        page: SettingsPage::Preview,
        title: "卡片最多显示行数",
        description: "列表卡片正文最多展示几行，超出部分用悬停预览查看。",
        keywords: &["行数", "高度", "卡片"],
    },
    SettingMeta {
        id: "theme",
        page: SettingsPage::Appearance,
        title: "主题",
        description: "浅色、深色或跟随系统外观。",
        keywords: &["配色", "深色", "浅色", "系统"],
    },
    SettingMeta {
        id: "card_radius",
        page: SettingsPage::Appearance,
        title: "卡片圆角",
        description: "调整列表卡片的圆角大小。",
        keywords: &["圆角", "外观"],
    },
    SettingMeta {
        id: "tag_sidebar_width",
        page: SettingsPage::Appearance,
        title: "主窗口标签栏宽度",
        description: "主窗口左侧标签栏宽度，可在 80–240 px 之间调整；窗口过窄时会自动收窄。",
        keywords: &["标签", "标签栏", "侧栏", "宽度", "布局"],
    },
    SettingMeta {
        id: "smooth_scroll",
        page: SettingsPage::Appearance,
        title: "平滑滚动",
        description: "滚轮滚动时使用缓动动画。",
        keywords: &["滚动", "动画", "滚轮"],
    },
    SettingMeta {
        id: "hotkey",
        page: SettingsPage::Shortcuts,
        title: "唤出快捷键",
        description: "点击“录制”后按下组合键，修改后立即生效。",
        keywords: &["全局", "热键", "快捷键", "自定义", "录制"],
    },
    SettingMeta {
        id: "fixed_shortcuts",
        page: SettingsPage::Shortcuts,
        title: "主窗口快捷键",
        description: "Ctrl+F 聚焦搜索，Esc 清空搜索或隐藏窗口。",
        keywords: &["搜索", "Esc", "Ctrl+F"],
    },
    SettingMeta {
        id: "click_action",
        page: SettingsPage::QuickPaste,
        title: "粘贴触发方式",
        description: "双击或单击卡片时粘贴。",
        keywords: &["单击", "双击", "点击"],
    },
    SettingMeta {
        id: "paste_action",
        page: SettingsPage::QuickPaste,
        title: "粘贴方式",
        description: "自动发送 Ctrl+V，或只复制到剪贴板。",
        keywords: &["粘贴", "复制", "剪贴板"],
    },
    SettingMeta {
        id: "track_usage",
        page: SettingsPage::QuickPaste,
        title: "统计使用次数",
        description: "粘贴成功后增加使用次数并重新排序。",
        keywords: &["排序", "次数", "统计"],
    },
    SettingMeta {
        id: "clear_search_after_paste",
        page: SettingsPage::QuickPaste,
        title: "粘贴后清空搜索",
        description: "下次唤出窗口时从完整列表开始。",
        keywords: &["搜索", "清空"],
    },
    SettingMeta {
        id: "enter_pastes_first",
        page: SettingsPage::QuickPaste,
        title: "回车粘贴第一条",
        description: "搜索框按 Enter 时粘贴第一条匹配记录。",
        keywords: &["回车", "Enter", "搜索"],
    },
    SettingMeta {
        id: "about_version",
        page: SettingsPage::About,
        title: "版本",
        description: "当前程序版本。",
        keywords: &["版本号", "更新"],
    },
    SettingMeta {
        id: "about_data",
        page: SettingsPage::About,
        title: "数据目录",
        description: "程序维护的本地状态文件，包含使用次数与置顶状态；远端数据源断网时也作为缓存。",
        keywords: &["数据", "目录", "状态"],
    },
    SettingMeta {
        id: "about_source",
        page: SettingsPage::About,
        title: "数据源",
        description: "当前使用的数据源；远端地址不显示 TOKEN，程序只读取其中的 text 和 tags。",
        keywords: &["YAML", "数据源", "memos"],
    },
    SettingMeta {
        id: "about_log",
        page: SettingsPage::About,
        title: "运行日志",
        description: "启动路径、配置与备忘录加载结果；时间为 UTC。",
        keywords: &["日志", "log", "app.log", "排查", "错误"],
    },
    SettingMeta {
        id: "about_hotkey",
        page: SettingsPage::About,
        title: "全局快捷键",
        description: "当前注册的全局唤出快捷键。",
        keywords: &["热键"],
    },
    SettingMeta {
        id: "about_homepage",
        page: SettingsPage::About,
        title: "项目主页",
        description: "GitHub 仓库与问题反馈。",
        keywords: &["GitHub", "主页", "反馈"],
    },
    SettingMeta {
        id: "about_license",
        page: SettingsPage::About,
        title: "许可证",
        description: "MIT 开源协议。",
        keywords: &["MIT", "协议"],
    },
];

fn meta(id: &str) -> &'static SettingMeta {
    SETTING_META
        .iter()
        .find(|meta| meta.id == id)
        .expect("setting metadata must exist")
}

fn matching_meta(query: &str) -> Vec<&'static SettingMeta> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }

    SETTING_META
        .iter()
        .filter(|meta| {
            let haystack = format!(
                "{} {} {} {} {}",
                meta.title,
                meta.description,
                meta.page.title(),
                meta.page.description(),
                meta.keywords.join(" "),
            )
            .to_lowercase();
            haystack.contains(&query)
        })
        .collect()
}

#[derive(Default)]
struct SettingsWindowService {
    window: Option<AnyWindowHandle>,
}

impl Global for SettingsWindowService {}

impl SettingsWindowService {
    fn install(cx: &mut App) {
        cx.set_global(Self::default());
        cx.on_window_closed(|cx, _| {
            cx.defer(|cx| {
                let open_windows = cx.windows();
                cx.update_global::<Self, _>(|service, _| {
                    if let Some(window) = service.window
                        && !open_windows
                            .iter()
                            .any(|handle| handle.window_id() == window.window_id())
                    {
                        service.window = None;
                    }
                });
            });
        })
        .detach();
    }
}

pub fn install(cx: &mut App) {
    SettingsWindowService::install(cx);
}

pub fn open(
    cx: &mut App,
    source: ContentSource,
    data_path: PathBuf,
) -> Result<(), String> {
    let existing = cx
        .try_global::<SettingsWindowService>()
        .and_then(|service| service.window);
    if let Some(window) = existing
        && window
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        cx.activate(true);
        return Ok(());
    }

    let options = WindowOptions {
        window_bounds: Some(WindowBounds::centered(
            size(px(SETTINGS_WINDOW_WIDTH), px(SETTINGS_WINDOW_HEIGHT)),
            cx,
        )),
        titlebar: Some(TitlebarOptions {
            title: Some("设置".into()),
            ..Default::default()
        }),
        window_min_size: Some(size(
            px(SETTINGS_WINDOW_MIN_WIDTH),
            px(SETTINGS_WINDOW_MIN_HEIGHT),
        )),
        focus: true,
        app_id: Some("com.g-ai-002.quick-paste".to_owned()),
        ..Default::default()
    };

    let (window, _) = gpui_kit::open_window(options, cx, move |window, cx| {
        cx.new(|cx| SettingsWindow::new(window, cx, source, data_path))
    })
    .map_err(|error| format!("无法创建设置窗口：{error}"))?;

    cx.set_global(SettingsWindowService {
        window: Some(window),
    });
    Ok(())
}

struct SettingsWindow {
    source: ContentSource,
    data_path: PathBuf,
    settings: Settings,
    data_source: DataSourceConfig,
    active_page: SettingsPage,
    search: Entity<InputState>,
    local_path_input: Entity<InputState>,
    remote_url_input: Entity<InputState>,
    token_input: Entity<InputState>,
    query: String,
    highlight: Option<&'static str>,
    notice: Option<SharedString>,
    recording_hotkey: bool,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl SettingsWindow {
    fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        source: ContentSource,
        data_path: PathBuf,
    ) -> Self {
        let current_settings = settings::current(cx);
        let data_source = current_settings.data_source.clone();
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("搜索设置…"));

        let local_path_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("留空自动查找当前目录 / 程序目录")
        });
        local_path_input.update(cx, |state, cx| {
            state.set_value(data_source.local_path.as_str(), window, cx);
        });
        let remote_url_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("https://…/memos.yaml")
        });
        remote_url_input.update(cx, |state, cx| {
            state.set_value(data_source.url.as_str(), window, cx);
        });
        let token_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("公开仓库可留空")
        });
        token_input.update(cx, |state, cx| {
            state.set_value(data_source.token.as_str(), window, cx);
            state.toggle_masked(window, cx);
        });

        let subscription = cx.subscribe_in(
            &search,
            window,
            |this, state, event, window, cx| match event {
                InputEvent::Change => {
                    this.query = state.read(cx).value().to_string();
                    this.highlight = None;
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => {
                    if let Some(meta) = matching_meta(&this.query).first().copied() {
                        this.select_search_result(meta, window, cx);
                    }
                }
                _ => {}
            },
        );

        let local_path_subscription = cx.subscribe_in(
            &local_path_input,
            window,
            |this, state, event, window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.data_source.local_path = state.read(cx).value().to_string();
                    cx.notify();
                } else if matches!(event, InputEvent::PressEnter { .. }) {
                    this.apply_data_source_now(window, cx);
                }
            },
        );
        let remote_url_subscription = cx.subscribe_in(
            &remote_url_input,
            window,
            |this, state, event, window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.data_source.url = state.read(cx).value().to_string();
                    cx.notify();
                } else if matches!(event, InputEvent::PressEnter { .. }) {
                    this.apply_data_source_now(window, cx);
                }
            },
        );
        let token_subscription = cx.subscribe_in(
            &token_input,
            window,
            |this, state, event, window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.data_source.token = state.read(cx).value().to_string();
                    cx.notify();
                } else if matches!(event, InputEvent::PressEnter { .. }) {
                    this.apply_data_source_now(window, cx);
                }
            },
        );

        let appearance_subscription =
            cx.observe_window_appearance(window, |_, _window, cx| {
                if settings::current(cx).theme == ThemeChoice::System {
                    gpui_kit::component::Theme::sync_system_appearance(None, cx);
                }
            });

        let focus_handle = cx.focus_handle();
        let blur_subscription = cx.on_blur(&focus_handle, window, |this, _window, cx| {
            if this.recording_hotkey {
                this.recording_hotkey = false;
                cx.notify();
            }
        });

        let app = Self {
            source,
            data_path,
            settings: current_settings,
            data_source,
            active_page: SettingsPage::General,
            search,
            local_path_input,
            remote_url_input,
            token_input,
            query: String::new(),
            highlight: None,
            notice: None,
            recording_hotkey: false,
            focus_handle,
            _subscriptions: vec![
                subscription,
                local_path_subscription,
                remote_url_subscription,
                token_subscription,
                appearance_subscription,
                blur_subscription,
            ],
        };
        app.search.focus_handle(cx).focus(window, cx);
        app
    }

    fn select_page(&mut self, page: SettingsPage, window: &mut Window, cx: &mut Context<Self>) {
        self.active_page = page;
        self.highlight = None;
        self.recording_hotkey = false;
        self.clear_search_value(window, cx);
        self.search.focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    fn select_search_result(
        &mut self,
        meta: &SettingMeta,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.active_page = meta.page;
        self.highlight = Some(meta.id);
        self.recording_hotkey = false;
        self.clear_search_value(window, cx);
        self.search.focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    fn clear_search_value(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.query.is_empty() {
            return;
        }
        self.query.clear();
        self.search
            .update(cx, |state, cx| state.set_value("", window, cx));
    }

    fn update_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut Settings),
    ) {
        let previous = self.settings.clone();
        let mut next = previous.clone();
        edit(&mut next);
        let next = next.normalized();

        if next.hotkey != previous.hotkey {
            if let Err(error) = hotkey::set_choice(next.hotkey.clone(), cx) {
                self.notice = Some(error.into());
                cx.notify();
                return;
            }
        }

        if let Err(error) = settings::commit(cx, next.clone()) {
            if next.hotkey != previous.hotkey {
                let _ = hotkey::set_choice(previous.hotkey.clone(), cx);
            }
            self.notice = Some(format!("无法保存设置：{error}").into());
            self.settings = previous;
            cx.notify();
            return;
        }

        if next.theme != previous.theme {
            settings::apply_theme(next.theme, Some(window), cx);
        }

        self.settings = next;
        self.highlight = None;
        self.notice = None;
        cx.notify();
    }

    fn set_data_source_kind(
        &mut self,
        value: SourceKind,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.data_source.kind != value {
            self.data_source.kind = value;
            self.notice = None;
            cx.notify();
        }
    }

    fn apply_data_source(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_data_source_now(window, cx);
    }

    fn apply_data_source_now(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.data_source = self.data_source.clone().normalized();
        if let Some(error) = self.data_source.validation_error() {
            self.notice = Some(format!("数据源配置无效：{error}").into());
            cx.notify();
            return;
        }

        let data_source = self.data_source.clone();
        self.update_settings(window, cx, move |settings| {
            settings.data_source = data_source;
        });

        if self.settings.data_source == self.data_source {
            self.source = ContentSource::resolve(&self.data_source, &self.data_path);
            self.notice = Some("数据源已保存，主窗口正在重新加载。".into());
            self.sync_data_source_inputs(window, cx);
        }
        cx.notify();
    }

    fn revert_data_source(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.data_source = self.settings.data_source.clone();
        self.sync_data_source_inputs(window, cx);
        self.notice = None;
        cx.notify();
    }

    fn sync_data_source_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let data_source = self.data_source.clone();
        self.local_path_input.update(cx, |state, cx| {
            state.set_value(data_source.local_path.as_str(), window, cx);
        });
        self.remote_url_input.update(cx, |state, cx| {
            state.set_value(data_source.url.as_str(), window, cx);
        });
        self.token_input.update(cx, |state, cx| {
            state.set_value(data_source.token.as_str(), window, cx);
        });
    }

    fn set_show_on_cursor_display(
        &mut self,
        value: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_settings(window, cx, |settings| {
            settings.show_on_cursor_display = value;
        });
    }

    fn set_auto_hide_on_blur(
        &mut self,
        value: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_settings(window, cx, |settings| {
            settings.auto_hide_on_blur = value;
        });
    }

    fn set_start_hidden(
        &mut self,
        value: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_settings(window, cx, |settings| {
            settings.start_hidden = value;
        });
    }

    fn set_hover_preview(
        &mut self,
        value: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_settings(window, cx, |settings| {
            settings.hover_preview = value;
        });
    }

    fn set_hover_preview_delay(
        &mut self,
        value: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_settings(window, cx, |settings| {
            settings.hover_preview_delay_ms = value;
        });
    }

    fn set_max_text_lines(
        &mut self,
        value: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_settings(window, cx, |settings| {
            settings.max_text_lines = value;
        });
    }

    fn set_theme(
        &mut self,
        value: ThemeChoice,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_settings(window, cx, |settings| {
            settings.theme = value;
        });
    }

    fn set_card_radius(
        &mut self,
        value: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_settings(window, cx, |settings| {
            settings.card_radius = value;
        });
    }

    fn set_tag_sidebar_width(
        &mut self,
        value: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_settings(window, cx, |settings| {
            settings.tag_sidebar_width = value;
        });
    }

    fn set_smooth_scroll(
        &mut self,
        value: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_settings(window, cx, |settings| {
            settings.smooth_scroll = value;
        });
    }

    fn set_hotkey(
        &mut self,
        value: HotkeyChoice,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if value == self.settings.hotkey {
            // 配置里已经是这个键时也允许再点一次重试注册（例如启动时被占用）。
            match hotkey::set_choice(value, cx) {
                Ok(()) => self.notice = None,
                Err(error) => self.notice = Some(error.into()),
            }
            self.highlight = None;
            cx.notify();
            return;
        }
        self.update_settings(window, cx, |settings| {
            settings.hotkey = value;
        });
    }

    fn toggle_recording_hotkey(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.recording_hotkey = !self.recording_hotkey;
        if self.recording_hotkey {
            self.query.clear();
            self.search
                .update(cx, |state, cx| state.set_value("", window, cx));
            self.highlight = None;
            self.notice = None;
            self.focus_handle.focus(window, cx);
        } else {
            self.search.focus_handle(cx).focus(window, cx);
        }
        cx.notify();
    }

    fn set_click_action(
        &mut self,
        value: ClickAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_settings(window, cx, |settings| {
            settings.click_action = value;
        });
    }

    fn set_paste_action(
        &mut self,
        value: PasteAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_settings(window, cx, |settings| {
            settings.paste_action = value;
        });
    }

    fn set_track_usage(
        &mut self,
        value: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_settings(window, cx, |settings| {
            settings.track_usage = value;
        });
    }

    fn set_clear_search_after_paste(
        &mut self,
        value: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_settings(window, cx, |settings| {
            settings.clear_search_after_paste = value;
        });
    }

    fn set_enter_pastes_first(
        &mut self,
        value: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_settings(window, cx, |settings| {
            settings.enter_pastes_first = value;
        });
    }

    fn reset_page(
        &mut self,
        page: SettingsPage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let defaults = Settings::default();
        self.update_settings(window, cx, move |settings| match page {
            SettingsPage::General => {
                settings.show_on_cursor_display = defaults.show_on_cursor_display;
                settings.start_hidden = defaults.start_hidden;
                settings.auto_hide_on_blur = defaults.auto_hide_on_blur;
            }
            SettingsPage::Preview => {
                settings.hover_preview = defaults.hover_preview;
                settings.hover_preview_delay_ms = defaults.hover_preview_delay_ms;
                settings.max_text_lines = defaults.max_text_lines;
            }
            SettingsPage::Appearance => {
                settings.theme = defaults.theme;
                settings.card_radius = defaults.card_radius;
                settings.tag_sidebar_width = defaults.tag_sidebar_width;
                settings.smooth_scroll = defaults.smooth_scroll;
            }
            SettingsPage::Shortcuts => {
                settings.hotkey = defaults.hotkey.clone();
            }
            SettingsPage::QuickPaste => {
                settings.click_action = defaults.click_action;
                settings.paste_action = defaults.paste_action;
                settings.track_usage = defaults.track_usage;
                settings.clear_search_after_paste = defaults.clear_search_after_paste;
                settings.enter_pastes_first = defaults.enter_pastes_first;
            }
            SettingsPage::DataSource | SettingsPage::About => {}
        });
        self.highlight = None;
        self.recording_hotkey = false;
        self.search.focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    fn reveal_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if let Err(error) = memo::reveal(&path) {
            self.notice = Some(format!("无法打开 {}：{error}", path.display()).into());
            cx.notify();
        }
    }

    fn open_url(&mut self, url: String, cx: &mut Context<Self>) {
        if let Err(error) = Command::new("rundll32.exe")
            .arg("url.dll,FileProtocolHandler")
            .arg(url.as_str())
            .spawn()
        {
            self.notice = Some(format!("无法打开 {url}：{error}").into());
        }
        cx.notify();
    }

    fn open_homepage(&mut self, cx: &mut Context<Self>) {
        self.open_url("https://github.com/g-ai-002/quick-paste".to_owned(), cx);
    }

    fn bool_switch(
        &self,
        id: &'static str,
        checked: bool,
        apply: fn(&mut Self, bool, &mut Window, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        Switch::new(id)
            .checked(checked)
            .on_click(
                cx.listener(move |this, checked: &bool, window, cx| {
                    apply(this, *checked, window, cx)
                }),
            )
            .into_any_element()
    }

    fn segmented<T>(
        &self,
        prefix: &'static str,
        options: &[(T, &'static str)],
        selected: T,
        apply: fn(&mut Self, T, &mut Window, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) -> AnyElement
    where
        T: Copy + PartialEq + 'static,
    {
        h_flex()
            .flex_wrap()
            .gap_1()
            .children(options.iter().enumerate().map(|(index, (value, label))| {
                let value = *value;
                Button::new(SharedString::from(format!("{prefix}-{index}")))
                    .xsmall()
                    .label(*label)
                    .when(value == selected, |button| button.primary())
                    .when(value != selected, |button| button.ghost())
                    .on_click(
                        cx.listener(move |this, _, window, cx| {
                            apply(this, value, window, cx)
                        }),
                    )
            }))
            .into_any_element()
    }

    fn setting_row(
        &self,
        meta: &'static SettingMeta,
        control: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let highlighted = self.highlight == Some(meta.id);

        h_flex()
            .id(SharedString::from(format!("setting-{}", meta.id)))
            .w_full()
            .items_center()
            .gap_6()
            .px_4()
            .py_3()
            .border_b_1()
            .border_color(theme.border)
            .when(highlighted, |this| this.bg(theme.accent))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(div().text_sm().font_medium().child(meta.title))
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(meta.description),
                    ),
            )
            .child(div().flex_shrink_0().child(control))
            .into_any_element()
    }

    fn page_layout(
        &self,
        page: SettingsPage,
        rows: Vec<AnyElement>,
        extra: Vec<AnyElement>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        v_flex()
            .id(SharedString::from(format!("settings-page-{}", page.id())))
            .w_full()
            .gap_4()
            .p_5()
            .child(
                h_flex()
                    .w_full()
                    .items_start()
                    .justify_between()
                    .gap_3()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(div().text_lg().font_semibold().child(page.title()))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(theme.muted_foreground)
                                    .child(page.description()),
                            ),
                    )
                    .children(page.is_resettable().then(|| {
                        Button::new(SharedString::from(format!("reset-{}", page.id())))
                            .ghost()
                            .xsmall()
                            .label("恢复默认")
                            .on_click(
                                cx.listener(move |this, _, window, cx| {
                                    this.reset_page(page, window, cx)
                                }),
                            )
                    })),
            )
            .child(
                v_flex()
                    .w_full()
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.background)
                    .overflow_hidden()
                    .children(rows),
            )
            .children(extra)
            .into_any_element()
    }

    fn info_card(&self, title: &'static str, body: &'static str, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        v_flex()
            .w_full()
            .gap_1()
            .rounded(px(8.0))
            .border_1()
            .border_color(theme.border)
            .bg(theme.title_bar)
            .p_3()
            .child(div().text_sm().font_medium().child(title))
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(body),
            )
            .into_any_element()
    }

    fn render_general(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut rows = Vec::new();

        let control = self.bool_switch(
            "general-show-on-cursor-display",
            self.settings.show_on_cursor_display,
            Self::set_show_on_cursor_display,
            cx,
        );
        rows.push(self.setting_row(meta("show_on_cursor_display"), control, cx));

        let control = self.bool_switch(
            "general-auto-hide-on-blur",
            self.settings.auto_hide_on_blur,
            Self::set_auto_hide_on_blur,
            cx,
        );
        rows.push(self.setting_row(meta("auto_hide_on_blur"), control, cx));

        let control = self.bool_switch(
            "general-start-hidden",
            self.settings.start_hidden,
            Self::set_start_hidden,
            cx,
        );
        rows.push(self.setting_row(meta("start_hidden"), control, cx));

        let extra = vec![self.info_card(
            "后台驻留",
            "关闭主窗口只会隐藏，程序继续在后台运行；要完全退出请点主窗口右下角「退出」。",
            cx,
        )];
        self.page_layout(SettingsPage::General, rows, extra, cx)
    }

    fn render_data_source(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut rows = Vec::new();

        let kinds: Vec<_> = SourceKind::ALL
            .into_iter()
            .map(|kind| (kind, kind.label()))
            .collect();
        let control = self.segmented(
            "data-source-kind",
            &kinds,
            self.data_source.kind,
            Self::set_data_source_kind,
            cx,
        );
        rows.push(self.setting_row(meta("data_source_kind"), control, cx));

        let local_path_control = div()
            .w(px(320.0))
            .child(
                Input::new(&self.local_path_input)
                    .small()
                    .cleanable(true)
                    .w_full(),
            )
            .into_any_element();
        rows.push(self.setting_row(
            meta("data_source_local_path"),
            local_path_control,
            cx,
        ));

        let url_control = div()
            .w(px(320.0))
            .child(
                Input::new(&self.remote_url_input)
                    .small()
                    .cleanable(true)
                    .w_full(),
            )
            .into_any_element();
        rows.push(self.setting_row(meta("data_source_url"), url_control, cx));

        let token_control = div()
            .w(px(320.0))
            .child(
                Input::new(&self.token_input)
                    .small()
                    .cleanable(true)
                    .mask_toggle()
                    .w_full(),
            )
            .into_any_element();
        rows.push(self.setting_row(meta("data_source_token"), token_control, cx));

        let dirty = self.data_source != self.settings.data_source;
        let apply_control = h_flex()
            .items_center()
            .gap_2()
            .child(
                Button::new("data-source-apply")
                    .xsmall()
                    .primary()
                    .label("应用")
                    .on_click(cx.listener(Self::apply_data_source)),
            )
            .child(
                Button::new("data-source-revert")
                    .xsmall()
                    .ghost()
                    .label("还原")
                    .on_click(cx.listener(Self::revert_data_source)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(if dirty { "有未应用的修改" } else { "已保存" }),
            )
            .into_any_element();
        rows.push(self.setting_row(meta("data_source_apply"), apply_control, cx));

        let status = match self.data_source.validation_error() {
            Some(error) => format!("配置有误：{error}"),
            None if dirty => "修改尚未保存。".to_owned(),
            None => format!("当前数据源：{}", self.source.location()),
        };

        let extra = vec![
            self.info_card_with_status("数据源状态", &status, cx),
            self.info_card(
                "远端与本地",
                "远端文件只读取 text 和 tags；使用次数与置顶状态仍保存在本机。启动时先显示本地缓存，网络恢复后点主窗口刷新按钮即可更新。",
                cx,
            ),
            self.info_card(
                "TOKEN 说明",
                "Gitea / Gitee / GitHub / GitLab 的私有仓库需要 TOKEN；公开仓库可以留空。TOKEN 以明文保存在本机 settings.yaml，不会显示在关于页或日志里。",
                cx,
            ),
            self.info_card(
                "自签名证书",
                "GitLab / Gitea 使用自签名 HTTPS 证书时，先把 CA 证书导入 Windows「受信任的根证书颁发机构」；程序通过系统证书库校验证书。",
                cx,
            ),
        ];
        self.page_layout(SettingsPage::DataSource, rows, extra, cx)
    }

    fn render_preview(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut rows = Vec::new();

        let control = self.bool_switch(
            "preview-hover-preview",
            self.settings.hover_preview,
            Self::set_hover_preview,
            cx,
        );
        rows.push(self.setting_row(meta("hover_preview"), control, cx));

        let delays = [
            (0_u64, "0 秒"),
            (100, "0.1 秒"),
            (200, "0.2 秒"),
            (300, "0.3 秒"),
            (400, "0.4 秒"),
        ];
        let control = self.segmented(
            "preview-delay",
            &delays,
            self.settings.hover_preview_delay_ms,
            Self::set_hover_preview_delay,
            cx,
        );
        rows.push(self.setting_row(meta("hover_preview_delay_ms"), control, cx));

        let lines = [(2_usize, "2 行"), (3, "3 行"), (4, "4 行"), (5, "5 行")];
        let control = self.segmented(
            "preview-lines",
            &lines,
            self.settings.max_text_lines,
            Self::set_max_text_lines,
            cx,
        );
        rows.push(self.setting_row(meta("max_text_lines"), control, cx));

        let extra = vec![self.info_card(
            "悬停预览",
            "只有超出卡片行数、未完整显示的内容才会弹出预览；鼠标滚轮滚动时预览会自动收起。",
            cx,
        )];
        self.page_layout(SettingsPage::Preview, rows, extra, cx)
    }

    fn render_appearance(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut rows = Vec::new();

        let themes: Vec<_> = ThemeChoice::ALL
            .into_iter()
            .map(|choice| (choice, choice.label()))
            .collect();
        let control = self.segmented(
            "appearance-theme",
            &themes,
            self.settings.theme,
            Self::set_theme,
            cx,
        );
        rows.push(self.setting_row(meta("theme"), control, cx));

        let radii = [
            (0.0_f32, "直角"),
            (4.0, "小"),
            (6.0, "中"),
            (10.0, "大"),
        ];
        let control = self.segmented(
            "appearance-radius",
            &radii,
            self.settings.card_radius,
            Self::set_card_radius,
            cx,
        );
        rows.push(self.setting_row(meta("card_radius"), control, cx));

        let width = self.settings.tag_sidebar_width;
        let control = h_flex()
            .items_center()
            .gap_1()
            .child(
                Button::new("appearance-sidebar-width-decrease")
                    .xsmall()
                    .ghost()
                    .label("−")
                    .tooltip("收窄标签栏")
                    .on_click(cx.listener(|this, _, window, cx| {
                        let value = (this.settings.tag_sidebar_width
                            - settings::TAG_SIDEBAR_WIDTH_STEP)
                            .max(settings::TAG_SIDEBAR_WIDTH_MIN);
                        if value != this.settings.tag_sidebar_width {
                            this.set_tag_sidebar_width(value, window, cx);
                        }
                    })),
            )
            .child(
                div()
                    .w(px(56.0))
                    .text_center()
                    .text_sm()
                    .child(format!("{width:.0} px")),
            )
            .child(
                Button::new("appearance-sidebar-width-increase")
                    .xsmall()
                    .ghost()
                    .label("+")
                    .tooltip("加宽标签栏")
                    .on_click(cx.listener(|this, _, window, cx| {
                        let value = (this.settings.tag_sidebar_width
                            + settings::TAG_SIDEBAR_WIDTH_STEP)
                            .min(settings::TAG_SIDEBAR_WIDTH_MAX);
                        if value != this.settings.tag_sidebar_width {
                            this.set_tag_sidebar_width(value, window, cx);
                        }
                    })),
            )
            .into_any_element();
        rows.push(self.setting_row(meta("tag_sidebar_width"), control, cx));

        let control = self.bool_switch(
            "appearance-smooth-scroll",
            self.settings.smooth_scroll,
            Self::set_smooth_scroll,
            cx,
        );
        rows.push(self.setting_row(meta("smooth_scroll"), control, cx));

        let extra = vec![self.info_card(
            "主窗口标签栏宽度",
            "窗口较窄时会优先保证右侧卡片可读，标签栏自动收窄；拉宽窗口后恢复设定宽度。",
            cx,
        )];
        self.page_layout(SettingsPage::Appearance, rows, extra, cx)
    }

    fn render_shortcuts(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut rows = Vec::new();
        let recording = self.recording_hotkey;
        let hotkey_label = if recording {
            "请按下组合键…".to_owned()
        } else {
            self.settings.hotkey.label()
        };
        let control = v_flex()
            .items_end()
            .gap_1()
            .child(div().text_sm().font_medium().child(hotkey_label))
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        Button::new("record-hotkey")
                            .xsmall()
                            .when(recording, |button| button.primary())
                            .when(!recording, |button| button.ghost())
                            .label(if recording { "取消" } else { "录制" })
                            .on_click(cx.listener(Self::toggle_recording_hotkey)),
                    )
                    .child(
                        Button::new("default-hotkey")
                            .ghost()
                            .xsmall()
                            .label("默认")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.recording_hotkey = false;
                                this.set_hotkey(HotkeyChoice::default(), window, cx);
                                this.search.focus_handle(cx).focus(window, cx);
                            })),
                    ),
            )
            .into_any_element();
        rows.push(self.setting_row(meta("hotkey"), control, cx));

        let hotkey_state = hotkey::state(cx);
        let status = match (&hotkey_state.label, &hotkey_state.error) {
            (Some(label), _) => format!("当前生效：{label}"),
            (None, Some(error)) => format!("注册失败：{error}"),
            (None, None) => "当前没有可用的全局快捷键".to_owned(),
        };

        let mut shortcut_rows = vec![
            ("Ctrl+F", "聚焦搜索框"),
            ("Ctrl+,", "打开设置"),
            ("Esc", "清空搜索 / 隐藏窗口"),
        ];
        if self.settings.enter_pastes_first {
            shortcut_rows.push(("Enter", "粘贴第一条匹配记录"));
        }

        let fixed = v_flex()
            .id("setting-fixed_shortcuts")
            .w_full()
            .rounded(px(8.0))
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .when(self.highlight == Some("fixed_shortcuts"), |this| {
                this.bg(cx.theme().accent)
            })
            .overflow_hidden()
            .children(shortcut_rows.into_iter().map(|(keys, action)| {
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(div().text_sm().child(action))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(keys),
                    )
                    .into_any_element()
            }));

        let extra = vec![
            self.info_card_with_status("全局快捷键状态", &status, cx),
            self.info_card("主窗口快捷键", "这些快捷键在主窗口内固定生效。", cx),
            fixed.into_any_element(),
        ];
        self.page_layout(SettingsPage::Shortcuts, rows, extra, cx)
    }

    fn info_card_with_status(
        &self,
        title: &'static str,
        status: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        v_flex()
            .w_full()
            .gap_1()
            .rounded(px(8.0))
            .border_1()
            .border_color(theme.border)
            .bg(theme.title_bar)
            .p_3()
            .child(div().text_sm().font_medium().child(title))
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(status.to_owned()),
            )
            .into_any_element()
    }

    fn render_quick_paste(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut rows = Vec::new();

        let click_actions: Vec<_> = ClickAction::ALL
            .into_iter()
            .map(|action| (action, action.label()))
            .collect();
        let control = self.segmented(
            "quick-paste-click",
            &click_actions,
            self.settings.click_action,
            Self::set_click_action,
            cx,
        );
        rows.push(self.setting_row(meta("click_action"), control, cx));

        let paste_actions: Vec<_> = PasteAction::ALL
            .into_iter()
            .map(|action| (action, action.label()))
            .collect();
        let control = self.segmented(
            "quick-paste-action",
            &paste_actions,
            self.settings.paste_action,
            Self::set_paste_action,
            cx,
        );
        rows.push(self.setting_row(meta("paste_action"), control, cx));

        let control = self.bool_switch(
            "quick-paste-track-usage",
            self.settings.track_usage,
            Self::set_track_usage,
            cx,
        );
        rows.push(self.setting_row(meta("track_usage"), control, cx));

        let control = self.bool_switch(
            "quick-paste-clear-search",
            self.settings.clear_search_after_paste,
            Self::set_clear_search_after_paste,
            cx,
        );
        rows.push(self.setting_row(meta("clear_search_after_paste"), control, cx));

        let control = self.bool_switch(
            "quick-paste-enter-first",
            self.settings.enter_pastes_first,
            Self::set_enter_pastes_first,
            cx,
        );
        rows.push(self.setting_row(meta("enter_pastes_first"), control, cx));

        let extra = vec![self.info_card(
            "置顶与排序",
            "卡片右上角的星标用于置顶 / 取消置顶；其余记录按使用次数从多到少排列。",
            cx,
        )];
        self.page_layout(SettingsPage::QuickPaste, rows, extra, cx)
    }

    fn render_about(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut rows = Vec::new();
        let muted_foreground = cx.theme().muted_foreground;

        let version = div()
            .text_sm()
            .child(format!("v{}", env!("CARGO_PKG_VERSION")))
            .into_any_element();
        rows.push(self.setting_row(meta("about_version"), version, cx));

        let data_path = self.data_path.clone();
        let data_control = h_flex()
            .items_center()
            .gap_2()
            .max_w(px(280.0))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .text_xs()
                    .text_color(muted_foreground)
                    .child(self.data_path.display().to_string()),
            )
            .child(
                Button::new("about-open-data")
                    .ghost()
                    .xsmall()
                    .label("打开")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.reveal_path(data_path.clone(), cx)
                    })),
            )
            .into_any_element();
        rows.push(self.setting_row(meta("about_data"), data_control, cx));

        let source = self.source.clone();
        let source_action = match &source {
            ContentSource::Local(path) => {
                let path = path.clone();
                Button::new("about-open-source")
                    .ghost()
                    .xsmall()
                    .label("打开")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.reveal_path(path.clone(), cx)
                    }))
                    .into_any_element()
            }
            ContentSource::Remote { .. } => {
                let url = source.public_url().unwrap_or_default();
                Button::new("about-open-source")
                    .ghost()
                    .xsmall()
                    .label("打开")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_url(url.clone(), cx)
                    }))
                    .into_any_element()
            }
        };
        let source_control = h_flex()
            .items_center()
            .gap_2()
            .max_w(px(280.0))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .text_xs()
                    .text_color(muted_foreground)
                    .child(source.location()),
            )
            .child(source_action)
            .into_any_element();
        rows.push(self.setting_row(meta("about_source"), source_control, cx));

        let log_path = logging::path().map(|path| path.to_path_buf());
        let log_control = if let Some(log_path) = log_path {
            let display_path = log_path.display().to_string();
            let reveal_path = log_path;
            h_flex()
                .items_center()
                .gap_2()
                .max_w(px(280.0))
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .truncate()
                        .text_xs()
                        .text_color(muted_foreground)
                        .child(display_path),
                )
                .child(
                    Button::new("about-open-log")
                        .ghost()
                        .xsmall()
                        .label("打开")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.reveal_path(reveal_path.clone(), cx)
                        })),
                )
                .into_any_element()
        } else {
            div()
                .text_xs()
                .text_color(muted_foreground)
                .child("日志尚未初始化")
                .into_any_element()
        };
        rows.push(self.setting_row(meta("about_log"), log_control, cx));

        let hotkey_state = hotkey::state(cx);
        let hotkey_label = hotkey_state
            .label
            .clone()
            .unwrap_or_else(|| "未注册".into());
        let hotkey_control = div()
            .text_sm()
            .child(hotkey_label)
            .into_any_element();
        rows.push(self.setting_row(meta("about_hotkey"), hotkey_control, cx));

        let homepage_control = h_flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .text_xs()
                    .text_color(muted_foreground)
                    .child("github.com/g-ai-002/quick-paste"),
            )
            .child(
                Button::new("about-open-homepage")
                    .ghost()
                    .xsmall()
                    .label("打开")
                    .on_click(cx.listener(|this, _, _, cx| this.open_homepage(cx))),
            )
            .into_any_element();
        rows.push(self.setting_row(meta("about_homepage"), homepage_control, cx));

        let license_control = div().text_sm().child("MIT").into_any_element();
        rows.push(self.setting_row(meta("about_license"), license_control, cx));

        let extra = vec![self.info_card(
            "快速粘贴",
            "把常用文本存成备忘录，双击即可粘贴回刚才的输入框。所有数据都保存在本机。",
            cx,
        )];
        self.page_layout(SettingsPage::About, rows, extra, cx)
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let pages = SettingsPage::ALL.into_iter().map(|page| {
            let active = self.active_page == page;
            let icon_color = if active {
                theme.sidebar_primary_foreground
            } else {
                theme.sidebar_foreground
            };
            div()
                .id(SharedString::from(format!("settings-nav-{}", page.id())))
                .w_full()
                .h(px(34.0))
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .rounded(px(6.0))
                .text_sm()
                .cursor_pointer()
                .when(active, |this| {
                    this.bg(theme.sidebar_primary)
                        .text_color(theme.sidebar_primary_foreground)
                })
                .when(!active, |this| {
                    this.text_color(theme.sidebar_foreground).hover(|style| {
                        style
                            .bg(theme.sidebar_accent)
                            .text_color(theme.sidebar_accent_foreground)
                    })
                })
                .on_click(
                    cx.listener(move |this, _, window, cx| this.select_page(page, window, cx)),
                )
                .child(
                    Icon::new(page.icon_name())
                        .small()
                        .text_color(icon_color),
                )
                .child(page.title())
                .into_any_element()
        });

        v_flex()
            .w(px(SIDEBAR_WIDTH))
            .h_full()
            .flex_shrink_0()
            .gap_3()
            .p_3()
            .border_r_1()
            .border_color(theme.border)
            .bg(theme.title_bar)
            .child(
                Input::new(&self.search)
                    .small()
                    .w_full()
                    .cleanable(true)
                    .prefix(Icon::new(IconName::Search).small()),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(if self.query.trim().is_empty() {
                        "设置".to_owned()
                    } else {
                        format!("搜索 “{}”", self.query.trim())
                    }),
            )
            .child(v_flex().w_full().gap_1().children(pages))
            .child(div().flex_1())
            .child(
                v_flex()
                    .w_full()
                    .gap_0()
                    .border_t_1()
                    .border_color(theme.border)
                    .pt_3()
                    .child(div().text_sm().child("快速粘贴"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
                    ),
            )
            .into_any_element()
    }

    fn render_search_results(&self, cx: &mut Context<Self>) -> AnyElement {
        let entries = matching_meta(&self.query);
        let theme = cx.theme();
        let count = entries.len();

        let mut page = v_flex()
            .id("settings-search-results")
            .w_full()
            .gap_4()
            .p_5()
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_lg().font_semibold().child("搜索结果"))
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(if count == 0 {
                                "没有匹配的设置项".to_owned()
                            } else {
                                format!("找到 {count} 个设置项")
                            }),
                    ),
            );

        if entries.is_empty() {
            page = page.child(
                v_flex()
                    .w_full()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .py(px(48.0))
                    .child(
                        Icon::new(IconName::Search)
                            .large()
                            .text_color(theme.muted_foreground),
                    )
                    .child(div().text_sm().child("没有匹配的设置项"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("换个关键词试试"),
                    ),
            );
        } else {
            let rows = entries.into_iter().map(|meta| {
                let meta = *meta;
                h_flex()
                    .id(SharedString::from(format!("search-result-{}", meta.id)))
                    .w_full()
                    .items_center()
                    .gap_3()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(theme.border)
                    .cursor_pointer()
                    .hover(|style| style.bg(theme.accent))
                    .on_click(
                        cx.listener(move |this, _, window, cx| {
                            this.select_search_result(&meta, window, cx)
                        }),
                    )
                    .child(
                        Icon::new(meta.page.icon_name())
                            .small()
                            .text_color(theme.muted_foreground),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(div().text_sm().font_medium().child(meta.title))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .truncate()
                                    .child(meta.description),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.primary)
                                    .child(format!(
                                        "{} · {}",
                                        meta.page.title(),
                                        meta.page.description()
                                    )),
                            ),
                    )
                    .into_any_element()
            });

            page = page.child(
                v_flex()
                    .w_full()
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.background)
                    .overflow_hidden()
                    .children(rows),
            );
        }

        page.into_any_element()
    }

    fn render_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted;
        let border = cx.theme().border;
        let danger = cx.theme().danger;
        let content = if !self.query.trim().is_empty() {
            self.render_search_results(cx)
        } else {
            match self.active_page {
                SettingsPage::General => self.render_general(cx),
                SettingsPage::DataSource => self.render_data_source(cx),
                SettingsPage::Preview => self.render_preview(cx),
                SettingsPage::Appearance => self.render_appearance(cx),
                SettingsPage::Shortcuts => self.render_shortcuts(cx),
                SettingsPage::QuickPaste => self.render_quick_paste(cx),
                SettingsPage::About => self.render_about(cx),
            }
        };

        v_flex()
            .id("settings-content-scroll")
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_y_scroll()
            .bg(muted)
            .children(self.notice.clone().map(|notice| {
                h_flex()
                    .w_full()
                    .items_start()
                    .gap_2()
                    .px_5()
                    .py_3()
                    .border_b_1()
                    .border_color(border)
                    .bg(danger.opacity(0.12))
                    .child(Icon::new(IconName::TriangleAlert).small().text_color(danger))
                    .child(
                        div()
                            .flex_1()
                            .text_xs()
                            .text_color(danger)
                            .child(notice),
                    )
            }))
            .child(content)
            .into_any_element()
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            return;
        }

        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;

        if self.recording_hotkey {
            if key == "escape" {
                self.recording_hotkey = false;
                self.notice = None;
                self.search.focus_handle(cx).focus(window, cx);
                cx.notify();
            } else if let Some(binding) =
                hotkey::capture(modifiers.control, modifiers.alt, modifiers.shift, key)
            {
                self.recording_hotkey = false;
                self.set_hotkey(HotkeyChoice::Custom(binding), window, cx);
                self.search.focus_handle(cx).focus(window, cx);
            } else if !matches!(
                key,
                "control" | "ctrl" | "shift" | "alt" | "meta" | "super" | "fn" | "function"
            ) {
                self.notice = Some(
                    "快捷键需要包含 Ctrl、Alt 或 Shift，并搭配字母、数字或功能键".into(),
                );
                cx.notify();
            }
            cx.stop_propagation();
            return;
        }

        if key == "escape" {
            if !self.query.trim().is_empty() {
                self.query.clear();
                self.search
                    .update(cx, |state, cx| state.set_value("", window, cx));
                self.highlight = None;
                cx.notify();
            } else {
                window.remove_window();
            }
            cx.stop_propagation();
        } else if modifiers.control && !modifiers.alt && !modifiers.shift && key == "f" {
            self.search.focus_handle(cx).focus(window, cx);
            cx.stop_propagation();
        }
    }
}

impl Focusable for SettingsWindow {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.search.read(cx).value().to_string();
        if query != self.query {
            self.query = query;
        }

        let sidebar = self.render_sidebar(cx);
        let content = self.render_content(cx);

        v_flex()
            .id("settings-window")
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .track_focus(&self.focus_handle)
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(sidebar)
                    .child(content),
            )
            .on_key_down(cx.listener(Self::on_key_down))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_matches_titles_descriptions_and_keywords() {
        assert!(matching_meta("悬停").iter().any(|meta| meta.id == "hover_preview"));
        assert!(matching_meta("显示器").iter().any(|meta| meta.id == "show_on_cursor_display"));
        assert!(matching_meta("MIT").iter().any(|meta| meta.id == "about_license"));
        assert!(matching_meta("快捷键").iter().any(|meta| meta.id == "hotkey"));
        assert!(matching_meta("不存在的设置").is_empty());
    }

    #[test]
    fn every_page_has_at_least_one_entry() {
        for page in SettingsPage::ALL {
            assert!(
                SETTING_META.iter().any(|meta| meta.page == page),
                "{page:?} 没有设置项"
            );
        }
    }
}
