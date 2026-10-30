//! 显示器几何辅助：按点反查所在显示器、把窗口左上角钳制在显示器内。
//!
//! 窗口 bounds 与显示器 bounds 同为全局逻辑坐标；跨显示器打开窗口时须
//! 同时指定目标显示器 id，后端才会按该显示器 scale 换算坐标（§9 条目 5）。

use gpui_kit::{App, Pixels, Point};

/// 显示器逻辑 bounds：(x, y, w, h) 与 id。
pub(crate) type DisplayBounds = ((f32, f32, f32, f32), u64);

/// 反查包含指定点（全局逻辑坐标）的显示器；点不在任何显示器上返回 `None`。
pub(crate) fn display_containing(cx: &App, p: Point<Pixels>) -> Option<DisplayBounds> {
    cx.displays().iter().find_map(|display| {
        let b = display.bounds();
        let inside = p.x >= b.origin.x
            && p.y >= b.origin.y
            && p.x <= b.origin.x + b.size.width
            && p.y <= b.origin.y + b.size.height;
        inside.then(|| {
            (
                (
                    b.origin.x.as_f32(),
                    b.origin.y.as_f32(),
                    b.size.width.as_f32(),
                    b.size.height.as_f32(),
                ),
                u64::from(display.id()),
            )
        })
    })
}

/// 窗口左上角钳制在显示器内（完整可见；显示器小于窗口时贴左上角）。
/// `w` / `h` 为窗口逻辑尺寸。
pub(crate) fn clamp_to_display(
    display: (f32, f32, f32, f32),
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) -> (f32, f32) {
    let (bx, by, bw, bh) = display;
    (
        x.clamp(bx, (bx + bw - w).max(bx)),
        y.clamp(by, (by + bh - h).max(by)),
    )
}
