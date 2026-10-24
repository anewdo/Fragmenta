//! 全局尺寸令牌（ADR-0007）：UI 像素值与文字角色的唯一来源。
//!
//! 全部为逻辑像素，DPI 缩放由 GPUI 处理；不使用 rem，也不把字号做全局倍率。
//! 页面代码只引用令牌，不新写零散字号 / 间距 / 内容宽度；确需局部特例时
//! 在调用处以命名常量集中声明。

use fragmenta_core::settings::FontScale;
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
    pub const TOAST_MAX: Pixels = px(360.);
    pub const CALENDAR: Pixels = px(272.);
    pub const TILE: Pixels = px(360.);
    pub const TILE_MIN: Pixels = px(240.);

    pub const SIDEBAR_COLLAPSED: Pixels = px(80.);
    pub const SIDEBAR_MIN: Pixels = px(180.);
    pub const SIDEBAR_MAX: Pixels = px(420.);
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
    /// 主窗口顶部通栏标题栏的高度。
    pub const TITLE_BAR: Pixels = px(40.);
}

/// 布局档位阈值：Compact < 900，Regular 900–1399，Wide ≥ 1400。
pub mod breakpoints {
    use super::{Pixels, px};

    pub const COMPACT_MAX: Pixels = px(900.);
    pub const WIDE_MIN: Pixels = px(1400.);
}

/// 文字角色：内容角色随正文字号档位缩放，界面角色固定。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TextRole {
    /// 内容区文档标题（笔记标题、磁贴标题）：中档 18，小 / 大 16 / 20。
    DocumentTitle,
    /// 内容正文（编辑器、磁贴正文）：中档 16，小 / 大 14 / 18。
    Body,
    /// 列表阅读文本（笔记卡片摘要等）：中档 14，小 / 大 12 / 16。
    Reading,
    /// 设置分组 / 页面区块标题：固定 18。
    SectionTitle,
    /// 控件标签（设置行标题、导航项）：固定 14。
    ControlLabel,
    /// 辅助说明与元信息：固定 12。
    Meta,
}

impl TextRole {
    /// 字号；只有内容角色读取 `scale`，界面角色恒定。
    pub fn size(self, scale: FontScale) -> Pixels {
        let base = match self {
            Self::DocumentTitle => px(18.),
            Self::Body => px(16.),
            Self::Reading => px(14.),
            Self::SectionTitle | Self::ControlLabel | Self::Meta => return self.fixed_size(),
        };
        match scale {
            FontScale::Small => base - px(2.),
            FontScale::Medium => base,
            FontScale::Large => base + px(2.),
        }
    }

    fn fixed_size(self) -> Pixels {
        match self {
            Self::SectionTitle => px(18.),
            Self::ControlLabel => px(14.),
            Self::Meta => px(12.),
            Self::DocumentTitle | Self::Body | Self::Reading => {
                unreachable!("内容角色使用随档位缩放的字号")
            }
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
    fn text_role(self, role: TextRole, scale: FontScale) -> Self {
        self.text_size(role.size(scale))
            .font_weight(role.weight())
            .line_height(role.line_height())
    }
}

impl<T: Styled> TextRoleExt for T {}
