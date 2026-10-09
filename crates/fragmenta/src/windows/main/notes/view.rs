//! 笔记页视图构建：筛选栏 / 卡片列表 / 空态与错误占位 / render 组装。
//! 状态与交互逻辑（订阅 / 快照拉取 / 对话框）见 `mod.rs`。

use fragmenta_core::model::{Note, SearchScope};

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::empty::{
    Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyMediaVariant, EmptyTitle,
};
use gpui_kit::component::input::Input;
use gpui_kit::component::menu::{ContextMenu, ContextMenuExt, DropdownMenu, PopupMenuItem};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::{ActiveTheme as _, Icon, Selectable, Sizable, Size, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::NotesPage;
use crate::widgets::{CategoryTag, TagBadge};

/// 卡片标签徽片上限：超过仅显示前 3 个 + "+N" 计数。
const CARD_TAG_LIMIT: usize = 3;

impl NotesPage {
    /// 分类筛选按钮 + 下拉：全部分类 + 各分类（单选可清）+ 重命名入口（选中时）。
    fn category_filter_button(&self, weak: &WeakEntity<Self>) -> AnyElement {
        let weak = weak.clone();
        let current = self.category.clone();
        let categories = self.categories.clone();
        Button::new("notes-category-filter")
            .outline()
            .small()
            .icon(IconName::Folder)
            .label(current.clone().unwrap_or_else(|| "全部分类".into()))
            .selected(current.is_some())
            .dropdown_menu(move |mut menu, _, _| {
                menu = menu.item({
                    let weak = weak.clone();
                    PopupMenuItem::new("全部分类")
                        .checked(current.is_none())
                        .on_click(move |_, _, cx| {
                            let _ = weak.update(cx, |this, cx| {
                                this.category = None;
                                this.refresh(cx);
                            });
                        })
                });
                for cat in &categories {
                    let weak = weak.clone();
                    let cat = cat.clone();
                    let checked = current.as_deref() == Some(cat.as_str());
                    menu = menu.item(PopupMenuItem::new(cat.as_str()).checked(checked).on_click(
                        move |_, _, cx| {
                            let _ = weak.update(cx, |this, cx| {
                                this.category = Some(cat.clone());
                                this.refresh(cx);
                            });
                        },
                    ));
                }
                if let Some(from) = &current {
                    let weak = weak.clone();
                    let from = from.clone();
                    menu = menu.separator().item(
                        PopupMenuItem::new(format!("重命名「{from}」…")).on_click(
                            move |_, window, cx| {
                                let _ = weak.update(cx, |this, cx| {
                                    this.open_rename_category(from.clone(), window, cx)
                                });
                            },
                        ),
                    );
                }
                menu
            })
            .into_any_element()
    }

    /// 标签筛选按钮 + Popover 多选面板（选择即时生效，面板保持打开）。
    fn tag_filter_popover(&self, weak: &WeakEntity<Self>, muted: Hsla) -> Popover {
        let weak = weak.clone();
        let all_tags = self.tags.clone();
        let selected = self.selected_tags.clone();
        let label = if selected.is_empty() {
            "标签".to_string()
        } else {
            format!("标签 ·{}", selected.len())
        };
        Popover::new("notes-tag-filter")
            .trigger(
                Button::new("notes-tag-filter-btn")
                    .outline()
                    .small()
                    .icon(IconName::Tags)
                    .label(label)
                    .selected(!selected.is_empty()),
            )
            .content(move |_, _, _| {
                v_flex()
                    .w(px(200.))
                    .gap_1()
                    .p_2()
                    .children(all_tags.iter().enumerate().map(|(ix, tag)| {
                        let weak = weak.clone();
                        let tag = tag.clone();
                        let checked = selected.contains(&tag);
                        Checkbox::new(("filter-tag", ix))
                            .label(tag.as_str())
                            .checked(checked)
                            .on_change(move |value, _, cx| {
                                let _ = weak.update(cx, |this, cx| {
                                    if *value {
                                        this.selected_tags.push(tag.clone());
                                    } else {
                                        this.selected_tags.retain(|t| t != &tag);
                                    }
                                    this.refresh(cx);
                                });
                            })
                    }))
                    .when(all_tags.is_empty(), |el| {
                        el.child(div().text_size(px(12.)).text_color(muted).child("暂无标签"))
                    })
            })
    }

    /// 单张笔记卡片：标题行 + 摘要 + 分类 / 标签徽片 + 右键菜单 + 删除确认。
    fn note_card(
        &self,
        note: &Note,
        weak: &WeakEntity<Self>,
        card_bg: Hsla,
        border: Hsla,
        muted: Hsla,
        accent: Hsla,
    ) -> ContextMenu<Stateful<Div>> {
        let note_id = note.id;
        let title = if note.title.is_empty() {
            "无标题".to_string()
        } else {
            note.title.clone()
        };
        let summary = note.summary();
        let created = note.created_at.format("%Y-%m-%d %H:%M").to_string();
        let extra_tags = note.tags.len().saturating_sub(CARD_TAG_LIMIT);

        // 右键菜单数据：全量分类 / 标签选项 + 该笔记当前归属（checked 态）
        let weak_menu = weak.clone();
        let categories = self.categories.clone();
        let all_tags = self.tags.clone();
        let note_category = note.category.clone();
        let note_tags = note.tags.clone();

        v_flex()
            .id(("note-card", note_id as u64))
            .gap_1p5()
            .p_3()
            .rounded(px(6.))
            .bg(card_bg)
            .border_1()
            .border_color(border)
            .hover(move |style| style.border_color(accent))
            .context_menu(move |mut menu, _, _| {
                // 分类：唯一、可清除（"无分类" = 清除）
                menu = menu.label("分类").item({
                    let weak = weak_menu.clone();
                    PopupMenuItem::new("无分类")
                        .checked(note_category.is_none())
                        .on_click(move |_, _, cx| {
                            let _ = weak.update(cx, |this, cx| {
                                this.notes
                                    .update(cx, |s, cx| s.set_category(note_id, None, cx));
                            });
                        })
                });
                for cat in &categories {
                    let weak = weak_menu.clone();
                    let cat = cat.clone();
                    let checked = note_category.as_deref() == Some(cat.as_str());
                    menu = menu.item(PopupMenuItem::new(cat.as_str()).checked(checked).on_click(
                        move |_, _, cx| {
                            let _ = weak.update(cx, |this, cx| {
                                this.notes
                                    .update(cx, |s, cx| s.set_category(note_id, Some(&cat), cx));
                            });
                        },
                    ));
                }
                // 标签：多选切换（点击后菜单关闭，重开继续；Changed 驱动刷新）
                menu = menu.separator().label("标签");
                if all_tags.is_empty() {
                    menu = menu.label("暂无标签");
                }
                for tag in &all_tags {
                    let weak = weak_menu.clone();
                    let mut next = note_tags.clone();
                    let had = next.iter().any(|t| t == tag);
                    if had {
                        next.retain(|t| t != tag);
                    } else {
                        next.push(tag.clone());
                    }
                    menu = menu.item(PopupMenuItem::new(tag.as_str()).checked(had).on_click(
                        move |_, _, cx| {
                            let _ = weak.update(cx, |this, cx| {
                                this.notes
                                    .update(cx, |s, cx| s.set_tags(note_id, next.clone(), cx));
                            });
                        },
                    ));
                }
                menu
            })
            // 标题行：标题 + 创建时间 + 删除
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_weight(FontWeight::MEDIUM)
                            .truncate()
                            .child(title),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(11.))
                            .text_color(muted)
                            .child(created),
                    )
                    .child(
                        Button::new(("note-delete", note_id as u64))
                            .ghost()
                            .compact()
                            .xsmall()
                            .icon(IconName::Trash)
                            .on_click({
                                let weak = weak.clone();
                                move |_, window, cx| {
                                    let _ = weak.update(cx, |this, cx| {
                                        this.confirm_delete(note_id, window, cx)
                                    });
                                }
                            }),
                    ),
            )
            // 摘要（空正文时省略该行）
            .when(!summary.is_empty(), |el| {
                el.child(
                    div()
                        .text_size(px(12.))
                        .text_color(muted)
                        .truncate()
                        .child(summary),
                )
            })
            // 分类 + 标签徽片（≤3 个，超出以 "+N" 计数）
            .when(note.category.is_some() || !note.tags.is_empty(), |el| {
                el.child(
                    h_flex()
                        .flex_wrap()
                        .gap_1()
                        .when_some(note.category.clone(), |el, cat| {
                            el.child(CategoryTag::new(cat))
                        })
                        .children(
                            note.tags
                                .iter()
                                .take(CARD_TAG_LIMIT)
                                .map(|tag| TagBadge::new(tag.clone())),
                        )
                        .when(extra_tags > 0, |el| {
                            el.child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(muted)
                                    .child(format!("+{extra_tags}")),
                            )
                        }),
                )
            })
    }
}

impl Render for NotesPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.entity().downgrade();
        let (background, card_bg, border, muted, accent, danger) = {
            let theme = cx.theme();
            (
                theme.background,
                theme.secondary,
                theme.border,
                theme.muted_foreground,
                theme.accent,
                theme.danger,
            )
        };

        // 行 1：搜索框 + 范围切换（标题 / 全文，selected 态分段）
        let search_row = h_flex()
            .items_center()
            .gap_2()
            .child(
                div().flex_1().min_w_0().child(
                    Input::new(&self.query_input)
                        .small()
                        .w_full()
                        .prefix(Icon::new(IconName::Search).with_size(Size::XSmall)),
                ),
            )
            .child(
                Button::new("scope-title")
                    .ghost()
                    .small()
                    .compact()
                    .label("标题")
                    .selected(self.scope == SearchScope::Title)
                    .on_click({
                        let weak = weak.clone();
                        move |_, _, cx| {
                            let _ = weak.update(cx, |this, cx| {
                                this.scope = SearchScope::Title;
                                this.refresh(cx);
                            });
                        }
                    }),
            )
            .child(
                Button::new("scope-fulltext")
                    .ghost()
                    .small()
                    .compact()
                    .label("全文")
                    .selected(self.scope == SearchScope::FullText)
                    .on_click({
                        let weak = weak.clone();
                        move |_, _, cx| {
                            let _ = weak.update(cx, |this, cx| {
                                this.scope = SearchScope::FullText;
                                this.refresh(cx);
                            });
                        }
                    }),
            );

        // 行 2：分类下拉 + 标签多选
        let filter_row = h_flex()
            .items_center()
            .gap_2()
            .child(self.category_filter_button(&weak))
            .child(self.tag_filter_popover(&weak, muted));

        let filter_bar = v_flex()
            .flex_shrink_0()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(border)
            .child(search_row)
            .child(filter_row);

        // 列表区：错误占位（含重试）/ 空态 / 卡片列表
        let list_area = match &self.list {
            Err(err) => v_flex()
                .flex_1()
                .min_h_0()
                .items_center()
                .justify_center()
                .gap_2()
                .child(div().text_color(danger).child("加载笔记列表失败"))
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(muted)
                        .child(err.clone()),
                )
                .child(
                    Button::new("notes-retry")
                        .outline()
                        .small()
                        .icon(IconName::RotateCw)
                        .label("重试")
                        .on_click({
                            let weak = weak.clone();
                            move |_, _, cx| {
                                let _ = weak.update(cx, |this, cx| this.refresh(cx));
                            }
                        }),
                )
                .into_any_element(),
            Ok(notes) if notes.is_empty() => {
                let filtering = self.has_active_filter(cx);
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        Empty::new().header(
                            EmptyHeader::new()
                                .media(
                                    EmptyMedia::new()
                                        .with_variant(EmptyMediaVariant::Icon)
                                        .child(Icon::new(IconName::NotebookPen)),
                                )
                                .title(EmptyTitle::new().child(if filtering {
                                    "没有匹配的笔记"
                                } else {
                                    "还没有笔记"
                                }))
                                .description(EmptyDescription::new().child(if filtering {
                                    "调整关键词或筛选条件试试"
                                } else {
                                    "按全局热键呼出磁贴，随手记下第一条笔记"
                                })),
                        ),
                    )
                    .into_any_element()
            }
            Ok(notes) => v_flex()
                .flex_1()
                .min_h_0()
                .gap_2()
                .px_3()
                .py_2()
                .overflow_y_scrollbar()
                .children(
                    notes
                        .iter()
                        .map(|note| self.note_card(note, &weak, card_bg, border, muted, accent)),
                )
                .into_any_element(),
        };

        v_flex()
            .size_full()
            .bg(background)
            .child(filter_bar)
            .child(list_area)
    }
}
