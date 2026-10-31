//! 标签矩形（可勾选项）：未选中为描边，选中为加深底色 + 淡入对勾，
//! 相邻项随对勾出现自然让位。

use std::rc::Rc;
use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::component::animation::{EffectTransition, ease_out_cubic};
use gpui_kit::component::{ActiveTheme as _, Colorize as _, Icon, Sizable, Size, h_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::ui::tokens::{self, TextRole, TextRoleExt};
use crate::utils::palette::filled_foreground;

/// 选中对勾淡入时长。
const CHECK_FADE: Duration = Duration::from_millis(120);

/// 标签矩形：`id` 同时用于命中测试与对勾动画的稳定标识。
#[derive(IntoElement)]
pub(crate) struct TagChip {
    id: ElementId,
    check_id: String,
    label: SharedString,
    color: Option<Hsla>,
    selected: bool,
    on_click: Option<Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>>,
}

impl TagChip {
    pub(crate) fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        let id = id.into();
        let check_id = format!("{id}-check");
        Self {
            id,
            check_id,
            label: label.into(),
            color: None,
            selected: false,
            on_click: None,
        }
    }

    /// 标签色；`None` 为未选色（主题中性样式）。
    pub(crate) fn color(mut self, color: Option<Hsla>) -> Self {
        self.color = color;
        self
    }

    pub(crate) fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub(crate) fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for TagChip {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let Self {
            id,
            check_id,
            label,
            color,
            selected,
            on_click,
        } = self;

        let accent = color.unwrap_or(theme.muted_foreground);
        let fill = color.unwrap_or(theme.primary);
        let foreground = if color.is_some() {
            filled_foreground()
        } else {
            theme.primary_foreground
        };
        let border = if selected {
            fill
        } else {
            color.unwrap_or(theme.border)
        };
        let text_color = if selected { foreground } else { accent };

        let check = EffectTransition::new(CHECK_FADE)
            .ease(ease_out_cubic)
            .fade(0., 1.)
            .slide_x(px(-2.), px(0.))
            .apply(
                div().child(
                    Icon::new(IconName::Check)
                        .with_size(Size::XSmall)
                        .text_color(foreground),
                ),
                check_id,
            );

        h_flex()
            .id(id)
            .items_center()
            .gap_1()
            .px(tokens::spacing::SM)
            .py(px(3.))
            .rounded(tokens::radius::SM)
            .border_1()
            .border_color(border)
            .text_role(TextRole::Meta)
            .text_color(text_color)
            .cursor_pointer()
            .when(selected, |el| el.bg(fill))
            .when(selected, move |el| {
                el.hover(move |style| style.bg(fill.darken(0.08)))
            })
            .when(!selected, move |el| {
                el.hover(move |style| style.bg(accent.opacity(0.1)).border_color(accent))
            })
            .when(selected, |el| el.child(check))
            .child(format!("#{label}"))
            .on_click(move |event, window, cx| {
                if let Some(handler) = &on_click {
                    handler(event, window, cx);
                }
            })
    }
}
