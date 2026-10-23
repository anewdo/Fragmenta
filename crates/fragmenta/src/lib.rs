//! fragmenta：应用层（bin crate 的 lib 目标）。
//!
//! 笔记、待办、设置功能及窗口与共享编辑能力归此 crate
//! 纯逻辑全部在 `fragmenta-core`，UI 层不触 SQL

pub mod app;
pub mod autostart;
pub mod editor;
pub mod hotkey;
pub mod tray;

pub mod notes;
pub mod settings;
pub mod todos;
pub mod utils;
pub mod widgets;
pub mod win32;
pub mod windows;
