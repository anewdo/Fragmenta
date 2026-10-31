//! 通用选色内容：无色 + 内置 6 色色板 + 自定义（hex 输入 + HSL 滑杆，
//! 状态机复用 kit `ColorPickerState`）。
//!
//! 笔记页的标签编辑弹窗以 Popover 承载 [`color_popover_content`]；标签管理
//! 页的新建 / 编辑弹窗直接内联 [`color_picker_content`]。选定语义由调用方经
//! `on_select` 回调决定，自定义滑杆的即时变更由调用方订阅
//! `ColorPickerEvent` 处理。

use std::rc::Rc;

use gpui_kit::component::color_picker::ColorPickerState;
use gpui_kit::component::input::Input;
use gpui_kit::component::popover::PopoverState;
use gpui_kit::component::separator::Separator;
use gpui_kit::component::slider::Slider;
use gpui_kit::component::{
    ActiveTheme as _, Colorize as _, Selectable, Sizable as _, Theme, h_flex, v_flex,
};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::ui::focus::{TextFieldExt as _, input_border};
use crate::ui::tokens::{self, TextRole, TextRoleExt};
use crate::utils::palette::{format_hex_color, palette_colors};

/// 色板色块边长。
const SWATCH: Pixels = px(20.);
/// 触发器色块宽高：收起态的矩形色块尺寸。
const TRIGGER_WIDTH: Pixels = px(30.);
const TRIGGER_HEIGHT: Pixels = px(18.);
/// 弹层内容的固定宽度。
const POPOVER_WIDTH: Pixels = px(232.);

/// 色相滑杆轨道的分段数（离散色块拼出彩虹带）。
const HUE_STEPS: usize = 12;

/// 未选色（跟随主题）时打开自定义取色器的起始色：色板蓝（给出可见的拖动起点）。
pub(crate) fn fallback_picker_color() -> Hsla {
    palette_colors()[4] // Blue 500
}

/// 创建取色器状态：初值为当前颜色，未选色以 [`fallback_picker_color`] 为起点。
pub(crate) fn create_picker(
    window: &mut Window,
    cx: &mut App,
    current: Option<Hsla>,
) -> Entity<ColorPickerState> {
    cx.new(|cx| {
        let mut state = ColorPickerState::new(window, cx)
            .default_value(current.unwrap_or_else(fallback_picker_color));
        // 无 kit ColorPicker 组件代劳渲染，这里自行冲刷初值到滑杆与 hex 输入
        state.sync_pending_value(window, cx);
        state
    })
}

/// 选色内容：`on_select` 处理「无色」与内置色板的选定；`none_label` 是无色
/// 选项的文案（管理页「无色」/ 编辑弹窗「跟随主题」）；`width` 由承载方决定
///（Popover 固定宽度，弹窗内联铺满内容列）。
pub(crate) fn color_picker_content<T: 'static>(
    current: Option<Hsla>,
    picker: &Entity<ColorPickerState>,
    none_label: &'static str,
    on_select: impl Fn(Option<Hsla>, &mut Window, &mut App) + 'static,
    width: Pixels,
    window: &mut Window,
    cx: &mut Context<T>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let on_select: Rc<dyn Fn(Option<Hsla>, &mut Window, &mut App)> = Rc::new(on_select);

    // 无色 / 跟随主题：恢复主题中性样式
    let none_selected = current.is_none();
    let select_none = on_select.clone();
    let none_row = h_flex()
        .id("color-none")
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .rounded(tokens::radius::SM)
        .cursor_pointer()
        .hover(|style| style.bg(theme.secondary))
        .on_click(move |_, window, cx| select_none(None, window, cx))
        .child(
            div()
                .size(px(10.))
                .rounded_full()
                .border_1()
                .border_color(if none_selected {
                    theme.ring
                } else {
                    theme.border
                }),
        )
        .child(div().text_role(TextRole::ControlLabel).child(none_label));

    // 内置 6 色 Material 色板
    let palette = palette_colors();
    let swatch_row = h_flex()
        .flex_wrap()
        .gap_2()
        .children(palette.iter().enumerate().map(|(ix, color)| {
            let color = *color;
            let hex = format_hex_color(color);
            let selected = current.is_some_and(|current| format_hex_color(current) == hex);
            let select = on_select.clone();
            div()
                .id(("color-swatch", ix))
                .size(SWATCH)
                .rounded(tokens::radius::SM)
                .bg(color)
                .border_1()
                .border_color(if selected {
                    theme.ring
                } else {
                    color.darken(0.25)
                })
                .when(selected, |this| this.border_2())
                .cursor_pointer()
                .hover(move |style| style.border_color(color.darken(0.5)))
                .on_click(move |_, window, cx| select(Some(color), window, cx))
        }));

    // 自定义任意取色：hex 输入 + H/S/L 滑杆（kit ColorPickerState 的状态机）
    let picker_state = picker.read(cx);
    let slider_color = picker_state
        .displayed_color()
        .unwrap_or_else(fallback_picker_color);
    let sliders = picker_state.sliders().clone();
    let hex_input = picker_state.hex_input().clone();
    let hex_border = input_border(
        &theme,
        hex_input.read(cx).focus_handle(cx).is_focused(window),
    );

    let hue_track = h_flex()
        .absolute()
        .left_0()
        .right_0()
        .h_2_5()
        .overflow_hidden()
        .children((0..HUE_STEPS).map(|ix| {
            div()
                .flex_1()
                .h_full()
                .bg(hsla(ix as f32 / HUE_STEPS as f32, 1., 0.5, 1.))
        }));
    let saturation_track = gradient_track(
        hsla(slider_color.h, 0., slider_color.l, 1.),
        hsla(slider_color.h, 1., slider_color.l, 1.),
    );
    let lightness_track = gradient_track(
        hsla(slider_color.h, slider_color.s, 0., 1.),
        hsla(slider_color.h, slider_color.s, 1., 1.),
    );

    // hex 输入回车经 Confirm 关闭宿主（Popover / Dialog），拦截后回车仅走
    // ColorPickerState 的提交语义（即时生效、宿主保持打开）
    v_flex()
        .w(width)
        .gap_2()
        .on_action(|_: &gpui_kit::base::actions::Confirm, _, cx| cx.stop_propagation())
        .child(none_row)
        .child(swatch_row)
        .child(Separator::horizontal())
        .child(
            v_flex()
                .gap_2()
                .child(div().text_role(TextRole::Meta).child("自定义"))
                .child(
                    Input::new(&hex_input)
                        .small()
                        .w_full()
                        .text_field_focus(hex_border),
                )
                .child(color_slider("色相", sliders.hue(), hue_track, &theme))
                .child(color_slider(
                    "饱和",
                    sliders.saturation(),
                    saturation_track,
                    &theme,
                ))
                .child(color_slider(
                    "亮度",
                    sliders.lightness(),
                    lightness_track,
                    &theme,
                )),
        )
        .into_any_element()
}

/// 选色弹层内容：Popover 固定宽度承载，供笔记页标签编辑弹窗使用。
pub(crate) fn color_popover_content(
    current: Option<Hsla>,
    picker: &Entity<ColorPickerState>,
    none_label: &'static str,
    on_select: impl Fn(Option<Hsla>, &mut Window, &mut App) + 'static,
    window: &mut Window,
    cx: &mut Context<PopoverState>,
) -> AnyElement {
    color_picker_content(
        current,
        picker,
        none_label,
        on_select,
        POPOVER_WIDTH,
        window,
        cx,
    )
}

/// 单条 HSL 滑杆：左侧角色化标签 + 彩色轨道（kit 滑杆本体透明叠加其上）。
fn color_slider(
    label: &'static str,
    state: &Entity<gpui_kit::component::slider::SliderState>,
    track: Div,
    theme: &Theme,
) -> impl IntoElement {
    h_flex()
        .gap_2()
        .items_center()
        .child(
            div()
                .w(px(28.))
                .flex_shrink_0()
                .text_role(TextRole::Meta)
                .child(label),
        )
        .child(
            div()
                .relative()
                .flex_1()
                .flex()
                .items_center()
                .h_6()
                .child(track)
                .child(Slider::new(state).flex_1().bg(theme.transparent)),
        )
}

/// 两端渐变轨道（饱和度 / 亮度用）。
fn gradient_track(start: Hsla, end: Hsla) -> Div {
    div()
        .absolute()
        .left_0()
        .right_0()
        .h_2_5()
        .overflow_hidden()
        .bg(linear_gradient(
            90.,
            linear_color_stop(start, 0.),
            linear_color_stop(end, 1.),
        ))
}

/// 取色 Popover 的矩形色块触发器：收起时显示当前颜色，未选色显示主题
/// 中性面；hover 以底色提示可点，弹层打开时描边强调。标签管理页与
/// 笔记页的标签编辑弹窗共用。
#[derive(IntoElement)]
pub(crate) struct ColorSwatchTrigger {
    id: ElementId,
    color: Option<Hsla>,
    selected: bool,
}

impl ColorSwatchTrigger {
    pub(crate) fn new(id: impl Into<ElementId>, color: Option<Hsla>) -> Self {
        Self {
            id: id.into(),
            color,
            selected: false,
        }
    }
}

impl Selectable for ColorSwatchTrigger {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl RenderOnce for ColorSwatchTrigger {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .id(self.id)
            .p(px(2.))
            .rounded(tokens::radius::SM)
            .cursor_pointer()
            .hover(|style| style.bg(theme.secondary))
            .child(
                div()
                    .w(TRIGGER_WIDTH)
                    .h(TRIGGER_HEIGHT)
                    .rounded(tokens::radius::SM)
                    .map(|this| match self.color {
                        Some(color) => this.bg(color).border_1().border_color(color.darken(0.25)),
                        None => this
                            .bg(theme.secondary)
                            .border_1()
                            .border_color(theme.border),
                    })
                    .when(self.selected, |this| this.border_color(theme.ring)),
            )
    }
}
