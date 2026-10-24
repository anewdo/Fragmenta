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
pub fn window_hwnd(window: &mut Window) -> Option<isize> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let handle = window.window_handle().ok()?;
    if let RawWindowHandle::Win32(win32) = handle.as_raw() {
        Some(win32.hwnd.get())
    } else {
        None
    }
}

/// 主窗口关闭后请求 Windows 回收当前进程暂驻工作集。
pub fn trim_working_set() {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, SetProcessWorkingSetSize};

        let process = GetCurrentProcess();
        let _ = SetProcessWorkingSetSize(process, usize::MAX, usize::MAX);
    }
}

/// 系统目录选择对话框（COM `IFileOpenDialog` + `FOS_PICKFOLDERS`，ticket 06）。
/// 用户取消或失败返回 None（设置页保持原输入）。
///
/// windows-sys 0.61 不再生成 COM 接口定义，此处按 SDK 头文件
/// （shobjidl_core.h，经 windows-0.61.3 生成源核对）以槽位直呼 vtable：
/// 槽位 0-2 = IUnknown（QI / AddRef / Release），3 = IModalWindow::Show，
/// 9 = IFileDialog::SetOptions，10 = GetOptions，20 = GetResult；
/// IShellItem 槽位 5 = GetDisplayName。
pub fn pick_folder(owner_hwnd: Option<isize>) -> Option<std::path::PathBuf> {
    #[cfg(windows)]
    {
        use std::ffi::c_void;
        use std::path::PathBuf;
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
            CoTaskMemFree, CoUninitialize,
        };
        use windows_sys::Win32::UI::Shell::{FOS_PICKFOLDERS, FileOpenDialog, SIGDN_FILESYSPATH};
        use windows_sys::core::{GUID, HRESULT};

        const IID_IFILE_OPEN_DIALOG: GUID = GUID::from_u128(0xd57c7288_d4ad_4768_be02_9d969532d960);
        const S_OK: HRESULT = 0;
        const S_FALSE: HRESULT = 1;

        type ShowFn = unsafe extern "system" fn(*mut c_void, HWND) -> HRESULT;
        type SetOptionsFn = unsafe extern "system" fn(*mut c_void, u32) -> HRESULT;
        type GetOptionsFn = unsafe extern "system" fn(*mut c_void, *mut u32) -> HRESULT;
        type GetResultFn = unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT;
        type GetDisplayNameFn =
            unsafe extern "system" fn(*mut c_void, i32, *mut *mut u16) -> HRESULT;
        type ReleaseFn = unsafe extern "system" fn(*mut c_void) -> u32;

        /// 取对象 vtable 第 index 槽位的函数指针。
        unsafe fn slot(this: *mut c_void, index: usize) -> *mut c_void {
            unsafe {
                let vtable = *(this as *const *const *mut c_void);
                *vtable.add(index)
            }
        }

        unsafe fn release(this: *mut c_void) {
            unsafe {
                if !this.is_null() {
                    let release: ReleaseFn = std::mem::transmute(slot(this, 2));
                    release(this);
                }
            }
        }

        unsafe {
            // COM 单元初始化：S_OK = 本次初始化（收尾须配对 CoUninitialize）；
            // S_FALSE = 线程已初始化（gpui 主线程常态），不配对反初始化
            let hr = CoInitializeEx(std::ptr::null_mut(), COINIT_APARTMENTTHREADED as u32);
            if hr < 0 && hr != S_FALSE {
                return None;
            }
            let need_uninit = hr == S_OK;

            let mut dialog: *mut c_void = std::ptr::null_mut();
            let result = (|| {
                if CoCreateInstance(
                    &FileOpenDialog,
                    std::ptr::null_mut(),
                    CLSCTX_INPROC_SERVER,
                    &IID_IFILE_OPEN_DIALOG,
                    &mut dialog,
                ) < 0
                    || dialog.is_null()
                {
                    return None;
                }
                // 在既有选项上追加"仅选目录"
                let get_options: GetOptionsFn = std::mem::transmute(slot(dialog, 10));
                let mut options: u32 = 0;
                get_options(dialog, &mut options);
                let set_options: SetOptionsFn = std::mem::transmute(slot(dialog, 9));
                set_options(dialog, options | FOS_PICKFOLDERS);
                // 模态选择：任何非零返回（用户取消 0x8007007A 或失败）都视为无选择
                let show: ShowFn = std::mem::transmute(slot(dialog, 3));
                let owner = owner_hwnd.map_or(std::ptr::null_mut(), |hwnd| hwnd as HWND);
                if show(dialog, owner) != 0 {
                    return None;
                }
                let get_result: GetResultFn = std::mem::transmute(slot(dialog, 20));
                let mut item: *mut c_void = std::ptr::null_mut();
                if get_result(dialog, &mut item) < 0 || item.is_null() {
                    return None;
                }
                let get_display_name: GetDisplayNameFn = std::mem::transmute(slot(item, 5));
                let mut path: *mut u16 = std::ptr::null_mut();
                if get_display_name(item, SIGDN_FILESYSPATH, &mut path) < 0 || path.is_null() {
                    release(item);
                    return None;
                }
                let mut len = 0usize;
                while *path.add(len) != 0 {
                    len += 1;
                }
                let picked = PathBuf::from(String::from_utf16_lossy(std::slice::from_raw_parts(
                    path, len,
                )));
                CoTaskMemFree(path.cast());
                release(item);
                Some(picked)
            })();

            release(dialog);
            if need_uninit {
                CoUninitialize();
            }
            result
        }
    }
    #[cfg(not(windows))]
    {
        let _ = owner_hwnd;
        None
    }
}

/// 经 shell 打开文件夹（一键打开当前数据目录）：`ShellExecuteW` 的
/// "open" 动词走用户环境注册的文件夹打开关联（第三方文件资源管理器
/// 通常接管 Directory 关联），失败（返回值 ≤ 32）才回退 explorer.exe。
pub fn open_folder(path: &std::path::Path) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        let verb: Vec<u16> = "open\0".encode_utf16().collect();
        let file: Vec<u16> = path
            .display()
            .to_string()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                file.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        if result as usize <= 32 {
            let _ = std::process::Command::new("explorer.exe").arg(path).spawn();
        }
    }
    #[cfg(not(windows))]
    {
        let _ = path;
    }
}

/// 切换窗口置顶（HWND_TOPMOST / HWND_NOTOPMOST），不动位置尺寸。
pub fn set_topmost(window: &mut Window, topmost: bool) -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos,
        };
        let Some(hwnd) = window_hwnd(window) else {
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

        let Some(hwnd) = window_hwnd(window) else {
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

        let Some(hwnd_isize) = window_hwnd(window) else {
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
