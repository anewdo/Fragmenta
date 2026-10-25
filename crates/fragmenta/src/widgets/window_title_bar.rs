//! 主窗口自绘标题栏：左侧内容 + 最小化 / 最大化 / 关闭三键。
//!
//! kit `TitleBar` 的控制按钮宽度固定为 34px 且不随条带高度变化，命中区偏小；
//! 这里沿用其窗口选项（透明系统标题栏、原生命中区）与平台语义，只重绘条带，
//! 尺寸取自 tokens。

use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Icon, InteractiveElementExt as _, Sizable as _, h_flex,
};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::ui::tokens;

#[cfg(target_os = "macos")]
const LEFT_PADDING: Pixels = px(80.);
#[cfg(not(target_os = "macos"))]
const LEFT_PADDING: Pixels = px(12.);

/// 顶部通栏标题栏；`content` 位于左侧拖拽区，窗口控制按钮固定最右。
pub fn window_title_bar(content: impl IntoElement, window: &Window, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let is_linux = cfg!(target_os = "linux");
    let is_macos = cfg!(target_os = "macos");

    h_flex()
        .id("title-bar")
        .w_full()
        .h(tokens::heights::TITLE_BAR)
        .flex_shrink_0()
        .items_center()
        .justify_between()
        .pl(LEFT_PADDING)
        .border_b_1()
        .border_color(theme.title_bar_border)
        .bg(if theme.mode.is_dark() {
            title_bar_background(theme.title_bar, theme.background)
        } else {
            theme.title_bar.into()
        })
        .when(is_linux, |this| {
            this.on_double_click(|_, window, _| window.zoom_window())
        })
        .when(is_macos, |this| {
            this.on_double_click(|_, window, _| window.titlebar_double_click())
        })
        .child(
            h_flex()
                .id("title-bar-drag")
                .flex_1()
                .h_full()
                .items_center()
                .window_control_area(WindowControlArea::Drag)
                .child(content),
        )
        .child(window_controls(window, cx))
}

/// 与 kit `TitleBar` 一致的条带背景：顶端偏背景色、底端为标题栏色。
fn title_bar_background(title_bar: Hsla, background: Hsla) -> Background {
    let title_bar_rgb = title_bar.to_rgb();
    let background_rgb = background.to_rgb();
    let mixed = Hsla::from(Rgba {
        r: title_bar_rgb.r * 0.55 + background_rgb.r * 0.45,
        g: title_bar_rgb.g * 0.55 + background_rgb.g * 0.45,
        b: title_bar_rgb.b * 0.55 + background_rgb.b * 0.45,
        a: title_bar_rgb.a * 0.55 + background_rgb.a * 0.45,
    });

    linear_gradient(
        180.,
        linear_color_stop(mixed, 0.),
        linear_color_stop(title_bar, 1.),
    )
}

fn window_controls(window: &Window, cx: &App) -> AnyElement {
    if cfg!(any(target_os = "macos", target_family = "wasm")) {
        return div().into_any_element();
    }
    // 服务端装饰下窗口管理器已绘制自己的三键，重复绘制会叠出两套控制按钮。
    #[cfg(target_os = "linux")]
    if !matches!(window.window_decorations(), Decorations::Client { .. }) {
        return div().into_any_element();
    }

    let supported = window.window_controls();
    let maximize_icon = if window.is_maximized() {
        IconName::WindowRestore
    } else {
        IconName::WindowMaximize
    };

    h_flex()
        .items_center()
        .flex_shrink_0()
        .h_full()
        .when(supported.minimize, |this| {
            this.child(control_button(
                "window-minimize",
                IconName::WindowMinimize,
                WindowControlArea::Min,
                false,
                cx,
            ))
        })
        .when(supported.maximize, |this| {
            this.child(control_button(
                "window-maximize",
                maximize_icon,
                WindowControlArea::Max,
                false,
                cx,
            ))
        })
        .child(control_button(
            "window-close",
            IconName::WindowClose,
            WindowControlArea::Close,
            true,
            cx,
        ))
        .into_any_element()
}

fn control_button(
    id: &'static str,
    icon: IconName,
    area: WindowControlArea,
    is_close: bool,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let (hover_fg, hover_bg, active_bg) = if is_close {
        (theme.danger_foreground, theme.danger, theme.danger_active)
    } else {
        (
            theme.secondary_foreground,
            theme.secondary_hover,
            theme.secondary_active,
        )
    };

    let mut button = div()
        .id(id)
        .flex()
        .w(tokens::widths::WINDOW_CONTROL)
        .h_full()
        .flex_shrink_0()
        .justify_center()
        .items_center()
        .text_color(theme.foreground)
        .hover(move |style| style.bg(hover_bg).text_color(hover_fg))
        .active(move |style| style.bg(active_bg).text_color(hover_fg))
        .child(Icon::new(icon).small());

    // Windows 交给系统做 WM_NCHITTEST，拖动 / 双击 / 三键都由原生路径处理。
    if cfg!(target_os = "windows") {
        button = button.window_control_area(area);
    }
    if cfg!(target_os = "linux") {
        button = button
            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
            })
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                match area {
                    WindowControlArea::Min => window.minimize_window(),
                    WindowControlArea::Max => window.zoom_window(),
                    WindowControlArea::Close => window.remove_window(),
                    WindowControlArea::Drag => {}
                }
            });
    }

    button.into_any_element()
}
