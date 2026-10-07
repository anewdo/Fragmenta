//! 全局热键控制器（架构 §5.7）：磁贴呼出热键的注册、事件转发与重绑。
//!
//! 管理器经 `Box::leak` 保活整个进程（析构即注销热键）；事件链路：
//! 独立线程阻塞读 crossbeam channel（过滤 `Pressed`）→ smol channel →
//! gpui 主循环消费 → `windows::tile::open_new`。
//!
//! 本应用经此管理器注册的热键同一时刻至多一个（磁贴呼出键），
//! 通道上的任何 `Pressed` 事件即磁贴呼出信号，rebind 无需同步热键 id。

use std::cell::RefCell;

use global_hotkey::hotkey::{HotKey, HotKeyParseError};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use gpui_kit::{App, Global};

use crate::windows;

/// 热键操作错误：组合键解析失败 / 系统注册失败。
#[derive(Debug)]
pub enum HotkeyError {
    Parse(HotKeyParseError),
    Register(global_hotkey::Error),
}

impl std::fmt::Display for HotkeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HotkeyError::Parse(err) => write!(f, "热键格式无法解析: {err}"),
            HotkeyError::Register(err) => write!(f, "热键注册失败: {err}"),
        }
    }
}

/// 热键控制器：持有保活管理器与当前热键，存于 App 全局。
///
/// 磁贴呼出是唯一写路径；`rebind` 供 Phase 13 设置页改绑，
/// 失败时保留旧热键并报错（组合键被占用场景）。
pub struct HotkeyController {
    /// `Box::leak` 保活的管理器引用，与进程同生命周期。
    manager: &'static GlobalHotKeyManager,
    /// 当前注册的热键；`rebind` 在主线程调用，`RefCell` 即可。
    hotkey: RefCell<HotKey>,
}

impl Global for HotkeyController {}

impl HotkeyController {
    /// 创建控制器：注册磁贴呼出热键并启动事件转发。
    ///
    /// 失败（热键被其他应用占用 / 格式非法）时返回 `Err`，由调用方决定
    /// 降级策略（应用照常启动，磁贴仅失去热键呼出能力）。
    pub fn init(hotkey: &str, cx: &mut App) -> Result<Self, HotkeyError> {
        let manager = Box::leak(Box::new(
            GlobalHotKeyManager::new().map_err(HotkeyError::Register)?,
        ));
        let parsed = hotkey.parse::<HotKey>().map_err(HotkeyError::Parse)?;
        manager.register(parsed).map_err(HotkeyError::Register)?;

        // 独立线程：阻塞读全局热键 channel，Pressed 事件转发到 smol channel
        let (tx, rx) = smol::channel::unbounded::<()>();
        std::thread::spawn(move || {
            let receiver = GlobalHotKeyEvent::receiver();
            while let Ok(event) = receiver.recv() {
                if event.state == HotKeyState::Pressed {
                    let _ = tx.try_send(());
                }
            }
        });

        // gpui 主循环消费热键流：每次按下呼出一个新磁贴
        cx.spawn(async move |cx| {
            while let Ok(()) = rx.recv().await {
                cx.update(windows::tile::open_new);
            }
        })
        .detach();

        Ok(Self {
            manager,
            hotkey: RefCell::new(parsed),
        })
    }

    /// 重绑热键：注销旧 → 注册新；注册失败时回滚并返回 `Err`，旧热键保持可用。
    pub fn rebind(&self, new_hotkey: &str) -> Result<(), HotkeyError> {
        let new = new_hotkey.parse::<HotKey>().map_err(HotkeyError::Parse)?;
        let old = *self.hotkey.borrow();
        if new == old {
            return Ok(());
        }
        self.manager
            .unregister(old)
            .map_err(HotkeyError::Register)?;
        match self.manager.register(new) {
            Ok(()) => {
                *self.hotkey.borrow_mut() = new;
                Ok(())
            }
            Err(err) => {
                // 回滚：旧热键刚被注销，重注册失败意味着热键能力全失，
                // 此情形实际不可达，仍以返回错误告知调用方
                let rollback = self.manager.register(old);
                if rollback.is_err() {
                    eprintln!("[fragmenta] 热键回滚失败，磁贴热键已不可用");
                }
                Err(HotkeyError::Register(err))
            }
        }
    }
}
