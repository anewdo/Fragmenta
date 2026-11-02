//! 磁贴（速记窗口）：全局热键呼出的无边框置顶小窗，多开。

mod open;
mod view;

pub use open::open_new;

/// 相邻磁贴的级联偏移（逻辑像素，架构 §5.8）。
pub(crate) const CASCADE: f32 = 26.;

/// 圆角半径（逻辑像素）：窗口区域与界面描边共用，保证视觉吻合。
pub(crate) const CORNER: f32 = 8.;

/// 磁贴窗口最小尺寸（逻辑像素）：`window_min_size` 声明，
/// 由后端 WM_GETMINMAXINFO 在原生 resize 中钳制。
pub(crate) const MIN_W: f32 = 240.;
pub(crate) const MIN_H: f32 = 280.;
