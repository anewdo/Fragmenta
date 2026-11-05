//! 热键管理

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
/// 磁贴呼出是唯一写路径；`rebind` 供设置页改绑，失败时保留旧热键并报错
/// （组合键被占用场景）。启动期注册失败同样保留控制器（冲突记为状态），
/// 设置页据此展示红字提示，改绑成功即恢复生效，无需重启。
pub struct HotkeyController {
    /// `Box::leak` 保活的管理器引用，与进程同生命周期。
    manager: &'static GlobalHotKeyManager,
    /// 当前已向系统注册的热键；启动期注册失败时为 `None`。
    /// `rebind` 在主线程调用，`RefCell` 即可。
    hotkey: RefCell<Option<HotKey>>,
    /// 启动期配置热键的注册失败原因；存在即表示设置中的热键当前不可用。
    conflict: RefCell<Option<String>>,
}

impl Global for HotkeyController {}

impl HotkeyController {
    /// 创建控制器：向系统注册磁贴呼出热键并启动事件转发。
    ///
    /// 仅管理器创建失败时返回 `Err`（热键能力整体缺席）；配置热键本身
    /// 注册失败（被其他应用占用 / 格式非法）不阻止控制器建立，失败原因
    /// 记为冲突状态，由 [`Self::conflict`] 暴露给设置页提示。
    pub fn init(hotkey: &str, cx: &mut App) -> Result<Self, HotkeyError> {
        let manager = Box::leak(Box::new(
            GlobalHotKeyManager::new().map_err(HotkeyError::Register)?,
        ));

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

        // 配置热键注册失败时记入冲突状态；事件转发线程独立于注册结果，
        // 改绑成功后沿用同一通道
        let (hotkey, conflict) = match hotkey.parse::<HotKey>() {
            Ok(parsed) => match manager.register(parsed) {
                Ok(()) => (Some(parsed), None),
                Err(err) => (None, Some(HotkeyError::Register(err))),
            },
            Err(err) => (None, Some(HotkeyError::Parse(err))),
        };
        Ok(Self {
            manager,
            hotkey: RefCell::new(hotkey),
            conflict: RefCell::new(conflict.map(|err| err.to_string())),
        })
    }

    /// 当前设置的热键是否未能注册（启动期被其他程序占用 / 格式非法）。
    pub fn conflict(&self) -> bool {
        self.conflict.borrow().is_some()
    }

    /// 启动期注册失败原因（供启动日志输出）；无冲突时为 `None`。
    pub fn conflict_reason(&self) -> Option<String> {
        self.conflict.borrow().clone()
    }

    /// 重绑热键：先注册新 → 成功后注销旧；新键注册失败（被其他程序占用）
    /// 时旧热键未被触碰，保持可用。启动期无已注册热键（冲突状态）时直接
    /// 注册新键，成功后冲突状态清除。
    pub fn rebind(&self, new_hotkey: &str) -> Result<(), HotkeyError> {
        let new = new_hotkey.parse::<HotKey>().map_err(HotkeyError::Parse)?;
        let old = *self.hotkey.borrow();
        if old == Some(new) {
            return Ok(());
        }
        self.manager.register(new).map_err(HotkeyError::Register)?;
        if let Some(old) = old
            && let Err(err) = self.manager.unregister(old)
        {
            // 新键已注册而旧键注销失败：收回新键，保持旧态并报错
            let _ = self.manager.unregister(new);
            return Err(HotkeyError::Register(err));
        }
        *self.hotkey.borrow_mut() = Some(new);
        *self.conflict.borrow_mut() = None;
        Ok(())
    }
}
