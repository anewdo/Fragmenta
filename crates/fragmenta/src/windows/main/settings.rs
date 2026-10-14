//! 设置页（架构 §5.8 / §5.11）：Phase 13 分组实装（GroupBox 分组卡片）。
//!
//! 已交付："外观"组（主题 Light / Dark / Auto 单选 + 字号三档）与"系统"组
//! （关闭主窗口时直接退出，默认关 = 托盘驻留）。退出入口收敛至托盘菜单。
//! 快捷键 / 数据组由后续 ticket 落地。

use fragmenta_core::settings::{FontScale, ThemeMode};
use gpui_kit::component::group_box::GroupBox;
use gpui_kit::component::radio::RadioGroup;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::app::SettingsStore;

/// 设置页视图：常驻于主窗口 shell，订阅设置广播刷新展示。
pub struct SettingsPage {
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
        let muted = cx.theme().muted_foreground;
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
                            .child(appearance_group(theme, font_scale, muted)),
                    )
                    .child(
                        GroupBox::new()
                            .title("系统")
                            .child(system_group(close_to_quit, muted)),
                    ),
            )
    }
}

/// "外观"组：主题三模式单选与字号三档，切换即时生效于全部存活窗口。
fn appearance_group(theme: ThemeMode, font_scale: FontScale, muted: Hsla) -> impl IntoElement {
    v_flex()
        .w_full()
        .gap_4()
        .child(theme_row(theme, muted))
        .child(font_scale_row(font_scale, muted))
}

/// 主题行：Light / Dark / Auto 单选。
fn theme_row(theme: ThemeMode, muted: Hsla) -> impl IntoElement {
    setting_row(
        "主题",
        "切换即时生效于全部窗口；Auto 跟随系统深浅色",
        muted,
        RadioGroup::horizontal("theme-mode")
            .selected_index(match theme {
                ThemeMode::Light => Some(0),
                ThemeMode::Dark => Some(1),
                ThemeMode::Auto => Some(2),
            })
            .on_change(|ix, _, cx| {
                let mode = match *ix {
                    0 => ThemeMode::Light,
                    1 => ThemeMode::Dark,
                    _ => ThemeMode::Auto,
                };
                let settings = cx.global::<SettingsStore>().0.clone();
                settings.update(cx, |state, cx| state.set_theme(mode, cx));
            })
            .children(["Light", "Dark", "Auto"]),
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
