//! widgets 层（架构 §5.10）：跨页面复用的通用小部件。

pub mod resizable_split;
pub mod tag_badge;
pub mod toast;
pub mod window_title_bar;

pub use resizable_split::ResizableSplit;
pub use tag_badge::{CategoryTag, TagBadge};
pub use window_title_bar::window_title_bar;
