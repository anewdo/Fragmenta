//! 外观组：主题色卡与编辑器 / 小窗排版卡片。

use fragmenta_core::settings::ThemeMode;
use gpui_kit::assets::IconName;
use gpui_kit::component::combobox::Combobox;
use gpui_kit::component::slider::{Slider, SliderState};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::common::{card_header, setting_row};
use super::{FontCombobox, SettingsPage};
use crate::settings::SettingsStore;
use crate::ui::layout::{CardStyle, setting_card};
use crate::ui::theme;
use crate::ui::tokens::{self, TextRole, TextRoleExt as _};

/// 主题色卡尺寸：设置页局部控件特例。
const THEME_SWATCH_WIDTH: Pixels = px(88.);
const THEME_SWATCH_HEIGHT: Pixels = px(52.);
/// 字号 / 行距滑杆宽度：范围仅十余档，短轨让每档的拖动距离足够可感。
const FONT_SLIDER_WIDTH: Pixels = px(220.);
/// 字体下拉宽度：与滑杆同宽，保持各控件左对齐。
const FONT_FAMILY_WIDTH: Pixels = px(220.);
/// 滑杆数值标签宽度：数字位数变化时不挤动滑杆。
const VALUE_LABEL_WIDTH: Pixels = px(56.);
/// 字号滑杆下方的实时示例词：中文 / 英文各一。
const EDITOR_FONT_SAMPLE: (&str, &str) = ("笔记", "Note");
const TILE_FONT_SAMPLE: (&str, &str) = ("速记", "Memo");

impl SettingsPage {
    /// "外观"组：主题色卡单选 + 编辑器 / 小窗排版（字体、字号、行距）。
    pub(super) fn appearance_group(&self, cx: &App) -> impl IntoElement {
        let (theme, editor_family, tile_family) = {
            let settings = cx.global::<SettingsStore>().0.read(cx);
            (
                settings.theme(),
                settings.editor_font_family().map(str::to_string),
                settings.tile_font_family().map(str::to_string),
            )
        };
        let (muted, primary, border, foreground) = {
            let colors = cx.theme();
            (
                colors.muted_foreground,
                colors.primary,
                colors.border,
                colors.foreground,
            )
        };
        let card = CardStyle::from_theme(cx.theme());
        v_flex()
            .w_full()
            .gap(tokens::spacing::SM)
            .child(setting_card(
                card,
                theme_card(theme, muted, primary, border, cx),
            ))
            .child(typography_card(
                IconName::Type,
                "编辑器排版",
                "仅在主窗口编辑器生效：调整标题与正文的字体、字号与行距；预览与左侧列表保持内置样式",
                &self.editor_font_combo,
                self.editor_font_preview,
                self.editor_line_preview,
                &self.editor_font_slider,
                &self.editor_line_slider,
                editor_family.as_deref(),
                EDITOR_FONT_SAMPLE,
                muted,
                foreground,
                card,
            ))
            .child(typography_card(
                IconName::AppWindow,
                "小窗排版",
                "仅在磁贴小窗生效：调整标题与正文的字体、字号与行距",
                &self.tile_font_combo,
                self.tile_font_preview,
                self.tile_line_preview,
                &self.tile_font_slider,
                &self.tile_line_slider,
                tile_family.as_deref(),
                TILE_FONT_SAMPLE,
                muted,
                foreground,
                card,
            ))
    }
}

/// 排版设置卡：字体下拉、字号与行距滑杆（与标题同行）与实时示例。
#[allow(clippy::too_many_arguments)]
fn typography_card(
    icon: IconName,
    title: &'static str,
    description: &'static str,
    font_combo: &Entity<FontCombobox>,
    size_value: f32,
    line_value: f32,
    size_slider: &Entity<SliderState>,
    line_slider: &Entity<SliderState>,
    family: Option<&str>,
    samples: (&'static str, &'static str),
    muted: Hsla,
    foreground: Hsla,
    card: CardStyle,
) -> impl IntoElement {
    setting_card(
        card,
        v_flex()
            .w_full()
            .gap(tokens::spacing::MD)
            .child(card_header(icon, title, description, muted))
            .child(setting_row(
                IconName::Type,
                "字体",
                "仅列常用阅读与编辑器字体，默认跟随系统",
                muted,
                div()
                    .w(FONT_FAMILY_WIDTH)
                    .child(Combobox::new(font_combo).w_full()),
            ))
            .child(setting_row(
                IconName::ALargeSmall,
                "字号",
                "12–24 px，拖动滑杆即时预览",
                muted,
                h_flex()
                    .gap(tokens::spacing::MD)
                    .child(Slider::new(size_slider).w(FONT_SLIDER_WIDTH))
                    .child(value_label(format!("{size_value:.0} px"), muted)),
            ))
            .child(setting_row(
                IconName::Baseline,
                "行距",
                "1.2–2.0 倍，标题与正文同步生效",
                muted,
                h_flex()
                    .gap(tokens::spacing::MD)
                    .child(Slider::new(line_slider).w(FONT_SLIDER_WIDTH))
                    .child(value_label(format!("{line_value:.2} 倍"), muted)),
            ))
            .child(typography_sample(
                samples, size_value, line_value, family, muted, foreground,
            )),
    )
}

/// 滑杆右侧数值：固定宽度，数值位数变化时布局不跳。
fn value_label(text: String, muted: Hsla) -> impl IntoElement {
    div()
        .w(VALUE_LABEL_WIDTH)
        .text_role(TextRole::ControlLabel)
        .text_color(muted)
        .child(text)
}

/// 实时排版示例：字号与行距取自滑杆拖动值，字体取自已提交的字体设置。
fn typography_sample(
    samples: (&'static str, &'static str),
    size: f32,
    line_height: f32,
    family: Option<&str>,
    muted: Hsla,
    foreground: Hsla,
) -> impl IntoElement {
    v_flex()
        .w_full()
        .gap(tokens::spacing::XS)
        .child(
            div()
                .text_role(TextRole::Meta)
                .text_color(muted)
                .child("示例"),
        )
        .child(typography_sample_line(
            samples.0,
            size,
            line_height,
            family,
            foreground,
        ))
        .child(typography_sample_line(
            samples.1,
            size,
            line_height,
            family,
            foreground,
        ))
}

/// 单行示例词：行高作用于每个文本行，两行叠加即可观察行距变化。
fn typography_sample_line(
    word: &'static str,
    size: f32,
    line_height: f32,
    family: Option<&str>,
    color: Hsla,
) -> impl IntoElement {
    div()
        .text_size(px(size))
        .line_height(relative(line_height))
        .text_color(color)
        .when_some(family, |el, family| el.font_family(family))
        .child(word)
}

/// 主题色卡预览的三区域颜色：菜单 / 标题栏 / 内容。
struct MockColors {
    chrome: Hsla,
    title: Hsla,
    content: Hsla,
}

/// 单套配色的矩形预览块：左条 = 侧栏、上条 = 标题栏、其余 = 内容。
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

/// 缩略图读取实际主题定义，与窗口及开发重载保持一致。
fn theme_mock(theme: ThemeMode, cx: &App) -> MockColors {
    let theme = theme::preview(theme, cx);
    MockColors {
        chrome: theme.sidebar,
        title: theme.title_bar,
        content: theme.background,
    }
}

/// 主题色块共用的三个主题色：选中描边、未选中描边与标签文字。
#[derive(Clone, Copy)]
struct SwatchColors {
    primary: Hsla,
    border: Hsla,
    muted: Hsla,
}

/// 主题卡：说明文字 + 矩形颜色按钮行（选中即整套配色生效，枚举单选）。
fn theme_card(
    theme: ThemeMode,
    muted: Hsla,
    primary: Hsla,
    border: Hsla,
    cx: &App,
) -> impl IntoElement {
    let colors = SwatchColors {
        primary,
        border,
        muted,
    };
    v_flex()
        .w_full()
        .gap(tokens::spacing::SM)
        .child(card_header(
            IconName::Palette,
            "主题",
            "切换即时生效于全部窗口；自动仅在浅色 / 深色之间跟随系统",
            muted,
        ))
        .child(
            h_flex()
                .gap(tokens::spacing::MD)
                .flex_wrap()
                .child(theme_swatch(
                    "theme-pick-auto",
                    "自动",
                    theme == ThemeMode::Auto,
                    ThemeMode::Auto,
                    // 左半 Light / 右半 Dark：跟随系统深浅色的双色预览
                    h_flex()
                        .size_full()
                        .child(
                            mock_preview(theme_mock(ThemeMode::Light, cx), px(9.), px(10.))
                                .flex_1()
                                .min_w_0(),
                        )
                        .child(
                            mock_preview(theme_mock(ThemeMode::Dark, cx), px(9.), px(10.))
                                .flex_1()
                                .min_w_0(),
                        ),
                    colors,
                ))
                .child(theme_swatch(
                    "theme-pick-light",
                    "浅色",
                    theme == ThemeMode::Light,
                    ThemeMode::Light,
                    mock_preview(theme_mock(ThemeMode::Light, cx), px(18.), px(12.)),
                    colors,
                ))
                .child(theme_swatch(
                    "theme-pick-dark",
                    "深色",
                    theme == ThemeMode::Dark,
                    ThemeMode::Dark,
                    mock_preview(theme_mock(ThemeMode::Dark, cx), px(18.), px(12.)),
                    colors,
                ))
                .child(theme_swatch(
                    "theme-pick-literary-green",
                    "绿色",
                    theme == ThemeMode::LiteraryGreen,
                    ThemeMode::LiteraryGreen,
                    mock_preview(theme_mock(ThemeMode::LiteraryGreen, cx), px(18.), px(12.)),
                    colors,
                ))
                .child(theme_swatch(
                    "theme-pick-ceramic",
                    "橙色",
                    theme == ThemeMode::Ceramic,
                    ThemeMode::Ceramic,
                    mock_preview(theme_mock(ThemeMode::Ceramic, cx), px(18.), px(12.)),
                    colors,
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
    colors: SwatchColors,
) -> impl IntoElement {
    let SwatchColors {
        primary,
        border,
        muted,
    } = colors;
    v_flex()
        .gap(tokens::spacing::XS)
        .child(
            div()
                .id(id)
                .cursor_pointer()
                .on_click(move |_, _, cx| {
                    let settings = cx.global::<SettingsStore>().0.clone();
                    settings.update(cx, |state, cx| state.set_theme(mode, cx));
                })
                .w(THEME_SWATCH_WIDTH)
                .h(THEME_SWATCH_HEIGHT)
                .rounded(tokens::radius::SM)
                .overflow_hidden()
                .border_2()
                .border_color(if selected { primary } else { border })
                .when(!selected, |el| {
                    el.hover(|style| style.border_color(primary.opacity(0.6)))
                })
                .child(preview),
        )
        .child(
            h_flex().w(THEME_SWATCH_WIDTH).justify_center().child(
                div()
                    .text_role(TextRole::Meta)
                    .text_color(if selected { primary } else { muted })
                    .child(label),
            ),
        )
}
