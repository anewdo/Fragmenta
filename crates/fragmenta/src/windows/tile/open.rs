//! 磁贴窗口打开与级联定位（§5.8 弹出行为）。
//!
//! 场上磁贴经弱引用登记表追踪：存活探测（窗口关闭 → 视图实体释放 →
//! 不可升级）收敛已关磁贴，全部关闭后下一次呼出自然回到首开定位。
//! 级联取创建时机最新的现存磁贴的**当前**位置（`window.bounds()`），
//! 磁贴被挪动后新磁贴跟随其现位置偏移。

use std::sync::Mutex;

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{CASCADE, MIN_H, MIN_W, TILE_H, TILE_W, view::TileView};

/// 场上磁贴登记项：弱引用探测存活；窗口句柄供级联定位取当前位置。
struct TileEntry {
    view: gpui_kit::WeakEntity<TileView>,
    window: gpui_kit::AnyWindowHandle,
}

static TILES: Mutex<Vec<TileEntry>> = Mutex::new(Vec::new());

/// 打开新磁贴：每次呼出新开一个窗口，旧磁贴保留（D7 磁贴多开）。
pub fn open_new(cx: &mut App) {
    let mut tiles = TILES.lock().expect("磁贴登记表锁中毒");
    // 清理已关磁贴；场上清空后下一次呼出自然回到首开定位（§5.8 重置规则）
    tiles.retain(|tile| tile.view.is_upgradable());

    // 级联参照：创建时机最新的现存磁贴的当前位置（被挪动后跟随），
    // 并反查其所在显示器，级联沿用同一坐标空间做钳制
    let cascade_from = tiles
        .last()
        .and_then(|tile| tile.window.update(cx, |_, window, _| window.bounds()).ok())
        .and_then(|b| {
            let center = point(
                b.origin.x + b.size.width * 0.5,
                b.origin.y + b.size.height * 0.5,
            );
            monitor_containing(cx, center).map(|(monitor, display_id)| {
                (
                    b.origin.x.as_f32(),
                    b.origin.y.as_f32(),
                    monitor,
                    display_id,
                )
            })
        });

    // 场上已有磁贴：相对其当前位置级联偏移；场上没有：窗口中心对准光标。
    // 空间不足时钳制，保证完整落在显示器内
    let cursor = crate::win32::cursor_placement();
    let (origin, display_id) = if let Some((x, y, monitor, display_id)) = cascade_from {
        (
            clamp_to_monitor(monitor, x + CASCADE, y + CASCADE),
            display_id,
        )
    } else if let Some(p) = cursor.as_ref() {
        (
            clamp_to_monitor(
                p.monitor,
                p.cursor.0 - TILE_W * 0.5,
                p.cursor.1 - TILE_H * 0.5,
            ),
            p.display_id,
        )
    } else {
        // 光标定位不可用：回退主显示器居中
        let bounds = Bounds::centered(None, size(px(TILE_W), px(TILE_H)), cx);
        let display_id = cx.primary_display().map(|d| u64::from(d.id())).unwrap_or(0);
        (
            (bounds.origin.x.as_f32(), bounds.origin.y.as_f32()),
            display_id,
        )
    };

    let bounds = Bounds {
        origin: point(px(origin.0), px(origin.1)),
        size: size(px(TILE_W), px(TILE_H)),
    };
    // display_id 绑定目标显示器：gpui 后端按目标显示器 scale 换算 bounds
    // 并校验中心点，缺失该 id 时窗口回退到主显示器默认居中（§9 条目 5）
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        display_id: Some(DisplayId::new(display_id)),
        titlebar: None,
        kind: WindowKind::PopUp,
        // 最小尺寸：后端 WM_GETMINMAXINFO 原生钳制（api-notes §2 配套）
        window_min_size: Some(size(px(MIN_W), px(MIN_H))),
        ..Default::default()
    };
    match gpui_kit::open_window(options, cx, |window, cx| {
        cx.new(|cx| TileView::new(window, cx))
    }) {
        Ok((window, view)) => tiles.push(TileEntry {
            view: view.downgrade(),
            window,
        }),
        Err(err) => eprintln!("[fragmenta] 打开磁贴失败: {err:?}"),
    }
}

/// 反查包含指定点（全局逻辑坐标）的显示器：返回其逻辑 bounds 与 id。
///
/// 级联定位与光标定位同样须绑定所在显示器 id（§9 条目 5）。
fn monitor_containing(cx: &App, p: Point<Pixels>) -> Option<((f32, f32, f32, f32), u64)> {
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
fn clamp_to_monitor(monitor: (f32, f32, f32, f32), x: f32, y: f32) -> (f32, f32) {
    let (bx, by, bw, bh) = monitor;
    (
        x.clamp(bx, (bx + bw - TILE_W).max(bx)),
        y.clamp(by, (by + bh - TILE_H).max(by)),
    )
}
