//! 待办页视图构建：新建 / 编辑表单 / 待办条目 / 清单区 / render 组装。
//! 状态与交互逻辑（订阅 / 快照 / 表单提交）见 `mod.rs`。

use std::cell::Cell;
use std::rc::Rc;

use chrono::{Datelike, NaiveDate, Weekday};

use fragmenta_core::model::Todo;

use gpui_kit::assets::IconName;
use gpui_kit::base::{Calendar, CalendarItem, CalendarItemKind, CalendarItemState};
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants};
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
use crate::ui::tokens;

/// 日历单元格边长：kit 默认 32 放大到 40（对齐 kit Large 档几何），
/// 为右上角圆点留出空间。
const CALENDAR_CELL: Pixels = px(40.);
/// 待办圆点直径与距单元格右上角的内缩。
const CALENDAR_DOT: Pixels = px(4.);
const CALENDAR_DOT_INSET: Pixels = px(6.);

/// 由网格单元格的日号与邻月标记还原实际日期：前导日（日号 > 15）属上月尾、
/// 尾随日（日号 ≤ 15）属次月头——网格溢出至多 6 天，两段日号不相交。
fn visible_date(year: i32, month: u8, day: u32, adjacent: bool) -> NaiveDate {
    let (year, month) = if !adjacent {
        (year, month as u32)
    } else if day > 15 {
        if month == 1 {
            (year - 1, 12)
        } else {
            (year, month as u32 - 1)
        }
    } else if month == 12 {
        (year + 1, 1)
    } else {
        (year, month as u32 + 1)
    };
    NaiveDate::from_ymd_opt(year, month, day).expect("网格单元格日恒为合法日期")
}

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

    /// 左日历卡：base 层承载全部交互（点选 / 翻月 / 月年选择 / 键盘），
    /// 装修沿用 kit 日历开箱规则并放大单元格；自定义之处：周一为列首
    /// （「一」…「六」「日」）、月 / 年用阿拉伯数字、有待办的日子右上角加圆点。
    fn calendar_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let (year, month) = {
            let state = self.calendar.read(cx);
            (state.current_year(), state.current_month())
        };
        let theme = cx.theme();
        let (border, muted, foreground) = (theme.border, theme.muted_foreground, theme.foreground);
        let (accent, accent_foreground) = (theme.accent, theme.accent_foreground);
        let (primary, primary_foreground) = (theme.primary, theme.primary_foreground);
        let secondary_hover = theme.secondary_hover;
        let (cell_radius, card_radius) = (theme.radius * 2., theme.radius_lg);
        let marked = self.marked.clone();
        // base 对同一条目先调 label 定文案、后调 item 装修，日号经此传递；
        // 每格一步，仅成立单月显示（本页固定 1 个月）
        let day_of_item = Rc::new(Cell::new(0u32));
        let (label_day, item_day) = (day_of_item.clone(), day_of_item);

        Calendar::new(
            ("todos-calendar", self.calendar.entity_id()),
            &self.calendar,
        )
        .first_day_of_week(Weekday::Mon)
        .label(move |kind: CalendarItemKind, value: i32| -> SharedString {
            match kind {
                // 星期序号 0 = 周日 … 6 = 周六（base 的传值约定）
                CalendarItemKind::Weekday => match value {
                    0 => "日".into(),
                    1 => "一".into(),
                    2 => "二".into(),
                    3 => "三".into(),
                    4 => "四".into(),
                    5 => "五".into(),
                    _ => "六".into(),
                },
                CalendarItemKind::Month | CalendarItemKind::MonthToggle => {
                    format!("{value}月").into()
                }
                CalendarItemKind::Day => {
                    label_day.set(value as u32);
                    value.to_string().into()
                }
                _ => value.to_string().into(),
            }
        })
        .item(move |item: CalendarItem, st: CalendarItemState, _, _| {
            let kind = st.kind();
            let item = match kind {
                CalendarItemKind::Previous => item
                    .clear_children()
                    .child(Icon::new(IconName::ChevronLeft).size_4()),
                CalendarItemKind::Next => item
                    .clear_children()
                    .child(Icon::new(IconName::ChevronRight).size_4()),
                _ => item,
            };
            let has_todo = kind == CalendarItemKind::Day
                && marked.contains(&visible_date(year, month, item_day.get(), st.is_muted()));
            item.relative()
                .size(CALENDAR_CELL)
                .rounded(cell_radius)
                .flex()
                .items_center()
                .justify_center()
                .when(kind != CalendarItemKind::Weekday, |this| this.text_sm())
                .when(kind == CalendarItemKind::Weekday, |this| {
                    this.text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(muted)
                })
                .when(kind == CalendarItemKind::Month, |this| this.text_xs())
                .when(
                    matches!(
                        kind,
                        CalendarItemKind::MonthToggle | CalendarItemKind::YearToggle
                    ),
                    |this| this.text_sm().font_weight(FontWeight::MEDIUM),
                )
                .when(
                    matches!(
                        kind,
                        CalendarItemKind::Month | CalendarItemKind::MonthToggle
                    ),
                    |this| {
                        this.when(kind == CalendarItemKind::Month, |this| this.w_full().my_1())
                            .when(kind == CalendarItemKind::MonthToggle, |this| {
                                this.w_auto().px_2()
                            })
                    },
                )
                .when(
                    matches!(kind, CalendarItemKind::Year | CalendarItemKind::YearToggle),
                    |this| {
                        this.when(kind == CalendarItemKind::Year, |this| this.w_full().my_1())
                            .when(kind == CalendarItemKind::YearToggle, |this| {
                                this.w_auto().px_2()
                            })
                    },
                )
                .when(st.is_muted(), |this| {
                    this.text_color(muted)
                        .when(st.is_disabled(), |this| this.opacity(0.5))
                })
                .when(st.is_in_range(), |this| {
                    this.bg(accent).text_color(accent_foreground)
                })
                .when(
                    !st.is_active() && !st.is_disabled() && kind != CalendarItemKind::Weekday,
                    |this| this.hover(|style| style.bg(secondary_hover).text_color(foreground)),
                )
                .when(st.is_active(), |this| {
                    this.bg(primary).text_color(primary_foreground)
                })
                .when(st.is_today() && !st.is_active(), |this| {
                    this.bg(accent).text_color(accent_foreground)
                })
                .when(has_todo, |this| {
                    this.child(
                        div()
                            .absolute()
                            .top(CALENDAR_DOT_INSET)
                            .right(CALENDAR_DOT_INSET)
                            .size(CALENDAR_DOT)
                            .rounded_full()
                            .bg(if st.is_active() {
                                primary_foreground
                            } else {
                                primary
                            }),
                    )
                })
                .into_any_element()
        })
        .border_1()
        .border_color(border)
        .rounded(card_radius)
        .p_3()
        .gap_0p5()
        .into_any_element()
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
                                let _ = weak.update(cx, |this, cx| {
                                    this.refresh(cx);
                                    this.refresh_marked(cx);
                                });
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
                    .w(tokens::widths::CALENDAR)
                    .flex_shrink_0()
                    .p(tokens::spacing::MD)
                    .child(self.calendar_card(cx)),
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

#[cfg(test)]
mod tests {
    use super::visible_date;
    use chrono::NaiveDate;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    /// 邻月单元格按日号区间辨别方向：前导日归上月尾、尾随日归次月头，跨年正确。
    #[test]
    fn visible_date_resolves_adjacent_month_days() {
        assert_eq!(visible_date(2026, 10, 15, false), date(2026, 10, 15));
        assert_eq!(visible_date(2026, 10, 30, true), date(2026, 9, 30));
        assert_eq!(visible_date(2026, 1, 28, true), date(2025, 12, 28));
        assert_eq!(visible_date(2026, 10, 4, true), date(2026, 11, 4));
        assert_eq!(visible_date(2026, 12, 3, true), date(2027, 1, 3));
    }
}
