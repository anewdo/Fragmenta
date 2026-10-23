//! 开机自启动: HKCU `Software\Microsoft\Windows\CurrentVersion\Run`
//! 键值 "Fragmenta" 的写入与删除。
//!
//! 开启时命令行固定为 `"{exe}" --silent`（带引号的当前 exe 路径 + 静默
//! 参数），每次开启都按当前路径重写，便携 exe 移动后再开启即自动刷新；
//! 关闭时删除该值，值本就不存在视为成功。非 Windows 平台为空实现。

/// 开机自启动操作错误。
#[derive(Debug, thiserror::Error)]
pub enum AutostartError {
    #[error("注册表 Run 键写入失败（代码 {0}）")]
    Write(u32),
    #[error("注册表 Run 键删除失败（代码 {0}）")]
    Delete(u32),
    #[error("无法定位当前可执行文件")]
    ExePath,
}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;

    use windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND;
    use windows_sys::Win32::System::Registry::{
        HKEY_CURRENT_USER, REG_SZ, RegDeleteKeyValueW, RegSetKeyValueW,
    };

    use super::AutostartError;

    /// HKCU 下的 Run 子键路径。
    const RUN_SUBKEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
    /// Run 键内的值名。
    const VALUE_NAME: &str = "Fragmenta";

    /// UTF-16 界定字符串（含结尾 NUL）。
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// 开启自启动：写入 `"{exe}" --silent`（REG_SZ）。每次调用都按当前
    /// exe 路径重写，便携 exe 移动后路径随之刷新。
    pub fn enable() -> Result<(), AutostartError> {
        let exe = std::env::current_exe().map_err(|_| AutostartError::ExePath)?;
        let command = format!("\"{}\" --silent", exe.display());
        let data = wide(&command);
        let subkey = wide(RUN_SUBKEY);
        let name = wide(VALUE_NAME);
        let code = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                name.as_ptr(),
                REG_SZ,
                data.as_ptr().cast::<c_void>(),
                (data.len() * 2) as u32,
            )
        };
        if code == 0 {
            Ok(())
        } else {
            Err(AutostartError::Write(code))
        }
    }

    /// 关闭自启动：删除 Run 值；值不存在（已被外部清除）同样视为成功。
    pub fn disable() -> Result<(), AutostartError> {
        let subkey = wide(RUN_SUBKEY);
        let name = wide(VALUE_NAME);
        let code = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, subkey.as_ptr(), name.as_ptr()) };
        if code == 0 || code == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else {
            Err(AutostartError::Delete(code))
        }
    }
}

#[cfg(windows)]
pub use imp::{disable, enable};

/// 非 Windows：无开机自启动，空实现（编译不破坏）。
#[cfg(not(windows))]
pub fn enable() -> Result<(), AutostartError> {
    Ok(())
}

#[cfg(not(windows))]
pub fn disable() -> Result<(), AutostartError> {
    Ok(())
}
