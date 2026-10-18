//! 设置页（架构 §5.8 / §5.11）：Phase 13 分组实装（GroupBox 分组卡片）。
//!
//! 已交付："外观"组（主题矩形色卡单选：Light / Dark / Auto + 内置色系，
//! ADR-0005，与字号三档）与"系统"组（关闭主窗口时直接退出，默认关 =
//! 托盘驻留）。退出入口收敛至托盘菜单。快捷键 / 数据组由后续 ticket 落地。

use fragmenta_core::settings::{FontScale, ThemeMode};
use gpui_kit::component::group_box::GroupBox;
use gpui_kit::component::radio::RadioGroup;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::settings::SettingsStore;
use crate::settings::theme_schemes;

/// 设置页视图：常驻于主窗口 shell，订阅设置广播刷新展示。
pub(crate) struct SettingsPage {
    _subscription: Subscription,
}

impl SettingsPage {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let settings = cx.global::<SettingsStore>().0.clone();
        let subscription = cx.subscribe(&settings, |_, _, _, cx| {
            // 设置变更（含本页操作）→ 刷新展示
            cx.notify();
        });
        Self {
            _subscription: subscription,
        }
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
