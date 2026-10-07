//! 磁贴（速记窗口，架构 §1 / §5.8）：全局热键呼出的无边框置顶小窗，多开。
//!
//! PopUp + `titlebar: None`（Windows 后端原生置顶 + 不进任务栏）；弹出行为
//! ——首开中心对准光标、连开相对最新现存磁贴的当前位置级联偏移 26 逻辑
//! 像素、全关重置；圆角边框经 `SetWindowRgn` 窗口区域 + 界面层圆角描边
//! 实现。窗口打开与定位归 [`open`]，视图 / 保存闭环归 [`view`]；八向边缘
//! resize 经 win32 的 WM_NCHITTEST 子类化走系统原生模态循环，最小尺寸由
//! `window_min_size` 经后端 WM_GETMINMAXINFO 钳制。

mod open;
mod view;

pub use open::open_new;

/// 磁贴窗口初始尺寸（逻辑像素，可拖拽边缘调整大小）。
pub(crate) const TILE_W: f32 = 360.;
pub(crate) const TILE_H: f32 = 480.;

/// 相邻磁贴的级联偏移（逻辑像素，架构 §5.8）。
pub(crate) const CASCADE: f32 = 26.;

/// 圆角半径（逻辑像素）：窗口区域与界面描边共用，保证视觉吻合。
pub(crate) const CORNER: f32 = 8.;

/// 磁贴窗口最小尺寸（逻辑像素）：`window_min_size` 声明，
/// 由后端 WM_GETMINMAXINFO 在原生 resize 中钳制。
pub(crate) const MIN_W: f32 = 240.;
pub(crate) const MIN_H: f32 = 280.;
