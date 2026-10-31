//! 待办页
//!
//! 数据流遵循：render 内不查询——清单为订阅（`CalendarEvent::Selected` /
//! `TodosEvent::Changed`）时拉取的页字段快照；新建 / 编辑共用同一 Dialog 表单。
//! 本文件承载状态与交互逻辑，视图构建见 `view.rs`

mod view;

use std::collections::HashMap;

use chrono::{Days, Local, NaiveDate, NaiveTime};

use fragmenta_core::model::Todo;

use gpui_kit::base::{CalendarEvent, CalendarState, Date};
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::input::InputState;
use gpui_kit::component::time_field::TimeFieldState;
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::settings::SettingsEvent;
use crate::settings::SettingsStore;
use crate::todos::TodosStore;
use crate::todos::{TodosEvent, TodosState};
use crate::ui::dialog_motion;
use crate::widgets::toast;

/// 日历圆点状态：当天仍有未完成待办，或当天待办已全部完成。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum DayMark {
    Pending,
    Done,
}

/// 新建 / 编辑共用的 Dialog 表单（每次打开为全新实体，对话框关闭即释放）。
///
/// Dialog builder 每帧经 WindowState 重新调用，读取本实体即建立观察，
/// 输入 / 勾选的任何变更都会重建内容与 footer（「保存」动态禁用依赖此机制）。
pub(crate) struct TodoForm {
    /// 编辑目标（None = 新建）；决定保存路径与对话框标题。
    editing: Option<Todo>,
    /// 归属日期：打开时锁定的日历选中日（表单内无日期字段，改归属日期的
    /// 产品路径 = 点日历对应日重新新增）。
    date: NaiveDate,
    /// 内容输入。
    content: Entity<InputState>,
    /// 是否指定时间区间（Checkbox 承载；TimeField 无空值状态）。
    has_interval: bool,
    /// 区间起止（新建默认 09:00–10:00，编辑按现状预填）。
    start: Entity<TimeFieldState>,
    end: Entity<TimeFieldState>,
}

/// 新建表单的默认区间（TimeField 默认 00:00 会导致 end ≤ start，给出合法初值）。
fn default_interval() -> (NaiveTime, NaiveTime) {
    (
        NaiveTime::from_hms_opt(9, 0, 0).expect("09:00 合法"),
        NaiveTime::from_hms_opt(10, 0, 0).expect("10:00 合法"),
    )
}

/// 待办页视图：日历 + 选中日清单，常驻于主窗口 shell。
pub(crate) struct TodosPage {
    todos: Entity<TodosState>,
    /// 左侧日历状态（初始选中今日）。
    calendar: Entity<CalendarState>,
    /// 当前选中日（清单归属日期）。
    selected: NaiveDate,
    /// 选中日清单快照：查询失败时存错误信息（错误占位 + 重试）。
    list: Result<Vec<Todo>, String>,
    /// 可见日历网格内有待办的日子及其圆点状态（单元格右上角圆点）。
    marked: HashMap<NaiveDate, DayMark>,
    /// `marked` 对应的可见月；base 每帧 notify，翻月才重拉。
    marked_month: (i32, u8),
    _calendar_sub: Subscription,
    _calendar_observe: Subscription,
    _changed_sub: Subscription,
    /// 编辑器字号变化 → 清单条目随 editor_font_px 重渲染。
    _settings_sub: Subscription,
}

impl TodosPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let todos = cx.global::<TodosStore>().0.clone();
        let today = Local::now().date_naive();
        let calendar = cx.new(|cx| {
            let mut state = CalendarState::new(window, cx);
            state.set_date(today, window, cx);
            state
        });

        // 日历选日 → 清单切换为该日（翻月 / 月年选择不触发，仅点选日期）
        let calendar_sub = cx.subscribe(&calendar, |this, _, event: &CalendarEvent, cx| {
            if let CalendarEvent::Selected(Date::Single(Some(date))) = event {
                this.selected = *date;
                this.refresh(cx);
            }
        });
        // 可见月变化（翻月 / 点选邻月日 / 月年选择）→ 重拉圆点矩阵；
        // base 每帧都会 set_number_of_months 触发 notify，故以可见月为键去重
        let calendar_observe = cx.observe(&calendar, |this, calendar, cx| {
            let key = {
                let state = calendar.read(cx);
                (state.current_year(), state.current_month())
            };
            if this.marked_month != key {
                this.refresh_marked(cx);
            }
        });
        // store 增删改 → 重拉选中日清单与圆点矩阵
        let changed_sub = cx.subscribe(&todos, |this, _, event: &TodosEvent, cx| {
            if matches!(event, TodosEvent::Changed) {
                this.refresh(cx);
                this.refresh_marked(cx);
            }
        });
        // 编辑器字号变化 → 清单重渲染（editor_font_px）
        let settings = cx.global::<SettingsStore>().0.clone();
        let settings_sub = cx.subscribe(&settings, |_, _, _: &SettingsEvent, cx| {
            cx.notify();
        });

        let mut this = Self {
            todos,
            calendar,
            selected: today,
            list: Ok(Vec::new()),
            marked: HashMap::new(),
            marked_month: (0, 0),
            _calendar_sub: calendar_sub,
            _calendar_observe: calendar_observe,
            _changed_sub: changed_sub,
            _settings_sub: settings_sub,
        };
        this.refresh(cx);
        this.refresh_marked(cx);
        this
    }

    /// 重拉选中日清单快照（render 外调用）。
    fn refresh(&mut self, cx: &mut Context<Self>) {
        let selected = self.selected;
        self.list = self
            .todos
            .read(cx)
            .todos_between(selected, selected)
            .map_err(|err| err.to_string());
        cx.notify();
    }

    /// 重拉圆点快照（render 外调用）：取可见月 ± 一周，覆盖网格溢出的邻月日；
    /// 同日多条待办合并为一种状态——任一未完成即为 `Pending`，否则 `Done`。
    /// 查询失败退化为无圆点（清单区错误占位已承载可见反馈）。
    fn refresh_marked(&mut self, cx: &mut Context<Self>) {
        let (year, month) = {
            let state = self.calendar.read(cx);
            (state.current_year(), state.current_month())
        };
        let first = NaiveDate::from_ymd_opt(year, month as u32, 1).expect("当前月首日合法");
        let last = if month == 12 {
            NaiveDate::from_ymd_opt(year + 1, 1, 1)
        } else {
            NaiveDate::from_ymd_opt(year, month as u32 + 1, 1)
        }
        .and_then(|next| next.pred_opt())
        .expect("次月首日前一日合法");
        let from = first
            .checked_sub_days(Days::new(7))
            .expect("前一周在支持范围内");
        let to = last
            .checked_add_days(Days::new(7))
            .expect("后一周在支持范围内");
        let mut marked = HashMap::new();
        if let Ok(todos) = self.todos.read(cx).todos_between(from, to) {
            for todo in todos {
                let mark = marked.entry(todo.date).or_insert(DayMark::Done);
                if !todo.done {
                    *mark = DayMark::Pending;
                }
            }
        }
        self.marked = marked;
        self.marked_month = (year, month);
        cx.notify();
    }

    /// 「今天」：日历跳回当月并选中今日，清单联动。
    fn go_today(&mut self, cx: &mut Context<Self>) {
        let today = Local::now().date_naive();
        self.calendar.update(cx, |state, cx| {
            if state.apply_date(Date::Single(Some(today))) {
                cx.notify();
            }
        });
        if self.selected != today {
            self.selected = today;
            self.refresh(cx);
        }
    }

    /// 「新建」：以日历当前选中日为归属日期打开空表单。
    fn open_create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_form(None, window, cx);
    }

    /// hover「编辑」：打开表单并预填已有内容与区间。
    fn open_edit(&mut self, todo: Todo, window: &mut Window, cx: &mut Context<Self>) {
        self.open_form(Some(todo), window, cx);
    }

    /// 打开新建 / 编辑共用表单：每次打开构建全新输入实体，互不残留。
    fn open_form(&mut self, editing: Option<Todo>, window: &mut Window, cx: &mut Context<Self>) {
        let date = editing.as_ref().map_or(self.selected, |todo| todo.date);
        let content_init = editing
            .as_ref()
            .map(|todo| todo.content.clone())
            .unwrap_or_default();
        let has_interval = editing.as_ref().is_some_and(|todo| todo.interval.is_some());
        let (start_init, end_init) = editing
            .as_ref()
            .and_then(|todo| todo.interval)
            .unwrap_or_else(default_interval);

        let content = cx.new(|cx| {
            let mut state = InputState::new(window, cx).placeholder("待办内容");
            state.set_value(content_init, window, cx);
            state
        });
        let start = cx.new(|cx| {
            let mut state = TimeFieldState::new(window, cx);
            state.set_time(start_init, window, cx);
            state
        });
        let end = cx.new(|cx| {
            let mut state = TimeFieldState::new(window, cx);
            state.set_time(end_init, window, cx);
            state
        });
        let form = cx.new(|_| TodoForm {
            editing,
            content,
            has_interval,
            start,
            end,
            date,
        });

        let weak = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, window, cx| {
            let is_edit = form.read(cx).editing.is_some();
            dialog_motion::kit_dialog(dialog)
                .title(if is_edit {
                    "编辑待办"
                } else {
                    "新建待办"
                })
                .width(px(380.))
                .centered()
                // 表单内的「指定时间区间」勾选框自行截获 Enter 切换勾选；
                // 其余焦点位置 Enter 等价于点击「保存」。
                .on_ok({
                    let form = form.clone();
                    let weak = weak.clone();
                    move |_, window, cx| {
                        submit_form(&form, &weak, window, cx);
                        // 成功时 submit_form 已异步关闭弹窗；失败或内容为空则保持打开
                        false
                    }
                })
                .content({
                    let form = form.clone();
                    move |content, window, cx| view::form_content(&form, content, window, cx)
                })
                .footer(view::form_footer(&form, &weak, window, cx))
        });
    }
}

/// 表单提交：内容必填（「保存」已禁用，双保险）；勾选区间时 `end <= start`
/// 拒绝（不支持跨天，跨天需求拆为两条待办）。
///
/// 成功后关闭对话框（`Changed` 驱动清单刷新）；失败以通知呈现并保留表单。
fn submit_form(
    form: &Entity<TodoForm>,
    weak: &WeakEntity<TodosPage>,
    window: &mut Window,
    cx: &mut App,
) {
    let (content, has_interval, start, end, editing, date) = {
        let form = form.read(cx);
        (
            form.content.read(cx).value().trim().to_string(),
            form.has_interval,
            form.start.read(cx).time(),
            form.end.read(cx).time(),
            form.editing.clone(),
            form.date,
        )
    };
    if content.is_empty() {
        return;
    }
    let interval = if has_interval {
        if end <= start {
            toast::warn("结束时间需晚于开始时间", window, cx);
            return;
        }
        Some((start, end))
    } else {
        None
    };

    let result = weak.update(cx, |page, cx| {
        page.todos.update(cx, |state, cx| match editing {
            None => state.add(&content, date, interval, cx).map(|_| ()),
            Some(mut todo) => {
                todo.content = content.clone();
                todo.interval = interval;
                state.update(&mut todo, cx)
            }
        })
    });
    match result {
        Ok(Ok(())) => window.close_dialog(cx),
        Ok(Err(err)) => toast::error(err.to_string(), window, cx),
        // 页面实体已释放（窗口关闭）：无处呈现，放弃
        Err(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use gpui_kit::component::WindowExt as _;
    use gpui_kit::component::dialog::Confirm;
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{
        AppContext as _, Context, FocusHandle, InteractiveElement as _, IntoElement,
        ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _, TestAppContext,
        Window, WindowOptions, div,
    };

    use crate::ui::dialog_motion;

    struct DialogProbe {
        checkbox: FocusHandle,
        other: FocusHandle,
    }

    impl Render for DialogProbe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full()
        }
    }

    /// 勾选框聚焦时，Enter 由就近的 `Confirm` 处理器接手，不落到弹窗 `on_ok`；
    /// 原生激活键已被 `Confirm` 吞掉，不应再次翻转。
    #[gpui_kit::test]
    fn enter_is_intercepted_by_focused_control_before_dialog_confirm(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let toggles = Rc::new(Cell::new(0));
        let saves = Rc::new(Cell::new(0));
        let activations = Rc::new(Cell::new(0));
        let (handle, probe) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
                cx.new(|cx| DialogProbe {
                    checkbox: cx.focus_handle(),
                    other: cx.focus_handle(),
                })
            })
            .unwrap()
        });

        cx.update_window(handle, {
            let probe = probe.clone();
            let toggles = toggles.clone();
            let saves = saves.clone();
            let activations = activations.clone();
            move |_, window, cx| {
                let checkbox = probe.read(cx).checkbox.clone();
                let other = probe.read(cx).other.clone();
                window.open_dialog(cx, move |dialog, _, _| {
                    let checkbox = checkbox.clone();
                    let other = other.clone();
                    let toggles = toggles.clone();
                    let saves = saves.clone();
                    let activations = activations.clone();
                    dialog_motion::kit_dialog(dialog)
                        .on_ok(move |_, _, _| {
                            saves.set(saves.get() + 1);
                            false
                        })
                        .content(move |content, _, _| {
                            let activations = activations.clone();
                            content
                                .child(
                                    div()
                                        .id("probe-guard")
                                        .on_action({
                                            let toggles = toggles.clone();
                                            move |_: &Confirm, _, _| {
                                                toggles.set(toggles.get() + 1);
                                            }
                                        })
                                        .child(
                                            div()
                                                .id("probe-target")
                                                .track_focus(&checkbox)
                                                .on_click(move |_, _, _| {
                                                    activations.set(activations.get() + 1);
                                                }),
                                        ),
                                )
                                .child(div().id("probe-outside").track_focus(&other))
                        })
                });
                window.render_frame(cx);
            }
        })
        .unwrap();

        cx.update_window(handle, |_, window, cx| {
            probe.read(cx).checkbox.clone().focus(window, cx);
            window.render_frame(cx);
        })
        .unwrap();

        cx.simulate_keystrokes(handle, "enter");

        assert_eq!(toggles.get(), 1, "Enter 由就近的勾选处理器接手");
        assert_eq!(saves.get(), 0, "勾选框聚焦时 Enter 不应保存");
        assert_eq!(
            activations.get(),
            0,
            "Confirm 已吞掉原生激活键，不应重复翻转"
        );

        // 焦点不在勾选框时，Enter 落到弹窗 on_ok（等价于点击「保存」）。
        cx.update_window(handle, |_, window, cx| {
            probe.read(cx).other.clone().focus(window, cx);
            window.render_frame(cx);
        })
        .unwrap();
        cx.simulate_keystrokes(handle, "enter");

        assert_eq!(saves.get(), 1, "其他焦点位置 Enter 触发保存");
        assert_eq!(toggles.get(), 1, "勾选框不再被翻转");
    }
}
