use fragmenta_core::settings::{FontScale, ThemeMode};
use gpui_kit::component::button::Button;
use gpui_kit::component::group_box::GroupBox;
use gpui_kit::component::radio::RadioGroup;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::hotkey::HotkeyError;
use crate::settings::SettingsStore;
use crate::settings::theme_schemes;

/// 设置页视图：常驻于主窗口 shell，订阅设置广播刷新展示。
pub(crate) struct SettingsPage {
    _subscription: Subscription,
    /// 热键录入框焦点：监听态接收按键的载体。
    hotkey_focus: FocusHandle,
    /// 热键录入框是否处于监听态（点击进入，捕获完成 / 取消退出）。
    hotkey_listening: bool,
    /// 监听中和弦进行中已按下的修饰键展示（如 "Ctrl + Alt"）。
    hotkey_pending_mods: Option<String>,
    /// 热键录入错误提示（红字展示）。
    hotkey_error: Option<&'static str>,
    /// 数据目录迁移错误提示（红字展示）。
    data_dir_error: Option<String>,
}

impl SettingsPage {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings = cx.global::<SettingsStore>().0.clone();
        let subscription = cx.subscribe(&settings, |_, _, _, cx| {
            // 设置变更（含本页操作）→ 刷新展示
            cx.notify();
        });
        Self {
            _subscription: subscription,
            hotkey_focus: cx.focus_handle(),
            hotkey_listening: false,
            hotkey_pending_mods: None,
            hotkey_error: None,
            data_dir_error: None,
        }
    }

    /// "快捷键"组：磁贴呼出热键捕获式录入框与红字错误提示。
    fn hotkey_group(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let hotkey = cx
            .global::<SettingsStore>()
            .0
            .read(cx)
            .tile_hotkey()
            .to_string();
        let (muted, primary, border, danger) = {
            let colors = cx.theme();
            (
                colors.muted_foreground,
                colors.primary,
                colors.border,
                colors.danger,
            )
        };
        let focus = self.hotkey_focus.clone();
        let listening = self.hotkey_listening;
        let pending_mods = self.hotkey_pending_mods.clone();
        let error = self.hotkey_error;

        let recorder = div()
            .id("hotkey-recorder")
            .track_focus(&focus)
            .cursor_pointer()
            .flex()
            .items_center()
            .justify_center()
            .w(px(240.))
            .h(px(32.))
            .rounded(px(6.))
            .border_1()
            .border_color(if listening { primary } else { border })
            .child(match (listening, pending_mods.as_deref()) {
                (true, Some(mods)) => div()
                    .text_sm()
                    .text_color(muted)
                    .child(format!("{mods} + …")),
                (true, None) => div().text_sm().text_color(muted).child("按下新组合键…"),
                (false, _) => div().child(pretty_hotkey(&hotkey)),
            })
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                // 点击框外退出监听态，不误改热键
                if this.hotkey_listening {
                    this.hotkey_listening = false;
                    this.hotkey_pending_mods = None;
                    this.hotkey_error = None;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(|this, _, window, cx| {
                // 点击录入框：聚焦并进入监听态
                window.focus(&this.hotkey_focus, cx);
                this.hotkey_listening = true;
                this.hotkey_pending_mods = None;
                this.hotkey_error = None;
                cx.notify();
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if !this.hotkey_listening {
                    return;
                }
                // 监听态吞掉全部按键，避免触发其他快捷键
                window.prevent_default();
                cx.stop_propagation();
                let stroke = &event.keystroke;
                match stroke.key.as_str() {
                    // Esc 退出监听态，不误改热键
                    "escape" => {
                        this.hotkey_listening = false;
                        this.hotkey_pending_mods = None;
                        this.hotkey_error = None;
                    }
                    // 修饰键自身按下：和弦进行中，展示由 on_modifiers_changed 维护
                    "shift" | "control" | "alt" | "platform" | "meta" | "super" | "cmd" | "win"
                    | "function" | "fn" => {}
                    _ => {
                        this.hotkey_pending_mods = None;
                        let candidate = hotkey_candidate(stroke);
                        let store = cx.global::<SettingsStore>().0.clone();
                        match store.update(cx, |state, cx| state.set_hotkey(&candidate, cx)) {
                            Ok(()) => {
                                this.hotkey_listening = false;
                                this.hotkey_error = None;
                            }
                            Err(HotkeyError::Parse(_)) => {
                                this.hotkey_error = Some("无效组合，请按其他组合键");
                            }
                            Err(HotkeyError::Register(_)) => {
                                this.hotkey_listening = false;
                                this.hotkey_error = Some("组合键被其他程序占用，已保留原热键");
                            }
                        }
                    }
                }
                cx.notify();
            }))
            .on_modifiers_changed(cx.listener(|this, event: &ModifiersChangedEvent, _, cx| {
                if !this.hotkey_listening {
                    return;
                }
                if event.modifiers.number_of_modifiers() == 0 {
                    // 和弦中松开全部修饰键而未按非修饰键：纯修饰键组合
                    if this.hotkey_pending_mods.take().is_some() {
                        this.hotkey_error = Some("纯修饰键组合无效，请按下一个非修饰键");
                        cx.notify();
                    }
                } else {
                    let mods = pretty_modifiers(&event.modifiers);
                    if this.hotkey_pending_mods.as_deref() != Some(mods.as_str()) {
                        this.hotkey_pending_mods = Some(mods);
                        cx.notify();
                    }
                }
            }));

        v_flex()
            .w_full()
            .gap_2()
            .child(setting_row(
                "磁贴呼出热键",
                "点击录入框后直接按下新组合键完成改绑；Esc 或点击框外取消",
                muted,
                recorder,
            ))
            .when_some(error, |el, err| {
                el.child(
                    h_flex()
                        .w_full()
                        .justify_end()
                        .child(div().text_sm().text_color(danger).child(err)),
                )
            })
    }
    /// "数据"组（ticket 06）：三行布局——当前目录（可打开）／更改存储路径
    /// （目录选择器选完即迁移）／重置为默认位置；迁移错误红字展示。
    fn data_group(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let (muted, danger) = {
            let colors = cx.theme();
            (colors.muted_foreground, colors.danger)
        };
        let current = cx
            .global::<SettingsStore>()
            .0
            .read(cx)
            .data_dir()
            .display()
            .to_string();
        let error = self.data_dir_error.clone();

        v_flex()
            .w_full()
            .gap_4()
            // 行 1：当前数据存储目录 + 打开
            .child(
                v_flex()
                    .w_full()
                    .gap_1()
                    .child(
                        h_flex()
                            .w_full()
                            .gap_2()
                            .child(div().flex_1().child("当前数据存储目录"))
                            .child(Button::new("data-dir-open").label("打开当前目录").on_click(
                                |_, _, cx| {
                                    let dir = cx
                                        .global::<SettingsStore>()
                                        .0
                                        .read(cx)
                                        .data_dir()
                                        .to_path_buf();
                                    crate::win32::open_folder(&dir);
                                },
                            )),
                    )
                    .child(div().text_sm().text_color(muted).child(current)),
            )
            // 行 2：更改存储路径（目录选择器选完即迁移）
            .child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .child(div().flex_1().child("更改存储路径"))
                    .child(
                        Button::new("data-dir-change").label("选择新位置").on_click(cx.listener(
                            |this, _, window, cx| {
                                // 系统目录选择器（COM IFileOpenDialog）；取消则不迁移
                                let owner = crate::win32::window_hwnd(window);
                                let Some(picked) = crate::win32::pick_folder(owner) else {
                                    return;
                                };
                                let store = cx.global::<SettingsStore>().0.clone();
                                match store.update(cx, |state, cx| state.set_data_dir(picked, cx))
                                {
                                    Ok(()) => this.data_dir_error = None,
                                    Err(err) => this.data_dir_error = Some(err.to_string()),
                                }
                                cx.notify();
                            },
                        )),
                    ),
            )
            // 行 3：重置为默认位置（exe 旁 data/）
            .child(
                v_flex()
                    .w_full()
                    .gap_1()
                    .child(
                        h_flex()
                            .w_full()
                            .gap_2()
                            .child(div().flex_1().child("重置为默认位置"))
                            .child(
                                Button::new("data-dir-reset").label("重置").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        // 默认位置与启动逻辑同源（{exe_dir}/data）
                                        let target =
                                            fragmenta_core::settings::Settings::default().data_dir;
                                        let store = cx.global::<SettingsStore>().0.clone();
                                        let already_default = store.read_with(cx, |state, _| {
                                            state.data_dir() == target
                                        });
                                        if already_default {
                                            return;
                                        }
                                        match store.update(cx, |state, cx| {
                                            state.set_data_dir(target, cx)
                                        }) {
                                            Ok(()) => this.data_dir_error = None,
                                            Err(err) => {
                                                this.data_dir_error = Some(err.to_string())
                                            }
                                        }
                                        cx.notify();
                                    }),
                                ),
                            ),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child("将数据存储位置重置为 exe 所在目录"),
                    ),
            )
            .when_some(error, |el, err| {
                el.child(
                    h_flex()
                        .w_full()
                        .justify_end()
                        .child(div().text_sm().text_color(danger).child(err)),
                )
            })
    }
}

impl Render for SettingsPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = cx.global::<SettingsStore>().0.read(cx);
        let (theme, font_scale, close_to_quit) = (
            settings.theme(),
            settings.font_scale(),
            settings.close_to_quit(),
        );
        let (muted, primary, border) = {
            let theme_colors = cx.theme();
            (
                theme_colors.muted_foreground,
                theme_colors.primary,
                theme_colors.border,
            )
        };
        v_flex()
            .size_full()
            .items_center()
            .overflow_y_scrollbar()
            .p_6()
            .child(
                v_flex()
                    .w_full()
                    .max_w(px(560.))
                    .gap_4()
                    .child(
                        div()
                            .text_size(px(20.))
                            .text_color(cx.theme().foreground.opacity(0.4))
                            .child("设置"),
                    )
                    .child(
                        GroupBox::new()
                            .title("外观")
                            .child(appearance_group(theme, font_scale, muted, primary, border)),
                    )
                    .child(GroupBox::new().title("快捷键").child(self.hotkey_group(cx)))
                    .child(GroupBox::new().title("数据").child(self.data_group(cx)))
                    .child(
                        GroupBox::new()
                            .title("系统")
                            .child(system_group(close_to_quit, muted)),
                    ),
            )
    }
}

/// "外观"组：主题色卡单选与字号三档，切换即时生效于全部存活窗口。
fn appearance_group(
    theme: ThemeMode,
    font_scale: FontScale,
    muted: Hsla,
    primary: Hsla,
    border: Hsla,
) -> impl IntoElement {
    v_flex()
        .w_full()
        .gap_4()
        .child(theme_section(theme, muted, primary, border))
        .child(font_scale_row(font_scale, muted))
}

/// 主题色卡预览的三区域颜色：菜单 / 标题栏 / 内容。
struct MockColors {
    chrome: Hsla,
    title: Hsla,
    content: Hsla,
}

/// 单套配色的矩形预览块：左条 = 菜单（深）、上条 = 标题栏、其余 = 内容（淡）。
fn mock_preview(colors: MockColors, chrome_w: Pixels, title_h: Pixels) -> Div {
    h_flex()
        .size_full()
        .child(div().w(chrome_w).h_full().flex_shrink_0().bg(colors.chrome))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .h_full()
                .child(div().h(title_h).flex_shrink_0().bg(colors.title))
                .child(div().flex_1().bg(colors.content)),
        )
}

/// Light / Dark 基线预览色（与 kit 默认主题同族的中性色）。
fn light_mock() -> MockColors {
    MockColors {
        chrome: rgb(0xE5E5E5).into(),
        title: rgb(0xF0F0F0).into(),
        content: rgb(0xFFFFFF).into(),
    }
}

/// Dark 基线预览色。
fn dark_mock() -> MockColors {
    MockColors {
        chrome: rgb(0x262626).into(),
        title: rgb(0x171717).into(),
        content: rgb(0x0A0A0A).into(),
    }
}

/// 内置色系预览色：菜单与标题栏取深调、内容取淡调（与落色 token 一致）。
fn scheme_mock(scheme: &theme_schemes::ThemeScheme) -> MockColors {
    MockColors {
        chrome: scheme.sidebar,
        title: scheme.title_bar,
        content: scheme.background,
    }
}

/// 主题节：说明文字 + 矩形颜色按钮行（选中即整套配色生效，枚举单选）。
fn theme_section(theme: ThemeMode, muted: Hsla, primary: Hsla, border: Hsla) -> impl IntoElement {
    v_flex()
        .w_full()
        .gap_2()
        .child(
            v_flex().gap_1().child("主题").child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("切换即时生效于全部窗口；Auto 跟随系统深浅色"),
            ),
        )
        .child(
            h_flex()
                .gap_3()
                .flex_wrap()
                .child(theme_swatch(
                    "theme-pick-light",
                    "Light",
                    theme == ThemeMode::Light,
                    ThemeMode::Light,
                    mock_preview(light_mock(), px(18.), px(12.)),
                    primary,
                    border,
                    muted,
                ))
                .child(theme_swatch(
                    "theme-pick-dark",
                    "Dark",
                    theme == ThemeMode::Dark,
                    ThemeMode::Dark,
                    mock_preview(dark_mock(), px(18.), px(12.)),
                    primary,
                    border,
                    muted,
                ))
                .child(theme_swatch(
                    "theme-pick-auto",
                    "Auto",
                    theme == ThemeMode::Auto,
                    ThemeMode::Auto,
                    // 左半 Light / 右半 Dark：跟随系统深浅色的双色预览
                    h_flex()
                        .size_full()
                        .child(
                            mock_preview(light_mock(), px(9.), px(10.))
                                .flex_1()
                                .min_w_0(),
                        )
                        .child(
                            mock_preview(dark_mock(), px(9.), px(10.))
                                .flex_1()
                                .min_w_0(),
                        ),
                    primary,
                    border,
                    muted,
                ))
                .child(theme_swatch(
                    "theme-pick-dousha",
                    theme_schemes::dou_sha_green().label,
                    theme == ThemeMode::DouShaGreen,
                    ThemeMode::DouShaGreen,
                    mock_preview(
                        scheme_mock(&theme_schemes::dou_sha_green()),
                        px(18.),
                        px(12.),
                    ),
                    primary,
                    border,
                    muted,
                ))
                .child(theme_swatch(
                    "theme-pick-skyblue",
                    theme_schemes::sky_blue().label,
                    theme == ThemeMode::SkyBlue,
                    ThemeMode::SkyBlue,
                    mock_preview(scheme_mock(&theme_schemes::sky_blue()), px(18.), px(12.)),
                    primary,
                    border,
                    muted,
                )),
        )
}

/// 矩形颜色按钮：窗口形预览块 + 下方标签；选中描主色边，点击整套配色生效。
fn theme_swatch(
    id: &'static str,
    label: &'static str,
    selected: bool,
    mode: ThemeMode,
    preview: impl IntoElement,
    primary: Hsla,
    border: Hsla,
    muted: Hsla,
) -> impl IntoElement {
    v_flex()
        .gap_1()
        .child(
            div()
                .id(id)
                .cursor_pointer()
                .on_click(move |_, _, cx| {
                    let settings = cx.global::<SettingsStore>().0.clone();
                    settings.update(cx, |state, cx| state.set_theme(mode, cx));
                })
                .w(px(88.))
                .h(px(52.))
                .rounded(px(6.))
                .overflow_hidden()
                .border_2()
                .border_color(if selected { primary } else { border })
                .when(!selected, |el| {
                    el.hover(|style| style.border_color(primary.opacity(0.6)))
                })
                .child(preview),
        )
        .child(
            h_flex().w(px(88.)).justify_center().child(
                div()
                    .text_sm()
                    .text_color(if selected { primary } else { muted })
                    .child(label),
            ),
        )
}

/// 字号行：三档单选。
fn font_scale_row(font_scale: FontScale, muted: Hsla) -> impl IntoElement {
    setting_row(
        "正文字号",
        "调整笔记、待办与磁贴的正文文字大小",
        muted,
        RadioGroup::horizontal("font-scale")
            .selected_index(match font_scale {
                FontScale::Small => Some(0),
                FontScale::Medium => Some(1),
                FontScale::Large => Some(2),
            })
            .on_change(|ix, _, cx| {
                let scale = match *ix {
                    0 => FontScale::Small,
                    1 => FontScale::Medium,
                    _ => FontScale::Large,
                };
                let settings = cx.global::<SettingsStore>().0.clone();
                settings.update(cx, |state, cx| state.set_font_scale(scale, cx));
            })
            .children(["小", "中", "大"]),
    )
}

/// "系统"组：关闭主窗口时直接退出开关。
fn system_group(close_to_quit: bool, muted: Hsla) -> impl IntoElement {
    setting_row(
        "关闭主窗口时直接退出",
        "默认关闭：关闭主窗口后驻留系统托盘，热键呼出磁贴不受影响",
        muted,
        Switch::new("close-to-quit")
            .checked(close_to_quit)
            .on_click(|checked: &bool, _window, cx| {
                let settings = cx.global::<SettingsStore>().0.clone();
                settings.update(cx, |state, cx| state.set_close_to_quit(*checked, cx));
            }),
    )
}

/// 设置行布局：左侧标题 + 说明，右侧控件。
fn setting_row(
    title: &'static str,
    description: &'static str,
    muted: Hsla,
    field: impl IntoElement,
) -> impl IntoElement {
    h_flex()
        .w_full()
        .items_center()
        .justify_between()
        .gap_4()
        .child(
            v_flex()
                .gap_1()
                .child(div().child(title))
                .child(div().text_sm().text_color(muted).child(description)),
        )
        .child(field)
}

/// gpui 按键事件 → global_hotkey 解析串（如 "ctrl+alt+n"）。
///
/// 键名统一大写以匹配解析器（"n"→"N"、"f5"→"F5"，符号与数字原样）；
/// 组合序为 shift + ctrl + alt + win + 主键。gpui 对 shift 组合键可能上报
/// 变形字符（如 "<"），此类候选会在解析层被拒并红字提示。
fn hotkey_candidate(stroke: &Keystroke) -> String {
    let mods = &stroke.modifiers;
    let mut candidate = String::new();
    if mods.shift {
        candidate.push_str("shift+");
    }
    if mods.control {
        candidate.push_str("ctrl+");
    }
    if mods.alt {
        candidate.push_str("alt+");
    }
    if mods.platform {
        candidate.push_str("super+");
    }
    candidate.push_str(&stroke.key.to_uppercase());
    candidate
}

/// 修饰键状态 → 展示串（如 "Ctrl + Alt"）。
fn pretty_modifiers(mods: &Modifiers) -> String {
    let mut parts = Vec::new();
    if mods.control {
        parts.push("Ctrl");
    }
    if mods.alt {
        parts.push("Alt");
    }
    if mods.shift {
        parts.push("Shift");
    }
    if mods.platform {
        parts.push("Win");
    }
    parts.join(" + ")
}

/// 设置串 → 展示串："ctrl+alt+n" → "Ctrl + Alt + N"。
/// 兼容 global_hotkey 规范形式（"KeyN" / "Digit5"）与常见修饰键别名。
fn pretty_hotkey(hotkey: &str) -> String {
    hotkey
        .split('+')
        .map(pretty_token)
        .collect::<Vec<_>>()
        .join(" + ")
}

/// 单个热键 token → 展示形式。
fn pretty_token(token: &str) -> String {
    let trimmed = token.trim();
    let lower = trimmed.to_lowercase();
    match lower.as_str() {
        "ctrl" | "control" => return "Ctrl".to_string(),
        "alt" => return "Alt".to_string(),
        "shift" => return "Shift".to_string(),
        "super" | "cmd" | "win" => return "Win".to_string(),
        _ => {}
    }
    if let Some(letter) = lower.strip_prefix("key").filter(|rest| rest.len() == 1) {
        return letter.to_uppercase();
    }
    if let Some(digit) = lower.strip_prefix("digit").filter(|rest| rest.len() == 1) {
        return digit.to_string();
    }
    // 单字符键大写；命名键（f5 / space 等）首字母大写
    let mut chars = trimmed.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => String::new(),
    }
}
