//! 分类 / 标签管理页视图：两页复用同一组件（`TaxonomyKind` 区分装配）。
//!
//! 列表行 = 色点（DB 持久颜色，未选色为中性空点）+ 名称 + 引用数，
//! 名称字典序排序（由数据层保证）。写操作（新建 / 重命名 / 删除 / 选色）
//! 由后续 ticket 接入；本页当前为展示 + 空态 / 错误占位。

use fragmenta_core::model::TaxonomyEntry;
use fragmenta_core::settings::FontScale;

use gpui_kit::component::button::Button;
use gpui_kit::component::empty::{
    Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyMediaVariant, EmptyTitle,
};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{TaxonomyKind, TaxonomyState, TaxonomyStore};
use crate::notes::{NotesEvent, NotesStore};
use crate::ui::layout::{LayoutClass, content_column, page_scroll};
use crate::ui::tokens::{self, TextRole, TextRoleExt as _};
use crate::utils::palette::parse_hex_color;

/// 行色点直径。
const COLOR_DOT: Pixels = px(10.);

/// 分类 / 标签管理页：常驻于主窗口 shell，订阅笔记广播刷新列表。
pub(crate) struct TaxonomyPage {
    kind: TaxonomyKind,
    state: Entity<TaxonomyState>,
    /// 条目列表快照（名称排序）。
    list: Result<Vec<TaxonomyEntry>, String>,
    /// 笔记写操作会改变分类 / 标签集合（挂载或建实体），订阅其广播刷新。
    _notes_changed: Subscription,
}

impl TaxonomyPage {
    pub(crate) fn new(kind: TaxonomyKind, cx: &mut Context<Self>) -> Self {
        let state = cx.global::<TaxonomyStore>().0.clone();
        let notes = cx.global::<NotesStore>().0.clone();
        let notes_changed = cx.subscribe(&notes, |this, _, _: &NotesEvent, cx| {
            this.refresh(cx);
        });
        let mut this = Self {
            kind,
            state,
            list: Ok(Vec::new()),
            _notes_changed: notes_changed,
        };
        this.refresh(cx);
        this
    }

    /// 重新拉取本种类条目列表。
    fn refresh(&mut self, cx: &mut Context<Self>) {
        let store = self.state.read(cx);
        self.list = self.kind.entries(store).map_err(|err| err.to_string());
    }

    /// 错误占位：说明 + 重试。
    fn render_error(&self, err: &str, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let (danger, muted) = (theme.danger, theme.muted_foreground);
        let weak = cx.entity().downgrade();
        v_flex()
            .flex_1()
            .min_h_0()
            .items_center()
            .justify_center()
            .gap_2()
            .child(div().text_color(danger).child("加载列表失败"))
            .child(div().text_size(px(12.)).text_color(muted).child(err.to_string()))
            .child(
                Button::new("taxonomy-retry")
                    .outline()
                    .small()
                    .label("重试")
                    .on_click({
                        let weak = weak.clone();
                        move |_, _, cx| {
                            let _ = weak.update(cx, |this, cx| this.refresh(cx));
                        }
                    }),
            )
            .into_any_element()
    }

    /// 空态：条目数为 0 时的引导。
    fn render_empty(&self) -> AnyElement {
        Empty::new()
            .header(
                EmptyHeader::new()
                    .media(
                        EmptyMedia::new()
                            .with_variant(EmptyMediaVariant::Icon)
                            .child(Icon::new(self.kind.icon())),
                    )
                    .title(EmptyTitle::new().child(self.kind.empty_title()))
                    .description(EmptyDescription::new().child(self.kind.empty_description())),
            )
            .into_any_element()
    }

    /// 条目列表：名称排序的行列表。
    fn render_list(&self, list: &[TaxonomyEntry], cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        v_flex()
            .flex_1()
            .min_h_0()
            .gap(tokens::spacing::SM)
            .children(list.iter().map(|entry| self.render_row(entry, theme)))
            .into_any_element()
    }

    /// 单行：色点 + 名称（截断）+ 引用数。
    fn render_row(&self, entry: &TaxonomyEntry, theme: &gpui_kit::component::Theme) -> AnyElement {
        let dot_color = entry.color.as_deref().and_then(parse_hex_color);
        let dot = match dot_color {
            Some(color) => div().size(COLOR_DOT).rounded_full().bg(color),
            None => div()
                .size(COLOR_DOT)
                .rounded_full()
                .border_1()
                .border_color(theme.border),
        };
        h_flex()
            .w_full()
            .items_center()
            .gap(tokens::spacing::MD)
            .px(tokens::spacing::LG)
            .py(tokens::spacing::MD)
            .rounded(tokens::radius::SM)
            .bg(theme.secondary)
            .border_1()
            .border_color(theme.border)
            .child(dot)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_role(TextRole::ControlLabel, FontScale::Medium)
                    .child(entry.name.clone()),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_role(TextRole::Meta, FontScale::Medium)
                    .text_color(theme.muted_foreground)
                    .child(format!("{} 篇", entry.note_count)),
            )
            .into_any_element()
    }
}

impl Render for TaxonomyPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let layout = LayoutClass::from_width(window.viewport_size().width);
        match &self.list {
            Err(err) => self.render_error(err, cx),
            Ok(list) if list.is_empty() => v_flex()
                .flex_1()
                .min_h_0()
                .w_full()
                .items_center()
                .justify_center()
                .child(self.render_empty())
                .into_any_element(),
            Ok(list) => page_scroll(
                layout,
                content_column(tokens::widths::TAXONOMY_CONTENT)
                    .flex_1()
                    .min_h_0()
                    .child(self.render_list(list, cx)),
            )
            .into_any_element(),
        }
    }
}
