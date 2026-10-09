//! 可拖动双栏分栏（架构 §5.10）：kit `h_resizable` 面板组的薄封装。
//!
//! 分界线外观（细线 + hover 胶囊指示）与拖动状态记忆由 kit 提供；
//! 左栏可设初始宽度与钳制范围，右栏自适应占余宽。

use std::ops::Range;

use gpui_kit::component::{h_resizable, resizable_panel};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, App, ElementId, IntoElement, Pixels, RenderOnce, Window};

/// 水平双栏分栏：拖动分界线调整左栏宽度。
#[derive(IntoElement)]
pub struct ResizableSplit {
    id: ElementId,
    left: AnyElement,
    right: AnyElement,
    left_size: Option<Pixels>,
    left_range: Option<Range<Pixels>>,
}

impl ResizableSplit {
    /// 创建分栏，`left` / `right` 为两栏内容（经 `into_any_element` 传入）。
    pub fn new(id: impl Into<ElementId>, left: AnyElement, right: AnyElement) -> Self {
        Self {
            id: id.into(),
            left,
            right,
            left_size: None,
            left_range: None,
        }
    }

    /// 左栏初始宽度；未设置时两栏均分。
    pub fn left_size(mut self, size: impl Into<Pixels>) -> Self {
        self.left_size = Some(size.into());
        self
    }

    /// 左栏宽度范围（拖动钳制）。
    pub fn left_range(mut self, range: Range<Pixels>) -> Self {
        self.left_range = Some(range);
        self
    }
}

impl RenderOnce for ResizableSplit {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        h_resizable(self.id)
            .child(
                resizable_panel()
                    .when_some(self.left_size, |panel, size| panel.size(size))
                    .when_some(self.left_range, |panel, range| panel.size_range(range))
                    .child(self.left),
            )
            .child(resizable_panel().child(self.right))
    }
}
