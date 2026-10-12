//! 设置状态（架构 §5.6 / D8 副作用集中）：运行期 `Settings` 的唯一权威副本，
//! 修改即持久化（settings.json，ADR-0004 锚点）。
//!
//! Phase 13 ticket 08 先落地 `close_to_quit`；主题 / 字号 / 热键 / 数据目录
//! 等后续 ticket 在此扩展（`set_theme` / `set_font_scale` / …）。

use std::path::PathBuf;

use fragmenta_core::settings::Settings;
use gpui_kit::{Context, EventEmitter};

/// 设置变更广播：订阅方（设置页等）据此刷新展示。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsEvent {
    Changed,
}

impl EventEmitter<SettingsEvent> for SettingsState {}

/// 设置状态：持有当前 `Settings` 与 settings.json 路径。
pub struct SettingsState {
    settings: Settings,
    path: PathBuf,
}

impl SettingsState {
    pub fn new(settings: Settings, path: PathBuf) -> Self {
        Self { settings, path }
    }

    /// 关闭主窗口时直接退出；默认 false（托盘驻留）。
    pub fn close_to_quit(&self) -> bool {
        self.settings.close_to_quit
    }

    /// 更新"关闭主窗口时直接退出"并立即持久化。
    /// 持久化失败不中断交互（eprintln 容错，与项目现状一致）。
    pub fn set_close_to_quit(&mut self, value: bool, cx: &mut Context<Self>) {
        if self.settings.close_to_quit == value {
            return;
        }
        self.settings.close_to_quit = value;
        if let Err(err) = self.settings.save(&self.path) {
            eprintln!("[fragmenta] 保存设置失败: {err}");
        }
        cx.emit(SettingsEvent::Changed);
    }
}
