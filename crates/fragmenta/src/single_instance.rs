//! 单实例守护（Windows）

#[cfg(windows)]
mod imp {
    use gpui_kit::App;
    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, WAIT_OBJECT_0,
    };
    use windows_sys::Win32::System::Threading::{
        CreateEventW, CreateMutexW, INFINITE, SetEvent, WaitForSingleObject,
    };

    use crate::windows;

    /// 首个实例的守护对象：持有互斥体与激活事件句柄。
    ///
    /// 内核句柄在进程生命周期内保持打开，故不实现 `Send`；跨线程只传递句柄数值
    pub struct Instance {
        mutex: HANDLE,
        event: HANDLE,
    }

    impl Instance {
        /// 激活事件句柄数值，供监听线程等待。
        pub fn event_handle(&self) -> isize {
            self.event as isize
        }
    }

    impl Drop for Instance {
        fn drop(&mut self) {
            // 激活事件由监听线程持续等待，主线程提前关闭会让等待方行为未定义；
            // 该句柄随进程退出一并回收。互斥体在此关闭，让出单实例身份。
            if !self.mutex.is_null() {
                unsafe { CloseHandle(self.mutex) };
            }
        }
    }

    /// 判定本次启动的身份；`open_main_window` 表示本次启动要见界面
    /// （静默启动传 `false`）。
    ///
    /// 返回 `Some`：本进程是首个实例。返回 `None`：已有实例在运行，
    /// 已按其意图转交激活请求，调用方应立即结束本进程。
    pub fn claim(open_main_window: bool) -> Option<Instance> {
        let (mutex_name, event_name) = instance_names();
        // 事件先于互斥体创建，保证"互斥体已被占用"时事件必然已存在
        let event = unsafe { CreateEventW(std::ptr::null(), 0, 0, wide(&event_name).as_ptr()) };
        let mutex = unsafe { CreateMutexW(std::ptr::null(), 0, wide(&mutex_name).as_ptr()) };
        let occupied = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        if mutex.is_null() || !occupied {
            // 互斥体创建失败（资源耗尽等）时按首个实例继续运行，守护降级
            return Some(Instance { mutex, event });
        }
        // 已有实例：按本次启动意图唤其打开主窗口；句柄随本进程结束回收
        if open_main_window && !event.is_null() {
            unsafe { SetEvent(event) };
        }
        unsafe {
            CloseHandle(mutex);
            if !event.is_null() {
                CloseHandle(event);
            }
        }
        None
    }

    /// 起激活监听：阻塞等待线程 + gpui 主循环消费。
    ///
    /// 事件置位后经 smol channel 汇入主循环，统一走
    /// [`windows::main::open`]（已开则前置，未开则新建）。
    pub fn listen(event: isize, cx: &mut App) {
        if event == 0 {
            return;
        }
        let (tx, rx) = smol::channel::unbounded::<()>();
        std::thread::spawn(move || {
            loop {
                let wait = unsafe { WaitForSingleObject(event as HANDLE, INFINITE) };
                if wait != WAIT_OBJECT_0 || tx.try_send(()).is_err() {
                    break;
                }
            }
        });
        cx.spawn(async move |cx| {
            while let Ok(()) = rx.recv().await {
                cx.update(windows::main::open);
            }
        })
        .detach();
    }

    /// 命名对象名（互斥体、激活事件），按 exe 路径区分实例归属。
    ///
    /// 路径先规范化（消解 8.3 短名等别名）再小写，Windows 路径大小写
    /// 不敏感，同一文件的不同写法必须得到同一组名字。
    fn instance_names() -> (String, String) {
        let exe = std::env::current_exe()
            .ok()
            .map(|path| std::fs::canonicalize(&path).unwrap_or(path))
            .unwrap_or_default();
        let key = fnv1a(exe.to_string_lossy().to_lowercase().as_bytes());
        (
            format!("Local\\Fragmenta.SingleInstance.{key:016x}"),
            format!("Local\\Fragmenta.Activate.{key:016x}"),
        )
    }

    /// FNV-1a 64 位：跨进程稳定的路径摘要。
    fn fnv1a(bytes: &[u8]) -> u64 {
        bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
    }

    /// UTF-16 界定字符串（含结尾 NUL）。
    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

#[cfg(windows)]
pub use imp::{Instance, claim, listen};

/// 非 Windows：无单实例守护，恒为首个实例。
#[cfg(not(windows))]
pub struct Instance;

#[cfg(not(windows))]
impl Instance {
    /// 无激活事件，返回 0（[`listen`] 直接返回）。
    pub fn event_handle(&self) -> isize {
        0
    }
}

/// 非 Windows：恒为首个实例。
#[cfg(not(windows))]
pub fn claim(_open_main_window: bool) -> Option<Instance> {
    Some(Instance)
}

/// 非 Windows：无激活监听，空实现。
#[cfg(not(windows))]
pub fn listen(_event: isize, _cx: &mut gpui_kit::App) {}
