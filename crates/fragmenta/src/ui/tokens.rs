//! 全局尺寸令牌（ADR-0007）：UI 像素值与文字角色的唯一来源。
//!
//! 全部为逻辑像素，DPI 缩放由 GPUI 处理；不使用 rem，也不把字号做全局倍率。
//! 页面代码只引用令牌，不新写零散字号 / 间距 / 内容宽度；确需局部特例时
//! 在调用处以命名常量集中声明。

use gpui_kit::{FontWeight, Pixels, Rems, Styled, px, rems};

/// 间距阶梯（4 的倍数）：页面留白、组内 / 组间间隔、卡片内边距。
pub mod spacing {
    use super::{Pixels, px};

    pub const XS: Pixels = px(4.);
    pub const SM: Pixels = px(8.);
    pub const MD: Pixels = px(12.);
    pub const LG: Pixels = px(16.);
    pub const XL: Pixels = px(24.);
    pub const XXL: Pixels = px(32.);
}

/// 圆角阶梯：控件与小卡片取 SM，较大面取 MD。
pub mod radius {
    use super::{Pixels, px};

    pub const SM: Pixels = px(6.);
    pub const MD: Pixels = px(8.);
}

/// 内容栏与窗口的宽度上限 / 默认值。
pub mod widths {
    use super::{Pixels, px};

    pub const SETTINGS_CONTENT: Pixels = px(640.);
    pub const TAXONOMY_CONTENT: Pixels = px(560.);
    pub const DIALOG: Pixels = px(380.);
    /// 笔记列表默认宽度：笔记页搜索栏与高级筛选面板沿用同一宽度。
    pub const NOTES_LIST: Pixels = px(fragmenta_core::settings::NOTES_LIST_WIDTH_DEFAULT_PX);
    /// 提示条固定宽度：短提示单行呈现，长错误自动换行。
    pub const TOAST: Pixels = px(260.);
    /// 待办页日历栏宽：日历卡（7 × 40 单元格 + 内边距 24 + 边框 2）306 + 页面留白 24。
    pub const CALENDAR: Pixels = px(330.);
    pub const TILE: Pixels = px(360.);
    pub const TILE_MIN: Pixels = px(240.);

    pub const SIDEBAR_COLLAPSED: Pixels = px(80.);
    pub const SIDEBAR_MIN: Pixels = px(150.);
    pub const SIDEBAR_MAX: Pixels = px(360.);
    pub const SIDEBAR_DEFAULT: Pixels = px(255.);

    /// 标题栏最右侧窗口控制按钮（最小化 / 最大化 / 关闭）的宽度。
    pub const WINDOW_CONTROL: Pixels = px(46.);

    pub const MAIN_WINDOW: Pixels = px(1080.);
    pub const MAIN_WINDOW_MIN: Pixels = px(760.);
}

/// 窗口与固定高度控件。
pub mod heights {
    use super::{Pixels, px};

    pub const TILE: Pixels = px(480.);
    pub const TILE_MIN: Pixels = px(280.);
    pub const MAIN_WINDOW: Pixels = px(720.);
    pub const MAIN_WINDOW_MIN: Pixels = px(520.);
    pub const NAV_ITEM: Pixels = px(48.);
    pub const CONTROL: Pixels = px(32.);
    /// 需要更大命中区域的图标按钮（如侧栏收起开关）。
    pub const CONTROL_LARGE: Pixels = px(40.);
    /// 主窗口顶部通栏标题栏的高度。
    pub const TITLE_BAR: Pixels = px(40.);
    /// 文档标题输入行最小高度：字号放大时行随内容增高，不裁切文字。
    pub const TITLE_ROW: Pixels = px(36.);
}

/// 布局档位阈值：Compact < 900，Regular 900–1399，Wide ≥ 1400。
pub mod breakpoints {
    use super::{Pixels, px};

    pub const COMPACT_MAX: Pixels = px(900.);
    pub const WIDE_MIN: Pixels = px(1400.);
}

/// 内容字号默认值：界面角色与未显式传入内容字号的场景使用。
pub mod font {
    use super::{Pixels, px};

    pub const DEFAULT_CONTENT: Pixels = px(16.);
}

/// 文字角色：内容角色以传入的内容字号为基准偏移，界面角色固定。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TextRole {
    /// 内容区文档标题（笔记标题、磁贴标题）：内容字号 +2。
    DocumentTitle,
    /// 内容正文（编辑器、磁贴正文）：内容字号本身。
    Body,
    /// 列表阅读文本（笔记卡片摘要等）：内容字号 -2。
    Reading,
    /// 设置分组 / 页面区块标题：固定 18。
    SectionTitle,
    /// 控件标签（设置行标题、导航项）：固定 14。
    ControlLabel,
    /// 辅助说明与元信息：固定 12。
    Meta,
}

impl TextRole {
    /// 字号；内容角色以 `content_px` 为基准，界面角色恒定。
    pub fn size(self, content_px: Pixels) -> Pixels {
        match self {
            Self::DocumentTitle => content_px + px(2.),
            Self::Body => content_px,
            Self::Reading => content_px - px(2.),
            Self::SectionTitle => px(18.),
            Self::ControlLabel => px(14.),
            Self::Meta => px(12.),
        }
    }

    pub fn weight(self) -> FontWeight {
        match self {
            Self::DocumentTitle | Self::SectionTitle => FontWeight::SEMIBOLD,
            Self::ControlLabel => FontWeight::MEDIUM,
            Self::Body | Self::Reading | Self::Meta => FontWeight::NORMAL,
        }
    }

    pub fn line_height(self) -> Rems {
        rems(match self {
            Self::DocumentTitle | Self::SectionTitle => 1.3,
            Self::Body => 1.6,
            Self::Reading => 1.5,
            Self::ControlLabel | Self::Meta => 1.4,
        })
    }
}

/// 给任意可样式元素套用文字角色的字号 / 字重 / 行高；颜色仍由调用方决定。
pub trait TextRoleExt: Styled + Sized {
    /// 界面角色（固定字号）；内容角色请用 [`Self::text_role_sized`]。
    fn text_role(self, role: TextRole) -> Self {
        self.text_role_sized(role, font::DEFAULT_CONTENT)
    }

    /// 内容角色：字号以设置中的内容像素为基准。
    fn text_role_sized(self, role: TextRole, content_px: Pixels) -> Self {
        self.text_size(role.size(content_px))
            .font_weight(role.weight())
            .line_height(role.line_height())
    }
}

impl<T: Styled> TextRoleExt for T {}
