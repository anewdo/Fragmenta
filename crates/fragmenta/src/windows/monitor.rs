//! 显示器几何辅助：按点反查所在显示器、把窗口左上角钳制在显示器内，
//! 以及把窗口 bounds 校验为可持久化的位置。
//!
//! 窗口 bounds 与显示器 bounds 同为全局逻辑坐标；跨显示器打开窗口时须
//! 同时指定目标显示器 id，后端才会按该显示器 scale 换算坐标（§9 条目 5）。

use gpui_kit::{App, Pixels, Point, WindowBounds};

/// 显示器逻辑 bounds：(x, y, w, h) 与 id。
pub(crate) type DisplayBounds = ((f32, f32, f32, f32), u64);

/// 从 `WindowBounds` 提取可持久化的窗口左上角。
///
/// 最大化 / 全屏不是用户摆放的位置，跳过以保留还原后的位置。更关键的是
/// Windows 后端在窗口销毁阶段仍会触发一次 bounds 观察：此时
/// `GetWindowPlacement` 已失败，`calculate_window_bounds` 拿全 0 的
/// `WINDOWPLACEMENT` 继续换算，产出「origin 只剩边框偏移、size 为 0 或负」
/// 的垃圾 bounds（125% DPI 下典型值 origin ≈ (7.2, 3.2)）。若不过滤，
/// 这个近零点会覆盖上次关闭位置，使下次呼出永远贴在屏幕左上角。
pub(crate) fn persistable_origin(bounds: WindowBounds) -> Option<Point<Pixels>> {
    let WindowBounds::Windowed(bounds) = bounds else {
        return None;
    };
    let finite = |v: Pixels| v.as_f32().is_finite();
    if !finite(bounds.origin.x)
        || !finite(bounds.origin.y)
        || !finite(bounds.size.width)
        || !finite(bounds.size.height)
        || bounds.size.width.as_f32() <= 0.
        || bounds.size.height.as_f32() <= 0.
    {
        return None;
    }
    Some(bounds.origin)
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{Bounds, point, px, size};

    fn windowed(origin: (f32, f32), dimensions: (f32, f32)) -> WindowBounds {
        WindowBounds::Windowed(Bounds {
            origin: point(px(origin.0), px(origin.1)),
            size: size(px(dimensions.0), px(dimensions.1)),
        })
    }

    /// 正常窗口：客户区左上角原样提交（与开窗时写入的坐标系一致）。
    #[test]
    fn windowed_bounds_yield_origin() {
        assert_eq!(
            persistable_origin(windowed((420., 144.), (1080., 720.))),
            Some(point(px(420.), px(144.)))
        );
    }

    /// 最大化 / 全屏跳过；销毁期 `GetWindowPlacement` 失败产出的
    /// 近零 origin + 非正尺寸必须拒绝，否则关闭位置会被覆盖成左上角。
    #[test]
    fn rejects_non_windowed_and_destroyed_window_bounds() {
        let bounds = windowed((420., 144.), (1080., 720.)).get_bounds();
        assert_eq!(persistable_origin(WindowBounds::Maximized(bounds)), None);
        assert_eq!(persistable_origin(WindowBounds::Fullscreen(bounds)), None);
        // 125% DPI 下真实观测到的垃圾值：(7.2, 3.2) / 负尺寸
        assert_eq!(
            persistable_origin(windowed((7.2, 3.2), (-14.4, -6.4))),
            None
        );
        assert_eq!(persistable_origin(windowed((7.2, 3.2), (0., 0.))), None);
        assert_eq!(
            persistable_origin(windowed((f32::NAN, 3.2), (1080., 720.))),
            None
        );
    }
}
