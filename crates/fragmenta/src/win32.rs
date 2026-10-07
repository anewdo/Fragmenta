//! Windows 原生窗口工具（生产实现，spike ④/⑤ 结论落地）。
//!
//! 光标 DPI 定位（`window_bounds` 语义见架构 §9 条目 5）、置顶切换
//! （PopUp 默认 `WS_EX_TOPMOST`，取消须 `SetWindowPos`）、圆角窗口区域、
//! 八向边缘 resize 的 hit-test 子类化（api-notes §2）。

#[cfg(windows)]
use std::collections::HashMap;
#[cfg(windows)]
use std::sync::{LazyLock, Mutex};

#[cfg(windows)]
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
#[cfg(windows)]
use windows_sys::Win32::UI::WindowsAndMessaging::WNDPROC;

use gpui_kit::Window;

/// 光标所在显示器的定位信息，用于把磁贴落到鼠标附近。
pub struct CursorPlacement {
    /// 光标屏幕逻辑坐标（物理像素 ÷ 所在显示器有效 DPI）。
    pub cursor: (f32, f32),
    /// 所在显示器逻辑 bounds（rcMonitor ÷ 同一 DPI）：(x, y, 宽, 高)。
    pub monitor: (f32, f32, f32, f32),
    /// 所在显示器 id（HMONITOR 的 u64 编码，与 gpui Windows 后端 DisplayId 一致）。
    pub display_id: u64,
}

/// 取光标定位：GetCursorPos → 所在显示器 → 有效 DPI → 逻辑坐标。
///
/// gpui Windows 后端把 `window_bounds` 解释为目标显示器的逻辑坐标，
/// 因此鼠标定位开窗必须按所在显示器 DPI 换算并绑定 `display_id`。
pub fn cursor_placement() -> Option<CursorPlacement> {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::Foundation::POINT;
        use windows_sys::Win32::Graphics::Gdi::{
            GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
        };
        use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
        use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

        let mut pt = POINT { x: 0, y: 0 };
        if GetCursorPos(&mut pt) == 0 {
            return None;
        }
        let monitor = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        if monitor.is_null() {
            return None;
        }
        let mut dpi_x: u32 = 0;
        let mut dpi_y: u32 = 0;
        // GetDpiForMonitor 返回 HRESULT，0 为成功
        if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) != 0 {
            return None;
        }
        let scale = dpi_x as f32 / 96.0;
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return None;
        }
        let rc = info.rcMonitor;
        Some(CursorPlacement {
            cursor: (pt.x as f32 / scale, pt.y as f32 / scale),
            monitor: (
                rc.left as f32 / scale,
                rc.top as f32 / scale,
                (rc.right - rc.left) as f32 / scale,
                (rc.bottom - rc.top) as f32 / scale,
            ),
            display_id: monitor as u64,
        })
    }
    #[cfg(not(windows))]
    None
}

/// 取窗口原生 HWND。
fn hwnd_of(window: &mut Window) -> Option<isize> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let handle = window.window_handle().ok()?;
    if let RawWindowHandle::Win32(win32) = handle.as_raw() {
        Some(win32.hwnd.get())
    } else {
        None
    }
}

/// 切换窗口置顶（HWND_TOPMOST / HWND_NOTOPMOST），不动位置尺寸。
pub fn set_topmost(window: &mut Window, topmost: bool) -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos,
        };
        let Some(hwnd) = hwnd_of(window) else {
            return false;
        };
        let insert_after = if topmost {
            HWND_TOPMOST
        } else {
            HWND_NOTOPMOST
        };
        let result = unsafe {
            SetWindowPos(
                hwnd as _,
                insert_after,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            )
        };
        result != 0
    }
    #[cfg(not(windows))]
    {
        let _ = (window, topmost);
        false
    }
}

/// 把窗口裁为圆角矩形区域（`CreateRoundRectRgn` + `SetWindowRgn`）。
///
/// 无边框 PopUp 窗口原生为直角；圆角半径按逻辑像素传入。几何取
/// `GetClientRect` 物理真值（不走 viewport 换算，杜绝舍入漂移），并经
/// `ScreenToClient` 把客户区原点换算到窗口坐标系（PopUp 无边框时为
/// (0,0)，带边框窗口也正确）。区域所有权移交系统，设置失败时由本函数
/// 负责销毁。尺寸 / scale 变化后调用方须重设（经 `observe_window_bounds`）。
pub fn set_rounded_region(window: &mut Window, radius: f32) -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{POINT, RECT};
        use windows_sys::Win32::Graphics::Gdi::{
            CreateRectRgn, CreateRoundRectRgn, DeleteObject, GetRgnBox, GetWindowRgn,
            ScreenToClient, SetWindowRgn,
        };
        use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetClientRect, GetWindowRect};

        let Some(hwnd) = hwnd_of(window) else {
            return false;
        };
        let mut window_rect: RECT = unsafe { std::mem::zeroed() };
        let mut client_rect: RECT = unsafe { std::mem::zeroed() };
        if unsafe { GetWindowRect(hwnd as _, &mut window_rect) } == 0
            || unsafe { GetClientRect(hwnd as _, &mut client_rect) } == 0
        {
            return false;
        }
        // 客户区原点在窗口坐标系中的位置：窗口左上角的客户坐标取反
        let mut origin = POINT {
            x: window_rect.left,
            y: window_rect.top,
        };
        unsafe { ScreenToClient(hwnd as _, &mut origin) };
        let (ox, oy) = (-origin.x, -origin.y);
        // 半径按窗口 DPI 换算物理像素（与界面层逻辑半径吻合）
        let scale = unsafe { GetDpiForWindow(hwnd as _) } as f32 / 96.;
        let d = (radius * scale).round() as i32 * 2;
        unsafe {
            // GDI 区域右/下边界排他：[ox, ox + w) 恰好覆盖全部客户区
            let hrgn = CreateRoundRectRgn(
                ox,
                oy,
                ox + client_rect.right,
                oy + client_rect.bottom,
                d,
                d,
            );
            if hrgn.is_null() {
                return false;
            }
            let result = SetWindowRgn(hwnd as _, hrgn, 1);
            // 临时诊断：三层尺寸 + 设置结果 + 系统实际持有的区域
            // （定位圆角左右不对称问题，验证后移除）
            let probe = CreateRectRgn(0, 0, 0, 0);
            if !probe.is_null() {
                let complexity = GetWindowRgn(hwnd as _, probe);
                let mut bbox: RECT = std::mem::zeroed();
                GetRgnBox(probe, &mut bbox);
                DeleteObject(probe);
                let viewport = window.viewport_size();
                eprintln!(
                    "[tile-rgn] window=({},{}) {}x{} client_origin=({ox},{oy}) client={}x{} \
                     viewport={:.1}x{:.1} win_scale={:.3} dpi_scale={scale:.3} d={d} \
                     set={result} eff={complexity} bbox=({},{})-({},{})",
                    window_rect.left,
                    window_rect.top,
                    window_rect.right - window_rect.left,
                    window_rect.bottom - window_rect.top,
                    client_rect.right,
                    client_rect.bottom,
                    viewport.width.as_f32(),
                    viewport.height.as_f32(),
                    window.scale_factor(),
                    bbox.left,
                    bbox.top,
                    bbox.right,
                    bbox.bottom,
                );
            }
            if result == 0 {
                // 设置失败时区域所有权未移交，须自行销毁
                DeleteObject(hrgn);
                return false;
            }
            true
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (window, radius);
        false
    }
}

/// 磁贴边缘子类化登记项：原 wndproc（链回 gpui）与热区参数（逻辑像素）。
#[cfg(windows)]
struct EdgeSubclass {
    orig: WNDPROC,
    edge: f32,
    corner: f32,
}

/// 已子类化窗口登记表（HWND → 登记项）；WM_NCDESTROY 时移除。
#[cfg(windows)]
static EDGE_SUBCLASSES: LazyLock<Mutex<HashMap<isize, EdgeSubclass>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 八向边缘命中测试：命中返回 HT\* 码，未命中（含窗口矩形读取失败）返回 None。
///
/// `edge` / `corner` 为逻辑像素热区参数，按窗口当前 DPI 换算为物理像素。
#[cfg(windows)]
unsafe fn edge_hit_test(hwnd: HWND, lparam: LPARAM, edge: f32, corner: f32) -> Option<u32> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowRect, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT,
        HTTOPRIGHT,
    };

    let mut rect: RECT = unsafe { std::mem::zeroed() };
    if unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
        return None;
    }
    // 窗口外的点交回默认处理（命中测试也会被捕获期间等路径问到）
    let x = (lparam & 0xFFFF) as i16 as i32;
    let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;
    if x < rect.left || x > rect.right || y < rect.top || y > rect.bottom {
        return None;
    }
    let scale = unsafe { GetDpiForWindow(hwnd) } as f32 / 96.;
    let edge = (edge * scale).round() as i32;
    let corner = (corner * scale).round() as i32;
    let left = x - rect.left;
    let right = rect.right - x;
    let top = y - rect.top;
    let bottom = rect.bottom - y;
    // 角部优先（角块覆盖边条端点）
    Some(if left <= corner && top <= corner {
        HTTOPLEFT
    } else if right <= corner && top <= corner {
        HTTOPRIGHT
    } else if left <= corner && bottom <= corner {
        HTBOTTOMLEFT
    } else if right <= corner && bottom <= corner {
        HTBOTTOMRIGHT
    } else if left <= edge {
        HTLEFT
    } else if right <= edge {
        HTRIGHT
    } else if top <= edge {
        HTTOP
    } else if bottom <= edge {
        HTBOTTOM
    } else {
        return None;
    })
}

/// 子类 wndproc：WM_NCHITTEST 补齐八向边缘命中，其余消息链回 gpui 原 wndproc。
///
/// 返回 HT\* 后系统接管：按下转为 WM_NCLBUTTONDOWN，gpui 后端对八个边缘码
/// 透传 `DefWindowProcW` 进入原生 resize 模态循环（期间 WM_ENTERSIZEMOVE
/// 的 timer 泵维持绘制），WM_SIZE 走与普通窗口 resize 相同的后端路径，
/// 视口 / 渲染面 / 圆角区域重设全部经既有机制同步闭环。
#[cfg(windows)]
unsafe extern "system" fn tile_edge_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallWindowProcW, DefWindowProcW, WM_NCDESTROY, WM_NCHITTEST,
    };

    // 取参即放锁：后续 CallWindowProcW 链上的模态循环会同步重入本 proc，
    // 持锁等待会死锁
    let key = hwnd as isize;
    let info = EDGE_SUBCLASSES
        .lock()
        .expect("磁贴子类化表锁中毒")
        .get(&key)
        .map(|info| (info.orig, info.edge, info.corner));
    let Some((orig, edge, corner)) = info else {
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    };

    if msg == WM_NCHITTEST
        && let Some(hit) = unsafe { edge_hit_test(hwnd, lparam, edge, corner) }
    {
        return hit as LRESULT;
    }
    if msg == WM_NCDESTROY {
        EDGE_SUBCLASSES
            .lock()
            .expect("磁贴子类化表锁中毒")
            .remove(&key);
    }
    unsafe { CallWindowProcW(orig, hwnd, msg, wparam, lparam) }
}

/// 安装八向边缘 resize 的 hit-test 子类化（每窗口幂等；非 Windows 返回 false）。
///
/// 框架在隐藏标题栏时只把顶边映射为 resize 边（api-notes §2.3），本子类化
/// 把窗口外缘八向命中全部补齐；最小尺寸由 `WindowOptions::window_min_size`
/// 经后端 WM_GETMINMAXINFO 原生钳制，无需应用层参与。`edge` 为边缘热区宽、
/// `corner` 为角部热区边长（逻辑像素）。
pub fn install_edge_subclass(window: &mut Window, edge: f32, corner: f32) -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{GWLP_WNDPROC, SetWindowLongPtrW};

        let Some(hwnd_isize) = hwnd_of(window) else {
            return false;
        };
        let mut map = EDGE_SUBCLASSES.lock().expect("磁贴子类化表锁中毒");
        if map.contains_key(&hwnd_isize) {
            return true;
        }
        let hwnd = hwnd_isize as HWND;
        let proc: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT = tile_edge_proc;
        // prev = gpui 原 wndproc（非空函数指针；0 表示设置失败）
        let prev = unsafe { SetWindowLongPtrW(hwnd, GWLP_WNDPROC, proc as usize as isize) };
        if prev == 0 {
            return false;
        }
        map.insert(
            hwnd_isize,
            EdgeSubclass {
                orig: unsafe { std::mem::transmute::<isize, WNDPROC>(prev) },
                edge,
                corner,
            },
        );
        true
    }
    #[cfg(not(windows))]
    {
        let _ = (window, edge, corner);
        false
    }
}
