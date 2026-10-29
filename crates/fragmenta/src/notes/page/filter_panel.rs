//! 筛选 Popover 内容面板：分类 / 标签草稿与选项树。
//!
//! 面板是独立 `Entity`：草稿与选项快照都在面板内维护，笔记页重绘（含卡片
//! hover）只复用一个实体元素，不再克隆选项或重建整棵筛选树。

use fragmenta_core::model::TaxonomyEntry;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{
    ActiveTheme as _, Icon, Sizable, Size, Theme, ThemeStyled as _, h_flex, v_flex,
};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::NotesPage;
use crate::ui::tokens::{self, TextRole, TextRoleExt};

/// 筛选面板中的可选项：左侧名称（可带图标），右侧引用计数。
fn filter_option(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    count: i64,
    icon: Option<IconName>,
    selected: bool,
    theme: &Theme,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let label: SharedString = label.into();
    let muted = theme.muted_foreground;
    let border = theme.border;
    let surface: Background = theme.tokens.secondary.background;
    let active_bg = theme.list_active;
    let active_border = theme.list_active_border;
    let hover = theme.list_hover;

    h_flex()
        .id(id)
        .items_center()
        .justify_between()
        .gap_2()
        .px(tokens::spacing::SM)
        .py(tokens::spacing::XS)
        .border_1()
        .border_color(if selected { active_border } else { border })
        .bg(if selected {
            Background::from(active_bg)
        } else {
            surface
        })
        .rounded(tokens::radius::SM)
        .cursor_pointer()
        .when(!selected, |el| el.hover(move |style| style.bg(hover)))
        .child(
            h_flex()
                .min_w_0()
                .items_center()
                .gap_1()
                .when_some(icon, |el, icon| {
                    el.child(Icon::new(icon).with_size(Size::XSmall))
                })
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_role(TextRole::Meta)
                        .child(label),
                ),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_role(TextRole::Meta)
                .text_color(muted)
                .child(count.to_string()),
        )
        .on_click(on_click)
}

/// 筛选面板：草稿在面板内维护，点“完成筛选”才写回笔记页。
pub(super) struct FilterPanel {
    page: WeakEntity<NotesPage>,
    categories: Vec<TaxonomyEntry>,
    tags: Vec<TaxonomyEntry>,
    note_count: i64,
    draft_categories: Vec<String>,
    draft_tags: Vec<String>,
}

impl FilterPanel {
    pub(super) fn new(page: WeakEntity<NotesPage>) -> Self {
        Self {
            page,
            categories: Vec::new(),
            tags: Vec::new(),
            note_count: 0,
            draft_categories: Vec::new(),
            draft_tags: Vec::new(),
        }
    }

    /// 更新筛选选项快照（笔记页拉取列表 / 分类 / 标签后推送）。
    pub(super) fn sync_options(
        &mut self,
        categories: Vec<TaxonomyEntry>,
        tags: Vec<TaxonomyEntry>,
        note_count: i64,
    ) {
        self.categories = categories;
        self.tags = tags;
        self.note_count = note_count;
    }

    /// 草稿重置为已生效条件（Popover 打开 / 关闭时）。
    pub(super) fn reset_draft(&mut self, categories: &[String], tags: &[String]) {
        self.draft_categories = categories.to_vec();
        self.draft_tags = tags.to_vec();
    }
}

impl Render for FilterPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.entity().downgrade();
        let page = self.page.clone();
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let border = theme.border;

        let mut category_grid = div().grid().grid_cols(2).gap_2();
        category_grid = category_grid.child(
            filter_option(
                ("filter-cat", 0usize),
                "所有笔记",
                self.note_count,
                Some(IconName::LibraryBig),
                self.draft_categories.is_empty(),
                theme,
                {
                    let weak = weak.clone();
                    move |_, _, cx| {
                        let _ = weak.update(cx, |panel, cx| {
                            panel.draft_categories.clear();
                            cx.notify();
                        });
                    }
                },
            )
            .w_full(),
        );
        for (ix, entry) in self.categories.iter().enumerate() {
            let weak = weak.clone();
            let name = entry.name.clone();
            let selected = self.draft_categories.contains(&entry.name);
            category_grid = category_grid.child(
                filter_option(
                    ("filter-cat", ix + 1),
                    entry.name.clone(),
                    entry.note_count,
                    Some(IconName::Folder),
                    selected,
                    theme,
                    move |_, _, cx| {
                        let _ = weak.update(cx, |panel, cx| {
                            if let Some(pos) =
                                panel.draft_categories.iter().position(|c| c == &name)
                            {
                                panel.draft_categories.remove(pos);
                            } else {
                                panel.draft_categories.push(name.clone());
                            }
                            cx.notify();
                        });
                    },
                )
                .w_full(),
            );
        }

        let tag_chips = h_flex()
            .flex_wrap()
            .gap_1()
            .children(self.tags.iter().enumerate().map(|(ix, entry)| {
                let weak = weak.clone();
                let name = entry.name.clone();
                let selected = self.draft_tags.contains(&entry.name);
                filter_option(
                    ("filter-tag", ix),
                    format!("#{}", entry.name),
                    entry.note_count,
                    None,
                    selected,
                    theme,
                    move |_, _, cx| {
                        let _ = weak.update(cx, |panel, cx| {
                            if panel.draft_tags.iter().any(|tag| tag == &name) {
                                panel.draft_tags.retain(|tag| tag != &name);
                            } else {
                                panel.draft_tags.push(name.clone());
                            }
                            cx.notify();
                        });
                    },
                )
                .rounded_full()
            }));

        v_flex()
            .w(tokens::widths::NOTES_LIST)
            .p(tokens::spacing::MD)
            .popover_style(cx)
            .gap_3()
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Icon::new(IconName::Funnel)
                                    .with_size(Size::Small)
                                    .text_color(theme.primary),
                            )
                            .child(
                                div()
                                    .text_size(tokens::font::DEFAULT_CONTENT)
                                    .font_weight(TextRole::SectionTitle.weight())
                                    .child("高级筛选"),
                            ),
                    )
                    .child(
                        Button::new("notes-filter-reset")
                            .ghost()
                            .small()
                            .text_color(theme.link)
                            .icon(IconName::RotateCcw)
                            .label("重置条件")
                            .on_click({
                                let weak = weak.clone();
                                move |_, _, cx| {
                                    let _ = weak.update(cx, |panel, cx| {
                                        panel.draft_categories.clear();
                                        panel.draft_tags.clear();
                                        cx.notify();
                                    });
                                }
                            }),
                    ),
            )
            .child(
                div()
                    .text_role(TextRole::ControlLabel)
                    .text_color(muted)
                    .child("分类维度"),
            )
            .child(category_grid)
            .child(
                div()
                    .text_role(TextRole::ControlLabel)
                    .text_color(muted)
                    .child("标签过滤（支持多选）"),
            )
            .child(if self.tags.is_empty() {
                div()
                    .text_role(TextRole::Meta)
                    .text_color(muted)
                    .child("暂无标签")
                    .into_any_element()
            } else {
                tag_chips.into_any_element()
            })
            .child(
                h_flex()
                    .justify_end()
                    .border_t_1()
                    .border_color(border)
                    .pt(tokens::spacing::MD)
                    .child(
                        Button::new("notes-filter-apply")
                            .primary()
                            .small()
                            .label("完成筛选")
                            .on_click({
                                let weak = weak.clone();
                                let page = page.clone();
                                move |_, _, cx| {
                                    let Some(panel) = weak.upgrade() else {
                                        return;
                                    };
                                    let (categories, tags) = {
                                        let panel = panel.read(cx);
                                        (panel.draft_categories.clone(), panel.draft_tags.clone())
                                    };
                                    let _ = page.update(cx, |page, cx| {
                                        page.apply_filter_draft(categories, tags, cx)
                                    });
                                }
                            }),
                    ),
            )
    }
}
