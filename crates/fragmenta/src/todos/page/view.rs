//! 待办页视图构建：新建 / 编辑表单 / 待办条目 / 清单区 / render 组装。
//! 状态与交互逻辑（订阅 / 快照 / 表单提交）见 `mod.rs`。

use chrono::Datelike;

use fragmenta_core::model::Todo;

use gpui_kit::assets::IconName;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::calendar::Calendar;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::DialogContent;
use gpui_kit::component::empty::{
    Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyMediaVariant, EmptyTitle,
};
use gpui_kit::component::input::Input;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::time_field::TimeField;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Sizable, Size, h_flex, v_flex,
};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{TodoForm, TodosPage};
use crate::settings::SettingsStore;

/// Dialog 表单内容：内容输入 + 「指定时间区间」勾选 + 起止时间（勾选时显示）。
///
/// 每帧经 builder 重建：读取表单实体即建立观察，勾选状态变化立即
/// 反映到 TimeField 行的显隐。
pub(super) fn form_content(
    form: &Entity<TodoForm>,
    content: DialogContent,
    cx: &mut App,
) -> DialogContent {
    let (has_interval, input, start, end) = {
        let form = form.read(cx);
        (
            form.has_interval,
            form.content.clone(),
            form.start.clone(),
            form.end.clone(),
        )
    };
    content.child(
        v_flex()
            .gap_2()
            .child(Input::new(&input).id("todo-form-content").w_full())
            .child({
                let form = form.clone();
                Checkbox::new("todo-form-interval")
                    .label("指定时间区间")
                    .checked(has_interval)
                    .on_change(move |value, _, cx| {
                        // notify 触发对话框层重建：勾选视觉、TimeField 行显隐
                        // 都由每帧重建的 builder 从表单实体重新读取
                        form.update(cx, |form, cx| {
                            form.has_interval = *value;
                            cx.notify();
                        });
                    })
            })
            .when(has_interval, |el| {
                el.child(
                    h_flex()
                        .id("todo-form-interval-row")
                        .test_support()
                        .gap_2()
                        .child(TimeField::new(&start).flex_1())
                        .child(TimeField::new(&end).flex_1()),
                )
            }),
    )
}

/// Dialog 自定义 footer：「取消」+「保存」。
///
/// builder 每帧重调本函数，按当前内容动态禁用「保存」（内容为空时）；
/// 取消 / ESC / 遮罩关闭均走 kit 默认语义。
pub(super) fn form_footer(
    form: &Entity<TodoForm>,
    weak: &WeakEntity<TodosPage>,
    _window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let content_empty = form.read(cx).content.read(cx).value().trim().is_empty();
    h_flex()
        .justify_end()
        .gap_2()
        .child(
            Button::new("todo-form-cancel")
                .label("取消")
                .on_click(|_, window, cx| {
                    window.close_dialog(cx);
                }),
        )
        .child(
            Button::new("todo-form-save")
                .primary()
                .label("保存")
                .disabled(content_empty)
                .on_click({
                    let form = form.clone();
                    let weak = weak.clone();
                    move |_, window, cx| super::submit_form(&form, &weak, window, cx)
                }),
        )
        .into_any_element()
}

impl TodosPage {
    /// 单条待办：圆形 check + 内容（可多行）+ 可选区间行 + hover 编辑 / 删除。
    ///
    /// 完成态：check 实心、内容与区间均加删除线；全天（无区间）条目只有内容行。
    #[allow(clippy::too_many_arguments)]
    fn todo_item(
        &self,
        todo: &Todo,
        weak: &WeakEntity<Self>,
        secondary: Hsla,
        border: Hsla,
        muted: Hsla,
        primary: Hsla,
        primary_foreground: Hsla,
    ) -> AnyElement {
        let id = todo.id;
        let done = todo.done;
        let group = format!("todo-{id}");

        // 圆形 check：点击翻转完成态
        let check = div()
            .id(("todo-check", id as u64))
            .flex_shrink_0()
            .size_4()
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .map(|el| {
                if done {
                    el.bg(primary).child(
                        Icon::new(IconName::Check)
                            .with_size(Size::XSmall)
                            .text_color(primary_foreground),
                    )
                } else {
                    el.border_1().border_color(border)
                }
            })
            .on_click({
                let weak = weak.clone();
                let todo = todo.clone();
                move |_, window, cx| {
                    let _ = weak.update(cx, |page, cx| {
                        let result = page.todos.update(cx, |state, cx| state.toggle(&todo, cx));
                        if let Err(err) = result {
                            window.push_notification(Notification::error(err.to_string()), cx);
                        }
                    });
                }
            });

        // 内容行：check + 内容 + hover 编辑 / 删除
        let content_row = h_flex()
            .items_start()
            .gap_2()
            .child(check)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(13.))
                    .map(|el| {
                        if done {
                            el.line_through().text_color(muted)
                        } else {
                            el
                        }
                    })
                    .child(todo.content.clone()),
            )
            .child(
                Button::new(("todo-edit", id as u64))
                    .ghost()
                    .compact()
                    .xsmall()
                    .icon(IconName::PenLine)
                    .opacity(0.)
                    .group_hover(group.as_str(), |style| style.opacity(1.))
                    .on_click({
                        let weak = weak.clone();
                        let todo = todo.clone();
                        move |_, window, cx| {
                            let _ = weak
                                .update(cx, |page, cx| page.open_edit(todo.clone(), window, cx));
                        }
                    }),
            )
            .child(
                Button::new(("todo-delete", id as u64))
                    .ghost()
                    .compact()
                    .xsmall()
                    .icon(IconName::Trash)
                    .opacity(0.)
                    .group_hover(group.as_str(), |style| style.opacity(1.))
                    .on_click({
                        let weak = weak.clone();
                        move |_, _, cx| {
                            let _ = weak.update(cx, |page, cx| {
                                page.todos.update(cx, |state, cx| state.delete(id, cx))
                            });
                        }
                    }),
            );

        v_flex()
            .id(("todo-item", id as u64))
            .test_support()
            .group(group)
            .gap_1()
            .p_2()
            .rounded(px(6.))
            .hover(move |style| style.bg(secondary))
            .child(content_row)
            // 区间行：左缩进对齐内容（16px check + 8px gap）
            .when_some(todo.interval, |el, (start, end)| {
                el.child(
                    div()
                        .pl(px(24.))
                        .text_size(px(12.))
                        .text_color(muted)
                        .when(done, |el| el.line_through())
                        .child(format!(
                            "{} 至 {}",
                            start.format("%H:%M"),
                            end.format("%H:%M")
                        )),
                )
            })
            .into_any_element()
    }

    /// 副标题日期文案：`10月11日 周六`。
    fn date_subtitle(&self) -> String {
        let weekday = match self.selected.weekday().num_days_from_monday() {
            0 => "周一",
            1 => "周二",
            2 => "周三",
            3 => "周四",
            4 => "周五",
            5 => "周六",
            _ => "周日",
        };
        format!(
            "{}月{}日 {weekday}",
            self.selected.month(),
            self.selected.day()
        )
    }
}

impl Render for TodosPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.entity().downgrade();
        let (secondary, border, muted, danger, primary, primary_foreground) = {
            let theme = cx.theme();
            (
                theme.secondary,
                theme.border,
                theme.muted_foreground,
                theme.danger,
                theme.primary,
                theme.primary_foreground,
            )
        };
        // 内容阅读面：清单条目随正文字号档缩放（日历与辅助小字保持固定）
        let content_px = cx.global::<SettingsStore>().0.read(cx).content_font_px();

        // 标题行：图标 + 标题 + 「今天」/「新建」
        let header = h_flex()
            .flex_shrink_0()
            .items_center()
            .gap_2()
            .h_12()
            .px_4()
            .border_b_1()
            .border_color(border)
            .child(Icon::new(IconName::SquareCheckBig))
            .child(div().font_weight(FontWeight::MEDIUM).child("待办事项清单"))
            .child(div().flex_1())
            .child(
                Button::new("todos-today")
                    .outline()
                    .small()
                    .icon(IconName::CalendarDays)
                    .label("今天")
                    .on_click({
                        let weak = weak.clone();
                        move |_, _, cx| {
                            let _ = weak.update(cx, |this, cx| this.go_today(cx));
                        }
                    }),
            )
            .child(
                Button::new("todos-create")
                    .primary()
                    .small()
                    .icon(IconName::Plus)
                    .label("新建")
                    .on_click({
                        let weak = weak.clone();
                        move |_, window, cx| {
                            let _ = weak.update(cx, |this, cx| this.open_create(window, cx));
                        }
                    }),
            );

        let subtitle = div()
            .flex_shrink_0()
            .px_4()
            .py_2()
            .text_size(px(12.))
            .text_color(muted)
            .child(self.date_subtitle());

        // 清单区：错误占位（含重试）/ 空态 / 条目列表
        let list_area = match &self.list {
            Err(err) => v_flex()
                .flex_1()
                .min_h_0()
                .items_center()
                .justify_center()
                .gap_2()
                .child(div().text_color(danger).child("加载待办清单失败"))
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(muted)
                        .child(err.clone()),
                )
                .child(
                    Button::new("todos-retry")
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
            Ok(todos) if todos.is_empty() => v_flex()
                .flex_1()
                .min_h_0()
                .child(
                    Empty::new().header(
                        EmptyHeader::new()
                            .media(
                                EmptyMedia::new()
                                    .with_variant(EmptyMediaVariant::Icon)
                                    .child(Icon::new(IconName::CalendarDays)),
                            )
                            .title(EmptyTitle::new().child("这一天还没有待办"))
                            .description(
                                EmptyDescription::new().child("点击右上角「新建」添加一条"),
                            ),
                    ),
                )
                .into_any_element(),
            Ok(todos) => v_flex()
                .flex_1()
                .min_h_0()
                .gap_1()
                .px_3()
                .py_2()
                .overflow_y_scrollbar()
                .text_size(content_px)
                .children(todos.iter().map(|todo| {
                    self.todo_item(
                        todo,
                        &weak,
                        secondary,
                        border,
                        muted,
                        primary,
                        primary_foreground,
                    )
                }))
                .into_any_element(),
        };

        // 左日历（选中日 = 清单归属日）+ 右清单。
        // 页面级双栏须用 stretch 布局：kit `h_flex` 附带 items_center 会把
        // 两栏在剩余高度内垂直居中，右栏内容整体悬空下沉。
        div()
            .flex()
            .size_full()
            .child(
                v_flex()
                    // 272 = kit 日历卡默认宽 248 + p_3 两侧留白 24
                    .w(px(272.))
                    .flex_shrink_0()
                    .p_3()
                    .child(Calendar::new(&self.calendar)),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(header)
                    .child(subtitle)
                    .child(list_area),
            )
    }
}
