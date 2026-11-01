//! 磁贴视图构建：顶栏 + 笔记编辑 / 列表 / 今日待办（可收起的内联新增表单 + 行内删除）。
//! 状态与交互逻辑见 `mod.rs`。

use fragmenta_core::model::{Note, Todo};

use gpui_kit::assets::IconName;
use gpui_kit::component::animation::{EffectTransition, ease_in_out_cubic};
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui_kit::component::input::Input;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Selectable as _, Sizable as _, Size, h_flex,
    v_flex,
};
use gpui_kit::prelude::*;
use gpui_kit::*;
use std::time::Duration;

use super::{CORNER, SaveIndicator, TileMode, TileView};
use crate::settings::SettingsStore;
use crate::todos::form_fields;
use crate::ui::focus::{TextFieldExt as _, input_border};
use crate::ui::note_list;
use crate::ui::tokens::{self, TextRole, TextRoleExt as _};
use crate::utils::time;

const NOTE_SUMMARY_LINES: usize = 1;
/// 磁贴笔记行标题 / 摘要字号：与笔记页卡片保持一致（固定，不随小窗字号缩放）。
const TILE_NOTE_TITLE_FONT: Pixels = px(13.);
const TILE_NOTE_SUMMARY_FONT: Pixels = px(11.);
const TODO_CHECK_SIZE: Pixels = px(16.);
/// 新增待办表单收起 / 展开的高度动画时长（与主窗口侧栏收起一致）。
const TODO_FORM_TRANSITION: Duration = Duration::from_millis(200);

impl TileView {
    fn editor_content(&self, content_px: Pixels, border: Hsla, muted: Hsla) -> AnyElement {
        v_flex()
            .flex_1()
            .min_h_0()
            .child(
                h_flex()
                    .flex_shrink_0()
                    .min_h(tokens::heights::TITLE_ROW)
                    .items_center()
                    .overflow_hidden()
                    .border_b_1()
                    .border_color(border)
                    .child(
                        Input::new(&self.title)
                            .appearance(false)
                            .px(px(10.))
                            .font_weight(FontWeight::BOLD)
                            .h_auto()
                            .text_size(content_px),
                    ),
            )
            .child(
                div().flex_1().min_h_0().child(
                    self.source
                        .element()
                        .appearance(false)
                        .h_full()
                        .text_size(content_px),
                ),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .h_6()
                    .px_3()
                    .items_center()
                    .text_size(px(11.))
                    .text_color(muted)
                    .child(format!("{} 字", self.word_count)),
            )
            .into_any_element()
    }

    fn notes_content(&self, weak: &WeakEntity<Self>, window: &Window, cx: &App) -> AnyElement {
        let theme = cx.theme();
        let focused = self
            .query_input
            .read(cx)
            .focus_handle(cx)
            .is_focused(window);
        let search_border = input_border(theme, focused);
        let border = theme.border;
        let muted = theme.muted_foreground;
        let danger = theme.danger;
        let search = div()
            .flex_shrink_0()
            .p(tokens::spacing::SM)
            .border_b_1()
            .border_color(border)
            .child(
                Input::new(&self.query_input)
                    .with_size(Size::Medium)
                    .w_full()
                    .text_field_focus(search_border)
                    .prefix(Icon::new(IconName::Search).with_size(Size::XSmall)),
            );
        let list = match &self.note_list {
            Err(err) => v_flex()
                .flex_1()
                .min_h_0()
                .items_center()
                .justify_center()
                .gap(tokens::spacing::SM)
                .p(tokens::spacing::MD)
                .child(div().text_color(danger).child("加载笔记列表失败"))
                .child(
                    div()
                        .text_role(TextRole::Meta)
                        .text_color(muted)
                        .child(err.clone()),
                )
                .into_any_element(),
            Ok(notes) if notes.is_empty() => {
                let searching = !self.query_input.read(cx).value().is_empty();
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .items_center()
                    .justify_center()
                    .text_role(TextRole::Meta)
                    .text_color(muted)
                    .child(if searching {
                        "没有匹配的笔记"
                    } else {
                        "还没有笔记"
                    })
                    .into_any_element()
            }
            Ok(notes) => v_flex()
                .id("tile-notes-scroll")
                .flex_1()
                .min_h_0()
                .gap(tokens::spacing::XS)
                .p(tokens::spacing::SM)
                .overflow_y_scrollbar()
                .children(notes.iter().map(|note| self.note_row(note, weak, cx)))
                .into_any_element(),
        };
        v_flex()
            .flex_1()
            .min_h_0()
            .child(search)
            .child(list)
            .into_any_element()
    }

    fn note_row(&self, note: &Note, weak: &WeakEntity<Self>, cx: &App) -> Stateful<Div> {
        let note_id = note.id;
        let title = if note.title.is_empty() {
            "无标题".to_string()
        } else {
            note.title.clone()
        };
        let summary = note.summary();
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let hover = theme.list_hover;

        v_flex()
            .id(("tile-note", note_id as u64))
            .gap(tokens::spacing::XS)
            .px(tokens::spacing::SM)
            .py(tokens::spacing::SM)
            .rounded(tokens::radius::SM)
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .on_click({
                let weak = weak.clone();
                move |_, window, cx| {
                    let _ = weak.update(cx, |this, cx| this.open_note(note_id, window, cx));
                }
            })
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap(tokens::spacing::SM)
                    .child(note_list::note_title(title, TILE_NOTE_TITLE_FONT))
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_role(TextRole::Meta)
                            .text_color(muted)
                            .child(time::relative_created_at(note.created_at)),
                    ),
            )
            .when(!summary.is_empty(), |element| {
                element.child(note_list::note_summary(
                    summary,
                    NOTE_SUMMARY_LINES,
                    TILE_NOTE_SUMMARY_FONT,
                    muted,
                ))
            })
    }

    fn todos_content(
        &self,
        weak: &WeakEntity<Self>,
        window: &mut Window,
        cx: &App,
    ) -> AnyElement {
        let theme = cx.theme();
        let border = theme.border;
        let muted = theme.muted_foreground;
        let danger = theme.danger;
        let hover = theme.list_hover;

        // 内联新增表单（可收起）：「新增待办」头行点击展开 / 收起。内容常驻
        // 挂载（同主窗口侧栏收起手法），收起态由内容体 overflow_hidden 裁为
        // 0 高，清单占满余下空间；输入状态由表单实体持有，重新展开即恢复。
        // 高度在 0 ↔ 实测自然高之间以 EffectTransition 过渡，动画 id 携带
        // 翻转序号，递增即从头重播。自然高经渲染探针每帧回写视图状态，
        // 「区间行」显隐等高度变化即时生效。
        // 按钮禁用态每帧按内容重算——render 读表单实体即建立观察，输入变化
        // 自动重建。
        let add_disabled = self.todo_form.read(cx).content_empty(cx);
        let add = Button::new("tile-todo-add")
            .primary()
            .label("保存")
            .disabled(add_disabled)
            .on_click({
                let weak = weak.clone();
                move |_, window, cx| {
                    let _ = weak.update(cx, |this, cx| this.submit_todo(window, cx));
                }
            })
            .into_any_element();
        let form_open = self.todo_form_open;
        let measured = self.todo_form_height;
        let form_header = h_flex()
            .id("tile-todo-form-toggle")
            .flex_shrink_0()
            .items_center()
            .gap(tokens::spacing::SM)
            .px(tokens::spacing::SM)
            .h(tokens::heights::CONTROL)
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .on_click({
                let weak = weak.clone();
                move |_, window, cx| {
                    let _ = weak.update(cx, |this, cx| this.toggle_todo_form(window, cx));
                }
            })
            .child(
                Icon::new(if form_open {
                    IconName::ChevronDown
                } else {
                    IconName::ChevronRight
                })
                .with_size(Size::XSmall)
                .text_color(muted),
            )
            .child(div().text_role(TextRole::ControlLabel).child("新增待办"));
        // 高度探针：absolute 铺满内容容器（不占布局空间），prepaint 把表单
        // 自然高回写视图状态；值变化才 notify，稳定时不产生额外重绘。
        let probe_weak = weak.clone();
        let height_probe = canvas(
            move |bounds, _, cx| {
                let _ = probe_weak.update(cx, |this, cx| {
                    let height = bounds.size.height;
                    if height != this.todo_form_height {
                        // 首次测量把动画起点同步为实测高，随后挂载的高度
                        // 动画静止（from == to）
                        if this.todo_form_height == px(0.) {
                            this.todo_form_anim_from = height;
                        }
                        this.todo_form_height = height;
                        cx.notify();
                    }
                });
            },
            |_, _, _, _| {},
        )
        .absolute()
        .left_0()
        .top_0()
        .bottom_0()
        .w(px(1.));
        let form_body = v_flex().overflow_hidden().child(
            v_flex()
                .relative()
                .p(tokens::spacing::SM)
                .child(form_fields(&self.todo_form, Some(add), window, cx))
                .child(height_probe),
        );
        // 首次进入 Todos 模式时尚未测得内容高：不加高度约束，探针回写后
        // 转入动画路径
        let form_body = if form_open && measured == px(0.) {
            form_body.into_any_element()
        } else {
            let target = if form_open { measured } else { px(0.) };
            EffectTransition::new(TODO_FORM_TRANSITION)
                .ease(ease_in_out_cubic)
                .height(self.todo_form_anim_from, target)
                .apply(
                    form_body,
                    ("tile-todo-form-height", self.todo_form_anim_seq),
                )
                .into_any_element()
        };
        let form = v_flex()
            .flex_shrink_0()
            .border_b_1()
            .border_color(border)
            .child(form_header)
            .child(form_body);

        let list = match &self.todo_list {
            Err(err) => v_flex()
                .flex_1()
                .min_h_0()
                .items_center()
                .justify_center()
                .gap(tokens::spacing::SM)
                .p(tokens::spacing::MD)
                .child(div().text_color(danger).child("加载今日待办失败"))
                .child(
                    div()
                        .text_role(TextRole::Meta)
                        .text_color(muted)
                        .child(err.clone()),
                )
                .into_any_element(),
            Ok(todos) if todos.is_empty() => v_flex()
                .flex_1()
                .min_h_0()
                .items_center()
                .justify_center()
                .text_role(TextRole::Meta)
                .text_color(muted)
                .child("今天还没有待办")
                .into_any_element(),
            Ok(todos) => v_flex()
                .id("tile-todos-scroll")
                .flex_1()
                .min_h_0()
                .gap(tokens::spacing::XS)
                .p(tokens::spacing::SM)
                .overflow_y_scrollbar()
                .children(todos.iter().map(|todo| self.todo_row(todo, weak, cx)))
                .into_any_element(),
        };
        v_flex()
            .flex_1()
            .min_h_0()
            .child(form)
            .child(list)
            .into_any_element()
    }

    fn todo_row(&self, todo: &Todo, weak: &WeakEntity<Self>, cx: &App) -> AnyElement {
        let theme = cx.theme();
        let border = theme.border;
        let muted = theme.muted_foreground;
        let danger = theme.danger;
        let primary = theme.primary;
        let primary_foreground = theme.primary_foreground;
        let hover = theme.list_hover;
        let done = todo.done;
        let todo_id = todo.id;
        let group = format!("tile-todo-{}", todo.id);
        let content_px = cx.global::<SettingsStore>().0.read(cx).tile_font_px();
        let check = div()
            .flex_shrink_0()
            .size(TODO_CHECK_SIZE)
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .map(|element| {
                if done {
                    element.bg(primary).child(
                        Icon::new(IconName::Check)
                            .with_size(Size::XSmall)
                            .text_color(primary_foreground),
                    )
                } else {
                    element.border_1().border_color(border)
                }
            });
        // hover「删除」（红色）：与主窗口待办页同款自定义变体；点击不触发
        // 整行 check / uncheck（stop_propagation），直接落库删除。
        let delete = Button::new(("tile-todo-delete", todo.id as u64))
            .custom(
                ButtonCustomVariant::new(cx)
                    .foreground(danger)
                    .hover(danger.opacity(0.12))
                    .active(danger.opacity(0.2)),
            )
            .small()
            .w(tokens::heights::CONTROL)
            .h(tokens::heights::CONTROL)
            .icon(IconName::Trash)
            .opacity(0.)
            .group_hover(group.as_str(), |style| style.opacity(1.))
            .on_click({
                let weak = weak.clone();
                move |_, _, cx| {
                    cx.stop_propagation();
                    let _ = weak.update(cx, |this, cx| this.delete_todo(todo_id, cx));
                }
            });
        // 单行内容与区间同行，check 圆心与文字行中心对齐（items_center）；
        // 点击整行任意处即切换完成状态。
        h_flex()
            .id(("tile-todo", todo.id as u64))
            .group(group)
            .items_center()
            .gap(tokens::spacing::SM)
            .p(tokens::spacing::SM)
            .rounded(tokens::radius::SM)
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .on_click({
                let weak = weak.clone();
                let todo = todo.clone();
                move |_, window, cx| {
                    let _ = weak.update(cx, |this, cx| this.toggle_todo(todo.clone(), window, cx));
                }
            })
            .child(check)
            .child(note_list::single_line(todo.content.clone(), content_px).map(|element| {
                if done {
                    element.line_through().text_color(muted)
                } else {
                    element
                }
            }))
            .when_some(todo.interval, |element, (start, end)| {
                element.child(
                    div()
                        .flex_shrink_0()
                        .text_role(TextRole::Meta)
                        .text_color(muted)
                        .when(done, |element| element.line_through())
                        .child(time::todo_interval_label(start, end)),
                )
            })
            .child(delete)
            .into_any_element()
    }
}

impl Render for TileView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.entity().downgrade();
        let (background, border, muted_foreground, danger, title_bar) = {
            let theme = cx.theme();
            (
                theme.background,
                theme.border,
                theme.muted_foreground,
                theme.danger,
                theme.title_bar,
            )
        };
        // 内容阅读面：标题 / 正文随小窗字号缩放（辅助小字保持固定）
        let content_px = cx.global::<SettingsStore>().0.read(cx).tile_font_px();

        // 保存状态文案（兼拖拽区内容；区域内无交互元素，§9 条目 2）
        let status_label = match self.status {
            SaveIndicator::Untouched => None,
            SaveIndicator::Pending => Some("未保存"),
            SaveIndicator::Saved => Some("已保存"),
            SaveIndicator::Failed => Some("保存失败"),
        };

        let topbar = h_flex()
            .items_center()
            .gap_1()
            .px_2()
            .h_9()
            .flex_shrink_0()
            // 顶栏：以 title_bar token 着色（色系主题下为深调 chrome）
            .bg(title_bar)
            .border_b_1()
            .border_color(border)
            .child(
                Button::new("tile-new")
                    .ghost()
                    .compact()
                    .icon(IconName::Plus)
                    .tooltip("新建笔记")
                    .on_click({
                        let weak = weak.clone();
                        move |_, window, cx| {
                            let _ = weak.update(cx, |this, cx| this.new_draft(window, cx));
                        }
                    }),
            )
            .child(
                Button::new("tile-notes")
                    .ghost()
                    .compact()
                    .icon(IconName::List)
                    .selected(self.mode == TileMode::Notes)
                    .tooltip("笔记列表")
                    .on_click({
                        let weak = weak.clone();
                        move |_, window, cx| {
                            let _ = weak.update(cx, |this, cx| {
                                this.toggle_mode(TileMode::Notes, window, cx)
                            });
                        }
                    }),
            )
            .child(
                Button::new("tile-todos")
                    .ghost()
                    .compact()
                    .icon(IconName::CircleCheckBig)
                    .selected(self.mode == TileMode::Todos)
                    .tooltip("今日待办")
                    .on_click({
                        let weak = weak.clone();
                        move |_, window, cx| {
                            let _ = weak.update(cx, |this, cx| {
                                this.toggle_mode(TileMode::Todos, window, cx)
                            });
                        }
                    }),
            )
            .child(
                h_flex()
                    .flex_1()
                    .h_full()
                    .min_w(px(24.))
                    .justify_center()
                    .window_control_area(WindowControlArea::Drag)
                    .when_some(status_label, |el, label| {
                        el.child(
                            div()
                                .text_size(px(11.))
                                .text_color(if self.status == SaveIndicator::Failed {
                                    danger
                                } else {
                                    muted_foreground
                                })
                                .child(label),
                        )
                    }),
            )
            .child(
                Button::new("tile-settings")
                    .ghost()
                    .compact()
                    .icon(IconName::Settings)
                    .tooltip("打开主窗口")
                    .on_click(|_, _, cx| crate::windows::main::open(cx)),
            )
            .child(
                Button::new("tile-topmost")
                    .ghost()
                    .compact()
                    .icon(if self.topmost {
                        IconName::PinOff
                    } else {
                        IconName::Pin
                    })
                    .tooltip(if self.topmost {
                        "取消置顶"
                    } else {
                        "置顶"
                    })
                    .on_click({
                        let weak = weak.clone();
                        move |_, window, cx| {
                            let _ = weak.update(cx, |this, cx| this.toggle_topmost(window, cx));
                        }
                    }),
            )
            .child(
                Button::new("tile-close")
                    .ghost()
                    .compact()
                    .icon(IconName::X)
                    .tooltip("关闭")
                    .on_click({
                        let weak = weak.clone();
                        move |_, window, cx| {
                            let _ = weak.update(cx, |this, cx| this.request_close(window, cx));
                        }
                    }),
            );

        let content = match self.mode {
            TileMode::Editor => self.editor_content(content_px, border, muted_foreground),
            TileMode::Notes => self.notes_content(&weak, window, cx),
            TileMode::Todos => self.todos_content(&weak, window, cx),
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(background)
            .rounded(px(CORNER))
            .border_1()
            .border_color(border)
            .overflow_hidden()
            .child(topbar)
            .child(content)
            // toast 浮层：deferred 图层，盖过弹窗与弹层（架构 §5.10）
            .child(self.toast.clone())
    }
}
