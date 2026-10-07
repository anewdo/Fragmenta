//! fragmenta：应用层（bin crate 的 lib 目标）。
//!
//! state 层、编辑器与后续 windows / widgets 模块归此 crate（架构 §3）；
//! 纯逻辑全部在 `fragmenta-core`，UI 层不触 SQL（铁律 2）。

pub mod app;
pub mod editor;
pub mod state;
pub mod windows;
