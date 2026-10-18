//! 内置色系主题（ADR-0005）：预设分区域配色定义与覆盖应用。
//!
//! 每套色系只定义核心 token，整体方向为：菜单与靠近边框的区域（侧栏 /
//! 标题栏 / 菜单浮层 / 边框）取色系深调，文字内容展示区（背景 / 卡片 /
//! 面板）取色系淡调，其余继承 Light 基线。由 `SettingsState` 在
//! `Theme::change(Light)` 后经 `Theme::update` 覆盖，全窗口即时生效。
//!
//! 色值可实施期微调：改这里的十六进制即可，无需动应用逻辑。

use gpui_kit::component::ThemeColor;
use gpui_kit::{Hsla, rgb};

/// 一套内置色系的核心配色（深→淡分层见各字段注释）。
pub struct ThemeScheme {
    /// 展示名（设置页色卡标签）。
    pub label: &'static str,
    /// 文字内容展示区背景（淡）。
    pub background: Hsla,
    /// 前景文字（深）。
    pub foreground: Hsla,
    /// 次要文字。
    pub muted_foreground: Hsla,
    /// 菜单 / 侧栏区（深）。
    pub sidebar: Hsla,
    /// 标题栏（最深的 chrome 层）。
    pub title_bar: Hsla,
    /// 菜单浮层（介于背景与侧栏之间）。
    pub popover: Hsla,
    /// 面板（GroupBox 等分组卡片，淡）。
    pub panel: Hsla,
    /// 内容卡片（secondary，略深于背景）。
    pub card: Hsla,
    /// hover 强调底色。
    pub accent: Hsla,
    /// 激活强调底色（侧栏选中项 / 次级按下态）。
    pub accent_active: Hsla,
    /// 边框（深）。
    pub border: Hsla,
    /// 输入框边框。
    pub input: Hsla,
    /// 主色（选中 / 聚焦 / 链接 / 拖拽手柄）。
    pub primary: Hsla,
    /// 主色 hover。
    pub primary_hover: Hsla,
    /// 主色按下。
    pub primary_active: Hsla,
    /// 输入选区。
    pub selection: Hsla,
    /// 滚动条 thumb。
    pub scrollbar_thumb: Hsla,
    /// 滚动条 thumb hover。
    pub scrollbar_thumb_hover: Hsla,
    /// Switch 关态轨道。
    pub switch_track: Hsla,
}

impl ThemeScheme {
    /// 将核心 token 覆盖到 Light 基线的 `ThemeColor` 上（经 `Theme::update`
    /// 调用，未列出的 token 继承 Light 基线）。
    pub fn apply(&self, c: &mut ThemeColor) {
        let foreground = self.foreground;
        let white: Hsla = rgb(0xFFFFFF).into();

        // 内容面（淡）
        c.background = self.background;
        c.foreground = foreground;
        c.muted_foreground = self.muted_foreground;
        c.caret = foreground;
        c.group_box = self.panel;
        c.group_box_foreground = foreground;
        c.secondary = self.card;
        c.secondary_hover = self.accent;
        c.secondary_active = self.accent_active;
        c.secondary_foreground = foreground;

        // 菜单 / 浮层 / 列表（深调）
        c.popover = self.popover;
        c.popover_foreground = foreground;
        c.sidebar = self.sidebar;
        c.sidebar_foreground = foreground;
        c.sidebar_border = self.border;
        c.sidebar_accent = self.accent_active;
        c.sidebar_accent_foreground = foreground;
        c.title_bar = self.title_bar;
        c.title_bar_border = self.border;
        c.border = self.border;
        c.input = self.input;
        c.list = self.background;
        c.list_even = self.card;
        c.list_hover = self.accent;
        c.list_active = self.accent_active;
        c.list_active_border = self.border;
        c.list_head = self.sidebar;

        // 强调 / 主色
        c.accent = self.accent;
        c.accent_foreground = foreground;
        c.muted = self.accent;
        c.primary = self.primary;
        c.primary_hover = self.primary_hover;
        c.primary_active = self.primary_active;
        c.primary_foreground = white;
        c.selection = self.selection;
        c.ring = self.primary;
        c.drag_border = self.primary;
        c.link = self.primary;
        c.link_hover = self.primary_hover;
        c.link_active = self.primary_active;

        // 杂项
        c.scrollbar_thumb = self.scrollbar_thumb;
        c.scrollbar_thumb_hover = self.scrollbar_thumb_hover;
        c.switch = self.switch_track;
        c.skeleton = self.panel;
    }
}

/// 豆沙绿：经典护眼底色的分区域配色。
pub fn dou_sha_green() -> ThemeScheme {
    ThemeScheme {
        label: "豆沙绿",
        background: rgb(0xC7EDCC).into(),
        foreground: rgb(0x1F3D28).into(),
        muted_foreground: rgb(0x52755C).into(),
        sidebar: rgb(0xA9D4AF).into(),
        title_bar: rgb(0x97C79E).into(),
        popover: rgb(0xB4DEBB).into(),
        panel: rgb(0xD6F0DA).into(),
        card: rgb(0xBBE2C1).into(),
        accent: rgb(0xA9D8B1).into(),
        accent_active: rgb(0x93CB9C).into(),
        border: rgb(0x7FAF88).into(),
        input: rgb(0x9CC5A5).into(),
        primary: rgb(0x2F7D4E).into(),
        primary_hover: rgb(0x38915A).into(),
        primary_active: rgb(0x286A43).into(),
        selection: rgb(0x9AD2A3).into(),
        scrollbar_thumb: rgb(0x7FAF88).into(),
        scrollbar_thumb_hover: rgb(0x6F9F78).into(),
        switch_track: rgb(0xBBDCC2).into(),
    }
}

/// 天空蓝：淡天蓝内容面 + 深天蓝 chrome 的分区域配色。
pub fn sky_blue() -> ThemeScheme {
    ThemeScheme {
        label: "天空蓝",
        background: rgb(0xCBE5F6).into(),
        foreground: rgb(0x1A3550).into(),
        muted_foreground: rgb(0x4E6E88).into(),
        sidebar: rgb(0xA5CCE3).into(),
        title_bar: rgb(0x92BEDC).into(),
        popover: rgb(0xAFD5EB).into(),
        panel: rgb(0xD8EEF9).into(),
        card: rgb(0xBAD9EE).into(),
        accent: rgb(0xA2CBE3).into(),
        accent_active: rgb(0x8BBBDA).into(),
        border: rgb(0x7BA9C8).into(),
        input: rgb(0x9AC0D9).into(),
        primary: rgb(0x2E7CB9).into(),
        primary_hover: rgb(0x3790D2).into(),
        primary_active: rgb(0x25699F).into(),
        selection: rgb(0x96C8E4).into(),
        scrollbar_thumb: rgb(0x7BA9C8).into(),
        scrollbar_thumb_hover: rgb(0x6B99B8).into(),
        switch_track: rgb(0xB3D2E7).into(),
    }
}
