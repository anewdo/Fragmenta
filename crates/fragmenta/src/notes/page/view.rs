//! 笔记页视图构建：筛选栏 / 卡片列表 / 空态与错误占位 / render 组装。
//! 状态与交互逻辑（订阅 / 快照拉取 / 对话框）见 `mod.rs`。

use std::time::Duration;

use chrono::{DateTime, Local};
use fragmenta_core::model::{Note, SearchScope};

use gpui_kit::assets::IconName;
use gpui_kit::component::animation::{EffectTransition, ease_out_cubic};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::empty::{
    Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyMediaVariant, EmptyTitle,
};
use gpui_kit::component::input::Input;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Selectable, Sizable, Size, h_flex, v_flex,
};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{EditorMode, NotesPage};
use crate::settings::SettingsStore;
use crate::ui::tokens::{self, TextRole, TextRoleExt};
use crate::widgets::{CategoryTag, ResizableSplit, TagBadge};

/// 卡片摘要最多显示行数（超出以省略号截断）。
const CARD_SUMMARY_LINES: usize = 2;
/// 选中卡片左侧 Fluent 指示条宽度。
const CARD_SELECTION_BAR: Pixels = px(3.);
/// 悬停操作按钮淡入时长。
const CARD_ACTION_FADE: Duration = Duration::from_millis(120);
/// 两个 32px 操作按钮及其间距占用的宽度。
const CARD_ACTIONS_WIDTH: Pixels = px(68.);

/// 创建时间的相对文案：一周内按刚刚 / 分钟 / 小时 / 天，更早显示日期。
fn relative_created_at(created_at: DateTime<Local>) -> String {
    relative_at(created_at, Local::now())
}

fn relative_at(created_at: DateTime<Local>, now: DateTime<Local>) -> String {
    let seconds = now.signed_duration_since(created_at).num_seconds();
    if seconds < 60 {
        "刚刚".to_string()
    } else if seconds < 60 * 60 {
        format!("{} 分钟前", seconds / 60)
    } else if seconds < 60 * 60 * 24 {
        format!("{} 小时前", seconds / 60 / 60)
    } else if seconds < 60 * 60 * 24 * 7 {
        format!("{} 天前", seconds / 60 / 60 / 24)
    } else {
        created_at.format("%Y-%m-%d").to_string()
    }
}

impl NotesPage {
    /// 页顶操作区：编辑器三模式切换组（渲染于笔记页顶部右侧）。
    ///
    /// 未选中笔记时整组禁用：编辑器空态占位与模式无关，可点的模式按钮
    /// 会读成"点了没反应"；灰置直到选中卡片才可切。
    fn editor_mode_group(&self, cx: &Context<Self>) -> AnyElement {
        let mode = self.editor.read(cx).mode();
        let disabled = self.selected.is_none();
        let button = |id: &'static str, label: &'static str, icon: IconName, target: EditorMode| {
            Button::new(id)
                .ghost()
                .small()
                .compact()
                .icon(icon)
                .label(label)
                .selected(mode == target)
                .disabled(disabled)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.editor
                        .update(cx, |editor, cx| editor.set_mode(target, cx));
                    cx.notify();
                }))
        };
        h_flex()
            .items_center()
            .gap_0p5()
            .p(px(1.))
            .border_1()
            .border_color(cx.theme().border)
            .rounded(px(4.))
            .child(button(
                "editor-mode-source",
                "源码",
                IconName::PenLine,
                EditorMode::Source,
            ))
            .child(button(
                "editor-mode-preview",
                "预览",
                IconName::Eye,
                EditorMode::Preview,
            ))
            .child(button(
                "editor-mode-split",
                "分屏",
                IconName::Columns2,
                EditorMode::Split,
            ))
            .into_any_element()
    }

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

    /// 单张笔记卡片（Fluent）：选中指示条 + 材质背景 + 悬停淡入操作。
    ///
    /// 头部标题（单行截断）与右上分类；中部多行截断摘要；底部左侧平铺
    /// 标签、右侧相对创建时间。点击卡片任意处 = 选中载入编辑器。
    fn note_card(
        &self,
        note: &Note,
        selected: bool,
        weak: &WeakEntity<Self>,
        content_px: Pixels,
        cx: &App,
    ) -> Stateful<Div> {
        let note_id = note.id;
        let hovered = self.hovered_note == Some(note_id);
        let title = if note.title.is_empty() {
            "无标题".to_string()
        } else {
            note.title.clone()
        };
        let summary = note.summary();
        let created = relative_created_at(note.created_at);
        let theme = cx.theme();
        // 卡片材质取主题 secondary（可携带主题 JSON 的渐变），悬停 / 选中
        // 背景与指示条颜色均来自主题角色，随 Light / Dark 各自维护。
        let surface: Background = theme.tokens.secondary.background;
        let selected_bg = Background::from(theme.list_active);
        let (hover_bg, border, selection, muted) = (
            theme.list_hover,
            theme.border,
            theme.list_active_border,
            theme.muted_foreground,
        );

        let action = |id: &'static str, icon: IconName, tooltip: &'static str| {
            Button::new((id, note_id as u64))
                .ghost()
                .compact()
                .w(tokens::heights::CONTROL)
                .h(tokens::heights::CONTROL)
                .icon(icon)
                .tooltip(tooltip)
        };

        let taxonomy = action("note-taxonomy", IconName::SquarePen, "分类与标签").on_click({
            let weak = weak.clone();
            let note = note.clone();
            move |_, window, cx| {
                let _ = weak.update(cx, |this, cx| {
                    this.open_taxonomy_dialog(note.clone(), window, cx)
                });
            }
        });
        let delete = action("note-delete", IconName::Trash, "删除").on_click({
            let weak = weak.clone();
            move |_, window, cx| {
                let _ = weak.update(cx, |this, cx| this.confirm_delete(note_id, window, cx));
            }
        });

        let actions = EffectTransition::new(CARD_ACTION_FADE)
            .ease(ease_out_cubic)
            .fade(0., 1.)
            .apply(
                h_flex()
                    .absolute()
                    .top_0()
                    .right_0()
                    .bottom_0()
                    .items_center()
                    .gap_1()
                    .w(CARD_ACTIONS_WIDTH)
                    .child(taxonomy)
                    .child(delete),
                format!("note-card-actions-{note_id}-{}", self.hover_seq),
            )
            .into_any_element();

        v_flex()
            .id(("note-card", note_id as u64))
            .relative()
            .gap_2()
            .p(tokens::spacing::MD)
            .rounded(tokens::radius::SM)
            .bg(if selected { selected_bg } else { surface })
            .border_1()
            .border_color(if selected {
                selection.opacity(0.35)
            } else {
                border
            })
            .cursor_pointer()
            .on_click({
                let weak = weak.clone();
                move |_, window, cx| {
                    let _ = weak.update(cx, |this, cx| {
                        this.select_note(note_id, window, cx);
                    });
                }
            })
            .on_hover({
                let weak = weak.clone();
                move |hovered, _, cx| {
                    let _ = weak.update(cx, |this, cx| {
                        if *hovered {
                            if this.hovered_note != Some(note_id) {
                                this.hovered_note = Some(note_id);
                                this.hover_seq = this.hover_seq.wrapping_add(1);
                                cx.notify();
                            }
                        } else if this.hovered_note == Some(note_id) {
                            this.hovered_note = None;
                            cx.notify();
                        }
                    });
                }
            })
            .when(!selected, |el| el.hover(move |style| style.bg(hover_bg)))
            // 选中指示条：Fluent 左侧 3px Accent 色条（颜色随主题定义）
            .when(selected, |el| {
                el.child(
                    div()
                        .absolute()
                        .left_0()
                        .top(tokens::spacing::SM)
                        .bottom(tokens::spacing::SM)
                        .w(CARD_SELECTION_BAR)
                        .rounded_full()
                        .bg(selection),
                )
            })
            // 头部：标题（截断）+ 分类（右上角）；悬停操作绝对定位于分类
            // 之上，不参与布局，避免悬停时卡片高度与标题宽度跳动。
            .child(
                h_flex()
                    .relative()
                    .items_center()
                    .gap_2()
                    .min_h(tokens::heights::CONTROL)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_role_sized(TextRole::DocumentTitle, content_px)
                            .child(title),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .min_w(CARD_ACTIONS_WIDTH)
                            .h_full()
                            .flex()
                            .items_center()
                            .justify_end()
                            .when(hovered, |el| el.invisible())
                            .when_some(note.category.clone(), |el, cat| {
                                let color = self.category_colors.get(&cat).copied().flatten();
                                el.child(CategoryTag::new(cat).color(color))
                            }),
                    )
                    .when(hovered, |el| el.child(actions)),
            )
            // 中部：摘要（多行截断；空正文时省略）
            .when(!summary.is_empty(), |el| {
                el.child(
                    div()
                        .w_full()
                        .text_role_sized(TextRole::Reading, content_px)
                        .text_color(muted)
                        .line_clamp(CARD_SUMMARY_LINES)
                        .child(summary),
                )
            })
            // 底部：左侧平铺标签 + 右侧相对创建时间
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(h_flex().flex_1().min_w_0().flex_wrap().gap_1().children(
                        note.tags.iter().map(|tag| {
                            TagBadge::new(tag.clone())
                                .color(self.tag_colors.get(tag).copied().flatten())
                        }),
                    ))
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_role(TextRole::Meta)
                            .text_color(muted)
                            .child(created),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::relative_at;
    use chrono::{Duration, Local};

    #[test]
    fn relative_time_uses_readable_buckets() {
        let now = Local::now();
        assert_eq!(relative_at(now - Duration::seconds(5), now), "刚刚");
        assert_eq!(relative_at(now - Duration::minutes(5), now), "5 分钟前");
        assert_eq!(relative_at(now - Duration::hours(3), now), "3 小时前");
        assert_eq!(relative_at(now - Duration::days(3), now), "3 天前");
        let old = now - Duration::days(30);
        assert_eq!(relative_at(old, now), old.format("%Y-%m-%d").to_string());
    }
}

impl Render for NotesPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.entity().downgrade();
        let (border, muted, danger) = {
            let theme = cx.theme();
            (theme.border, theme.muted_foreground, theme.danger)
        };
        // 内容阅读面：列表标题 / 摘要随编辑器字号缩放（辅助小字保持固定）
        let content_px = cx.global::<SettingsStore>().0.read(cx).editor_font_px();
        let notes_list_width = cx.global::<SettingsStore>().0.read(cx).notes_list_width();
        let split_state = self.split_state.clone();

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
                .text_size(content_px)
                .children(notes.iter().map(|note| {
                    let selected = self.selected == Some(note.id);
                    self.note_card(note, selected, &weak, content_px, cx)
                }))
                .into_any_element(),
        };

        // 左栏：筛选栏 + 列表（双栏组装，Phase 10）
        let left = v_flex()
            .size_full()
            .min_w_0()
            .child(filter_bar)
            .child(list_area);

        // 页顶操作区：编辑器三模式切换靠右；底部分割线将其与列表、
        // 编辑器内容隔开；其下为列表 / 编辑器双栏
        v_flex()
            .size_full()
            .child(
                h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_end()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(border)
                    .child(self.editor_mode_group(cx)),
            )
            .child(
                // 双栏：分界线可拖动（左栏 400px 起步，钳制 ≥240px）
                div().flex_1().min_h_0().child(
                    ResizableSplit::new(
                        "notes-page-split",
                        left.into_any_element(),
                        self.editor.clone().into_any_element(),
                    )
                    .with_state(split_state)
                    .left_size(notes_list_width)
                    .left_range(px(240.)..Pixels::MAX)
                    .on_resize(|state, _, cx| {
                        if let Some(width) = state.read(cx).sizes().first().copied() {
                            let settings = cx.global::<SettingsStore>().0.clone();
                            settings.update(cx, |state, _| state.set_notes_list_width(width));
                        }
                    }),
                ),
            )
    }
}
