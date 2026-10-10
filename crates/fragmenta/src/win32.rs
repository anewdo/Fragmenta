//! Windows 原生窗口工具（生产实现，spike ④/⑤ 结论落地）。
//!
//! 光标 DPI 定位（`window_bounds` 语义见架构 §9 条目 5）、置顶切换
//! （PopUp 默认 `WS_EX_TOPMOST`，取消须 `SetWindowPos`）、圆角窗口区域、
//! 鼠标拖拽边缘缩放（接管窗口消息处理实现，见 [`install_edge_subclass`]）。

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
/// `ScreenToClient` 把客户区原点换算到窗口坐标系，沿可见客户区裁剪。
/// 区域所有权移交系统，设置失败时由本函数
/// 负责销毁。尺寸 / scale 变化后调用方须重设（经 `observe_window_bounds`）。
pub fn set_rounded_region(window: &mut Window, radius: f32) -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{POINT, RECT};
        use windows_sys::Win32::Graphics::Gdi::{
            CreateRoundRectRgn, DeleteObject, ScreenToClient, SetWindowRgn,
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
            // CreateRoundRectRgn 的右/下端点须加 1，才能保留客户区最后一列与一行。
            let hrgn = CreateRoundRectRgn(
                ox,
                oy,
                ox + client_rect.right + 1,
                oy + client_rect.bottom + 1,
                d,
                d,
            );
            if hrgn.is_null() {
                return false;
            }
            let result = SetWindowRgn(hwnd as _, hrgn, 1);
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

/// 边缘感应登记项：`orig` 为 gpui 原本的窗口消息处理函数（未被拦截的
/// 消息转发回它），`edge` / `corner` 为感应区尺寸（逻辑像素）。
#[cfg(windows)]
struct EdgeSubclass {
    orig: WNDPROC,
    edge: f32,
    corner: f32,
}

/// 已接管消息处理的窗口登记表（HWND → 登记项）；窗口销毁（WM_NCDESTROY）
/// 时移除，防止句柄被系统复用后误用已失效的旧函数指针。
#[cfg(windows)]
static EDGE_SUBCLASSES: LazyLock<Mutex<HashMap<isize, EdgeSubclass>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 八向边缘命中测试：光标落在感应区内返回对应 `HT*` 部位码，否则（含窗口
/// 矩形读取失败）返回 None，交回默认处理。
///
/// `edge` 为四边感应宽度、`corner` 为四角感应边长（逻辑像素），按窗口当前
/// DPI 换算为物理像素后，在圆角裁剪使用的客户区内比较。
#[cfg(windows)]
unsafe fn edge_hit_test(hwnd: HWND, lparam: LPARAM, edge: f32, corner: f32) -> Option<u32> {
    use windows_sys::Win32::Foundation::{POINT, RECT};
    use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetClientRect, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT,
        HTTOPRIGHT,
    };

    let mut rect: RECT = unsafe { std::mem::zeroed() };
    let mut cursor = POINT {
        x: (lparam & 0xFFFF) as i16 as i32,
        y: ((lparam >> 16) & 0xFFFF) as i16 as i32,
    };
    // 圆角区域沿客户区裁剪，感应条也沿同一范围定位。
    if unsafe { GetClientRect(hwnd, &mut rect) } == 0
        || unsafe { ScreenToClient(hwnd, &mut cursor) } == 0
    {
        return None;
    }
    // 客户区外的点交回默认处理（命中测试也会被捕获期间等路径问到）
    let (x, y) = (cursor.x, cursor.y);
    if x < rect.left || x >= rect.right || y < rect.top || y >= rect.bottom {
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

/// 接管后的窗口消息处理函数：八向边缘命中、移动与按下交由系统处理，
/// 内容区和标题栏拖拽消息转发给 gpui 原处理函数。
///
/// 返回部位码之后的缩放流程见 [`install_edge_subclass`] 注释。
#[cfg(windows)]
unsafe extern "system" fn tile_edge_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallWindowProcW, DefWindowProcW, HTBOTTOMRIGHT, HTLEFT, WM_NCDESTROY, WM_NCHITTEST,
        WM_NCLBUTTONDOWN, WM_NCMOUSEMOVE,
    };

    // 查表后立即放锁：缩放循环进行期间，系统会在同一线程上再次进入本函数
    // 处理后续消息；若一直持锁，重入的那次会等待锁释放而卡死
    let key = hwnd as isize;
    let info = EDGE_SUBCLASSES
        .lock()
        .expect("边缘感应登记表锁状态异常")
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
    // gpui 会把非客户区鼠标消息转为内容输入；边缘移动与按下须交给系统，
    // 保持缩放光标，并让输入框旁的边缘也能进入原生缩放循环。
    if matches!(msg, WM_NCMOUSEMOVE | WM_NCLBUTTONDOWN)
        && (HTLEFT..=HTBOTTOMRIGHT).contains(&(wparam as u32))
    {
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    }
    if msg == WM_NCDESTROY {
        EDGE_SUBCLASSES
            .lock()
            .expect("边缘感应登记表锁状态异常")
            .remove(&key);
    }
    unsafe { CallWindowProcW(orig, hwnd, msg, wparam, lparam) }
}

/// 启用鼠标拖拽边缘缩放（每窗口幂等；非 Windows 返回 false）。
///
/// 感应范围沿圆角裁剪使用的可见客户区定位，覆盖四边与四角。
/// `edge` 为四边感应宽度，`corner` 为四角感应边长（逻辑像素）。
///
/// 拖拽缩放的完整流程：
/// 1. 本函数把窗口消息处理函数替换为 [`tile_edge_proc`]，gpui 原处理
///    函数存入登记表，未被拦截的消息由它转发回去；
/// 2. 鼠标移动时系统先发 `WM_NCHITTEST` 询问"光标处算窗口哪个部位"，
///    `tile_edge_proc` 对外缘八向命中返回对应 `HT*` 部位码（四角优先）；
/// 3. 在感应区内按下鼠标，系统按部位码发 `WM_NCLBUTTONDOWN`，拦截 gpui 后端
///    的边缘部位码处理，直接交回系统默认逻辑；
/// 4. 系统进入原生拖拽缩放循环：缩放光标与画面刷新全部由系统与 gpui
///    既有机制托管（后端在循环期间用定时器持续绘制），松开鼠标即结束；
/// 5. 循环中的每次尺寸变化经 `WM_SIZE` 走 gpui 与普通窗口相同的同步
///    路径，视口、渲染表面、圆角区域（`observe_window_bounds`）自动跟随；
/// 6. 最小尺寸由 `WindowOptions::window_min_size` 声明，系统在循环中经
///    `WM_GETMINMAXINFO` 直接钳制，应用层无需参与。
pub fn install_edge_subclass(window: &mut Window, edge: f32, corner: f32) -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{GWLP_WNDPROC, SetWindowLongPtrW};

        let Some(hwnd_isize) = hwnd_of(window) else {
            return false;
        };
        let mut map = EDGE_SUBCLASSES.lock().expect("边缘感应登记表锁状态异常");
        if map.contains_key(&hwnd_isize) {
            return true;
        }
        let hwnd = hwnd_isize as HWND;
        let proc: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT = tile_edge_proc;
        // 返回值为 gpui 原消息处理函数（0 表示替换失败）
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
