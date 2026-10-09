//! widgets 层（架构 §5.10）：跨页面复用的通用小部件。

pub mod resizable_split;
pub mod tag_badge;

pub use resizable_split::ResizableSplit;
pub use tag_badge::{CategoryTag, TagBadge};
