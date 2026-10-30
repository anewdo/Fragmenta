//! 笔记页视图构建：筛选栏 / 卡片列表 / 空态与错误占位 / render 组装。
//! 状态与交互逻辑（订阅 / 快照拉取 / 对话框）见 `mod.rs`。

use std::time::Duration;

use fragmenta_core::model::{Note, NoteSort};

use gpui_kit::assets::IconName;
use gpui_kit::component::animation::{EffectTransition, ease_out_cubic};
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui_kit::component::empty::{
    Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyMediaVariant, EmptyTitle,
};
use gpui_kit::component::input::Input;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::{ActiveTheme as _, Icon, Selectable, Sizable, Size, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::NotesPage;
use crate::settings::SettingsStore;
use crate::ui::focus::{TextFieldExt as _, input_border};
use crate::ui::note_list;
use crate::ui::tokens::{self, TextRole};
use crate::utils::time;
use crate::widgets::{CategoryTag, ResizableSplit, TagBadge};

/// 卡片摘要最多显示行数（超出以省略号截断）。
const CARD_SUMMARY_LINES: usize = 2;
/// 卡片标题字号（固定，不随编辑器字号变化）。
const CARD_TITLE_FONT: Pixels = px(13.);
/// 卡片摘要字号（固定，不随编辑器字号变化）。
const CARD_SUMMARY_FONT: Pixels = px(11.);
/// 卡片元信息（相对时间）字号。
const CARD_META_FONT: Pixels = px(11.);
/// 选中卡片左侧 Fluent 指示条宽度。
const CARD_SELECTION_BAR: Pixels = px(3.);
/// 悬停操作按钮淡入时长。
const CARD_ACTION_FADE: Duration = Duration::from_millis(120);
/// 两个 32px 操作按钮及其间距占用的宽度。
const CARD_ACTIONS_WIDTH: Pixels = px(68.);

impl NotesPage {
    /// 排序下拉：按更新时间 / 按创建时间 / 按名称。
    fn sort_menu(&self, weak: &WeakEntity<Self>) -> AnyElement {
        const OPTIONS: [(NoteSort, &str); 3] = [
            (NoteSort::UpdatedAt, "按更新时间"),
            (NoteSort::CreatedAt, "按创建时间"),
            (NoteSort::Title, "按名称"),
        ];
        let current = self.sort;
        let label = OPTIONS
            .iter()
            .find(|(sort, _)| *sort == current)
            .map(|(_, label)| *label)
            .unwrap_or("按更新时间");
        let weak = weak.clone();
        Button::new("notes-sort")
            .outline()
            .small()
            .icon(IconName::ArrowUpDown)
            .label(label)
            .dropdown_menu(move |mut menu, _, _| {
                for (sort, label) in OPTIONS {
                    let weak = weak.clone();
                    menu = menu.item(PopupMenuItem::new(label).checked(sort == current).on_click(
                        move |_, _, cx| {
                            let _ = weak.update(cx, |this, cx| {
                                if this.sort != sort {
                                    this.sort = sort;
                                    this.refresh(cx);
                                }
                            });
                        },
                    ));
                }
                menu
            })
            .into_any_element()
    }

    /// 筛选 Popover：分类（多选）+ 标签（多选）草稿，点击"完成筛选"才生效；
    /// 点击面板外关闭时草稿丢弃，恢复为已生效条件。
    ///
    /// 触发器是搜索框右侧的滑杆图标，面板按列表内容宽度固定（向下展开），
    /// 不随左栏拖动变宽。
    /// 面板内容是独立实体：页面重绘只复用元素，不克隆选项、不重建筛选树。
    fn filter_popover(&self, weak: &WeakEntity<Self>) -> Popover {
        let open = self.filter_open;
        let active_count = self.selected_categories.len() + self.selected_tags.len();
        let panel = self.filter_panel.clone();
        let open_weak = weak.clone();
        Popover::new("notes-filter")
            .anchor(Anchor::TopRight)
            .appearance(false)
            .open(open)
            .on_open_change(move |is_open, _, cx| {
                let _ = open_weak.update(cx, |this, cx| {
                    this.filter_open = *is_open;
                    this.reset_filter_draft(cx);
                });
            })
            .trigger(
                Button::new("notes-filter-btn")
                    .ghost()
                    .small()
                    .icon(IconName::SlidersHorizontal)
                    .selected(active_count > 0)
                    .tooltip("高级筛选"),
            )
            .content(move |_, _, _| panel.clone().into_any_element())
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
        let created = time::relative_created_at(note.created_at);
        let theme = cx.theme();
        // Fluent Light 卡片：白色列表底 + 极浅描边，悬停时描边加深；深色主题
        // 沿用 secondary 面与常规描边（list 底与页面同色，会失去卡片轮廓）。
        // 选中底色与左侧指示条保持主题 list_active 配色不变。
        let (surface, border) = if theme.mode.is_dark() {
            (theme.tokens.secondary.background, theme.border)
        } else {
            (theme.tokens.list.background, theme.border.opacity(0.55))
        };
        let selected_bg = Background::from(theme.list_active);
        let (hover_bg, border_hover, selection, muted) = (
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
        let delete = Button::new(("note-delete", note_id as u64))
            .custom(
                ButtonCustomVariant::new(cx)
                    .foreground(theme.danger)
                    .hover(theme.danger.opacity(0.12))
                    .active(theme.danger.opacity(0.2)),
            )
            .compact()
            .w(tokens::heights::CONTROL)
            .h(tokens::heights::CONTROL)
            .icon(IconName::Trash)
            .tooltip("删除")
            .on_click({
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
            .overflow_hidden()
            .gap_1()
            .px(tokens::spacing::MD)
            .py(tokens::spacing::XS)
            .rounded(tokens::radius::SM)
            .bg(if selected { selected_bg } else { surface })
            .border_1()
            .border_color(if selected {
                selection.opacity(0.35)
            } else if hovered {
                border_hover
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
            // 选中指示条：Fluent 左侧 3px Accent 色条，覆盖卡片整个左侧高度
            // （颜色随主题定义；卡片圆角由 overflow_hidden 裁切）
            .when(selected, |el| {
                el.child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .w(CARD_SELECTION_BAR)
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
                    .child(note_list::note_title(title, CARD_TITLE_FONT))
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
                el.child(note_list::note_summary(
                    summary,
                    CARD_SUMMARY_LINES,
                    CARD_SUMMARY_FONT,
                    muted,
                ))
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
                            .text_size(CARD_META_FONT)
                            .font_weight(TextRole::Meta.weight())
                            .line_height(TextRole::Meta.line_height())
                            .text_color(muted)
                            .child(created),
                    ),
            )
    }
}

impl Render for NotesPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.entity().downgrade();
        let (muted, danger) = {
            let theme = cx.theme();
            (theme.muted_foreground, theme.danger)
        };
        let (input_border_color, focus_border_color) = {
            let theme = cx.theme();
            (input_border(theme, false), input_border(theme, true))
        };
        let search_border = if self
            .query_input
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
        {
            focus_border_color
        } else {
            input_border_color
        };
        // 列表卡片使用固定字号，不随编辑器字号缩放
        let notes_list_width = cx.global::<SettingsStore>().0.read(cx).notes_list_width();
        let split_state = self.split_state.clone();

        // 列表容器顶部：搜索框（右侧内嵌筛选触发器）
        let search_row = div().flex_shrink_0().px_3().pt_2().child(
            div()
                // 点击搜索框之外的任意处即失焦
                .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                    if this
                        .query_input
                        .read(cx)
                        .focus_handle(cx)
                        .is_focused(window)
                    {
                        window.blur(cx);
                    }
                }))
                .child(
                    Input::new(&self.query_input)
                        .with_size(Size::Medium)
                        .w_full()
                        .text_field_focus(search_border)
                        .prefix(Icon::new(IconName::Search).with_size(Size::XSmall))
                        .suffix(self.filter_popover(&weak)),
                ),
        );

        // 搜索框下方：左排序、右新增笔记；与列表同容器，不设下边框
        let list_header = h_flex()
            .flex_shrink_0()
            .items_center()
            .justify_between()
            .gap_2()
            .px_3()
            .py_2()
            .child(self.sort_menu(&weak))
            .child(
                Button::new("notes-create")
                    .primary()
                    .small()
                    .icon(IconName::Plus)
                    .label("新增笔记")
                    .on_click({
                        let weak = weak.clone();
                        move |_, window, cx| {
                            let _ = weak.update(cx, |this, cx| this.create_note(window, cx));
                        }
                    }),
            );

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
                .gap_1()
                .px_3()
                .py_2()
                .overflow_y_scrollbar()
                .children(notes.iter().map(|note| {
                    let selected = self.selected == Some(note.id);
                    self.note_card(note, selected, &weak, cx)
                }))
                .into_any_element(),
        };

        // 左栏：搜索 / 排序 / 新增 + 列表（双栏组装，Phase 10）
        let left = v_flex()
            .size_full()
            .min_w_0()
            .child(search_row)
            .child(list_header)
            .child(list_area);

        // 左右双栏直接铺满内容区：搜索归左栏，模式切换归右栏编辑器标题行
        v_flex()
            .size_full()
            .child(
                // 双栏：分界线可拖动（左栏宽度取自设置，默认 320px，钳制 ≥240px）
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
            // 弹窗挂最后：无样式 Dialog 走 deferred 图层，覆盖整个窗口
            .when_some(self.render_taxonomy_modal(window, cx), |el, modal| {
                el.child(modal)
            })
    }
}
