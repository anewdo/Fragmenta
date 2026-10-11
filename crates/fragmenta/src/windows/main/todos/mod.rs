//! 待办页（Phase 11）：左侧日历选日 + 右侧选中日清单。
//!
//! 数据流遵循架构铁律：render 内不查询——清单为订阅（`CalendarEvent::Selected` /
//! `TodosEvent::Changed`）时拉取的页字段快照；新建 / 编辑共用同一 Dialog 表单。
//! 本文件承载状态与交互逻辑（订阅 / 快照 / 表单提交），视图构建见 `view.rs`。

mod view;

use chrono::{Local, NaiveDate, NaiveTime};

use fragmenta_core::model::Todo;

use gpui_kit::base::{CalendarEvent, CalendarState, Date};
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::input::InputState;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::time_field::TimeFieldState;
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::app::TodosStore;
use crate::state::{TodosEvent, TodosState};

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
pub struct TodosPage {
    todos: Entity<TodosState>,
    /// 左侧日历状态（初始选中今日）。
    calendar: Entity<CalendarState>,
    /// 当前选中日（清单归属日期）。
    selected: NaiveDate,
    /// 选中日清单快照：查询失败时存错误信息（错误占位 + 重试）。
    list: Result<Vec<Todo>, String>,
    _calendar_sub: Subscription,
    _changed_sub: Subscription,
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
        // store 增删改 → 重拉选中日清单
        let changed_sub = cx.subscribe(&todos, |this, _, event: &TodosEvent, cx| {
            if matches!(event, TodosEvent::Changed) {
                this.refresh(cx);
            }
        });

        let mut this = Self {
            todos,
            calendar,
            selected: today,
            list: Ok(Vec::new()),
            _calendar_sub: calendar_sub,
            _changed_sub: changed_sub,
        };
        this.refresh(cx);
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
            dialog
                .title(if is_edit { "编辑待办" } else { "新建待办" })
                .width(px(380.))
                .content({
                    let form = form.clone();
                    move |content, _, cx| view::form_content(&form, content, cx)
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
            window.push_notification(Notification::error("结束时间需晚于开始时间"), cx);
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
        Ok(Err(err)) => window.push_notification(Notification::error(err.to_string()), cx),
        // 页面实体已释放（窗口关闭）：无处呈现，放弃
        Err(_) => {}
    }
}
