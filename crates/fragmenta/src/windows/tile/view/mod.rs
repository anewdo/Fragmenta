//! 磁贴视图状态与交互逻辑：防抖保存闭环、模式切换、数据刷新与窗口操作。
//! 视图构建见 `render.rs`
//! headless 测试见 `tests.rs`

mod render;

#[cfg(test)]
mod tests;

use std::time::Duration;

use chrono::{Local, NaiveDate, TimeZone};
use fragmenta_core::model::{Note, NoteFilter, Todo};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::editor::ImageStore;
use crate::editor::SourceText;
use crate::notes::NotesStore;
use crate::notes::{NoteDraft, NoteKey, NotesEvent, NotesState, SaveStatus};
use crate::settings::SettingsEvent;
use crate::settings::SettingsStore;
use crate::todos::{TodoForm, TodoFormError, TodosEvent, TodosState, TodosStore};
use crate::widgets::toast::{self, ToastState, ToastStore};
use crate::win32;

use super::CORNER;

/// 边缘热区条宽（逻辑像素）：WM_NCHITTEST 子类化的命中带宽。
const EDGE_HOT: f32 = 6.5;
/// 角部热区边长（逻辑像素）：对角 resize 命中区。
const CORNER_HOT: f32 = 14.;

/// 顶栏保存状态（区别于 state 层 `SaveStatus`：补充 UI 侧"未触碰 / 防抖中"两态）。
#[derive(Clone, Copy, PartialEq, Eq)]
enum SaveIndicator {
    /// 尚无任何输入。
    Untouched,
    /// 有输入待落库（防抖计时段）。
    Pending,
    /// 最近一次落库成功。
    Saved,
    /// 最近一次落库失败；持续显示直至再次成功。
    Failed,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TileMode {
    Editor,
    Notes,
    Todos,
}

/// 磁贴视图：顶栏操作 + 标题 / 正文输入 + 防抖保存闭环。
pub(super) struct TileView {
    /// 防抖槽身份：未落库草稿为 `Draft`(自身实体 id)，首存成功后切换为 `Note(id)`。
    key: NoteKey,
    notes: Entity<NotesState>,
    todos: Entity<TodosState>,
    /// 内联新增待办表单（Todos 模式顶部常驻）：随视图创建一次，模式切换间
    /// 输入保留；归属日期随跨零点刷新对齐"今天"。
    todo_form: Entity<TodoForm>,
    /// 新增表单展开 / 收起：收起时只留「新增待办」头行，展开态跨模式切换保留。
    todo_form_open: bool,
    /// 表单内容自然高（渲染探针每帧回写）：展开动画的目标高；「区间行」
    /// 显隐等高度变化经它即时反映。
    todo_form_height: Pixels,
    /// 高度动画起点：翻转时取当前可见高（展开态 = 自然高，收起态 = 0）；
    /// 重进 Todos 模式时对齐目标高，令重新挂载的动画静止（from == to）。
    todo_form_anim_from: Pixels,
    /// 高度动画序号：每次翻转递增，动画从头重播。
    todo_form_anim_seq: u64,
    title: Entity<InputState>,
    source: SourceText,
    mode: TileMode,
    query_input: Entity<InputState>,
    note_list: Result<Vec<Note>, String>,
    todo_list: Result<Vec<Todo>, String>,
    todo_date: NaiveDate,
    /// toast 浮层渲染载体：App 级共享实体，仅渲染本窗口触发的条目。
    toast: Entity<ToastState>,
    midnight_task: Option<Task<()>>,
    topmost: bool,
    status: SaveIndicator,
    word_count: usize,
    /// 最近一次圆角区域参数 (宽, 高, scale)：未变则跳过重设。
    region: Option<(f32, f32, f32)>,
    _title_sub: Subscription,
    _word_sub: Subscription,
    _query_sub: Subscription,
    _saved_sub: Subscription,
    _notes_changed_sub: Subscription,
    _todos_changed_sub: Subscription,
    _bounds_sub: Subscription,
    /// 小窗字号变化 → 标题 / 正文随 tile_font_px 重渲染。
    _settings_sub: Subscription,
    _appearance_sub: Subscription,
}

impl TileView {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let notes = cx.global::<NotesStore>().0.clone();
        let todos = cx.global::<TodosStore>().0.clone();
        let toast = cx.global::<ToastStore>().0.clone();
        let source = SourceText::new(
            cx.global::<ImageStore>().0.clone(),
            "写点什么……",
            window,
            cx,
        );
        let title = cx.new(|cx| InputState::new(window, cx).placeholder("标题"));
        let query_input = cx.new(|cx| InputState::new(window, cx).placeholder("搜索标题或内容…"));
        let todo_form = TodoForm::create(Local::now().date_naive(), window, cx);

        let title_sub = cx.subscribe(&title, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.dispatch_save(cx);
            }
        });
        // 字数回调兼作正文保存钩子：正文 Change → 更新字数 + 提交防抖
        let word_sub = source.on_word_count(cx, |this, count, cx| {
            this.word_count = count;
            this.dispatch_save(cx);
        });
        let query_sub = cx.subscribe(
            &query_input,
            |this, _, event: &InputEvent, cx| match event {
                InputEvent::Change => this.refresh_notes(cx),
                InputEvent::Focus | InputEvent::Blur => cx.notify(),
                _ => {}
            },
        );
        let saved_sub = cx.subscribe(&notes, |this, _, event: &NotesEvent, cx| {
            if let NotesEvent::Saved { key, id, status } = event
                && key == &this.key
            {
                match status {
                    SaveStatus::Saved => {
                        this.status = SaveIndicator::Saved;
                        if let NoteKey::Draft(_) = &this.key {
                            this.promote(*id, cx);
                        }
                    }
                    SaveStatus::Failed => this.status = SaveIndicator::Failed,
                }
                cx.notify();
            }
        });
        let notes_changed_sub = cx.subscribe(&notes, |this, _, event: &NotesEvent, cx| {
            if matches!(event, NotesEvent::Changed) && this.mode == TileMode::Notes {
                this.refresh_notes(cx);
            }
        });
        let todos_changed_sub = cx.subscribe(&todos, |this, _, event: &TodosEvent, cx| {
            if matches!(event, TodosEvent::Changed) && this.mode == TileMode::Todos {
                this.refresh_todos(cx);
            }
        });

        // 八向边缘 resize：WM_NCHITTEST 子类化补齐框架缺口（隐藏标题栏时
        // 框架只算顶边），命中后由系统原生模态循环接管；最小尺寸经
        // window_min_size 由后端 WM_GETMINMAXINFO 原生钳制
        let _ = win32::install_edge_subclass(window, EDGE_HOT, CORNER_HOT);
        // 圆角窗口区域（失败不阻断：界面层圆角描边兜底）
        let _ = win32::set_rounded_region(window, CORNER);
        // 窗口 bounds 变化（用户 resize / 拖动 / 跨显示器 scale 变化）时重设区域，
        // 并把调整后的尺寸记入设置（下次呼出沿用，防抖写盘）
        let bounds_sub = cx.observe_window_bounds(window, |this, window, cx| {
            this.refresh_rounded_region(window);
            this.record_size(window, cx);
        });

        let settings = cx.global::<SettingsStore>().0.clone();
        let appearance_sub =
            settings.update(cx, |state, cx| state.attach_appearance_bridge(window, cx));
        // 小窗字号变化 → 标题 / 正文重渲染（tile_font_px）
        let settings_sub = cx.subscribe(&settings, |_, _, _: &SettingsEvent, cx| {
            cx.notify();
        });
        let weak = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            let closing = weak
                .update(cx, |this, cx| this.can_close(cx))
                .unwrap_or(true);
            if closing {
                super::open::forget(window.window_handle());
            }
            closing
        });

        // 焦点落标题：热键呼出后立即输入标题
        let focus = title.read(cx).focus_handle(cx);
        window.focus(&focus, cx);

        Self {
            key: NoteKey::Draft(cx.entity().entity_id()),
            notes,
            todos,
            todo_form,
            todo_form_open: true,
            todo_form_height: px(0.),
            todo_form_anim_from: px(0.),
            todo_form_anim_seq: 0,
            title,
            source,
            mode: TileMode::Editor,
            query_input,
            note_list: Ok(Vec::new()),
            todo_list: Ok(Vec::new()),
            todo_date: Local::now().date_naive(),
            toast,
            midnight_task: None,
            topmost: true, // PopUp 默认携带 WS_EX_TOPMOST（spike ④ 验证）
            status: SaveIndicator::Untouched,
            word_count: 0,
            region: None,
            _title_sub: title_sub,
            _word_sub: word_sub,
            _query_sub: query_sub,
            _saved_sub: saved_sub,
            _notes_changed_sub: notes_changed_sub,
            _todos_changed_sub: todos_changed_sub,
            _bounds_sub: bounds_sub,
            _settings_sub: settings_sub,
            _appearance_sub: appearance_sub,
        }
    }

    /// 提交防抖保存：每次变更以全量内容重建该 key 的计时（ADR-0001）。
    ///
    /// 空草稿（未晋升）无保存价值：取消旧槽并回到未触碰态，免于落出空笔记
    /// 或过期内容；已晋升为 `Note(id)` 的笔记允许存空（"已落库不删"）。
    fn dispatch_save(&mut self, cx: &mut Context<Self>) {
        let title = self.title.read(cx).value().to_string();
        let content = self.source.value(cx);
        if title.is_empty()
            && content.is_empty()
            && let NoteKey::Draft(_) = &self.key
        {
            let key = self.key.clone();
            self.notes.update(cx, |state, _| state.cancel(&key));
            self.status = SaveIndicator::Untouched;
            cx.notify();
            return;
        }
        let key = self.key.clone();
        self.notes.update(cx, |state, cx| {
            state.save(
                NoteDraft {
                    key,
                    title,
                    content,
                },
                cx,
            )
        });
        // 保存失败持续提示直至再次成功，重试期间不降级为"未保存"
        if self.status != SaveIndicator::Failed {
            self.status = SaveIndicator::Pending;
        }
        cx.notify();
    }

    /// 草稿晋升：记录笔记 id 并把防抖槽身份切换为 `Note(id)`（§5.6 id 回填闭环）。
    ///
    /// 落库完成的消息派发若与输入交错，旧 `Draft` 槽可能已带过期内容重建计时，
    /// 到期会以 Draft 身份插入重复笔记；晋升时 cancel 旧槽并按当前全量内容
    /// 重新入队，掐断该路径。
    fn promote(&mut self, id: i64, cx: &mut Context<Self>) {
        let draft_key = std::mem::replace(&mut self.key, NoteKey::Note(id));
        let title = self.title.read(cx).value().to_string();
        let content = self.source.value(cx);
        let key = self.key.clone();
        self.notes.update(cx, |state, cx| {
            state.cancel(&draft_key);
            state.save(
                NoteDraft {
                    key,
                    title,
                    content,
                },
                cx,
            );
        });
    }

    /// 按钮与原生关闭共用判断：空磁贴取消待写槽，非空先同步保存，失败则留窗。
    fn can_close(&mut self, cx: &mut Context<Self>) -> bool {
        let title = self.title.read(cx).value().to_string();
        let content = self.source.value(cx);
        if title.is_empty() && content.is_empty() {
            let key = self.key.clone();
            self.notes.update(cx, |state, _| state.cancel(&key));
            true
        } else {
            let key = self.key.clone();
            let status = self.notes.update(cx, |state, cx| state.flush_now(key, cx));
            if status == Some(SaveStatus::Failed) {
                self.status = SaveIndicator::Failed;
                cx.notify();
                false
            } else {
                true
            }
        }
    }

    fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.can_close(cx) {
            super::open::forget(window.window_handle());
            window.remove_window();
        }
    }

    /// 置顶切换：PopUp 自带置顶，取消 / 恢复须 `SetWindowPos`；失败时状态回滚。
    fn toggle_topmost(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let target = !self.topmost;
        if win32::set_topmost(window, target) {
            self.topmost = target;
            cx.notify();
        }
    }

    /// 圆角区域重设：仅尺寸 / scale 变化时执行（origin-only 移动跳过，
    /// 免于每次窗口拖动都全窗口重绘）。
    fn refresh_rounded_region(&mut self, window: &mut Window) {
        let size = window.viewport_size();
        let scale = window.scale_factor();
        let dims = (size.width.as_f32(), size.height.as_f32(), scale);
        if self.region == Some(dims) {
            return;
        }
        self.region = Some(dims);
        let _ = win32::set_rounded_region(window, CORNER);
    }

    /// 磁贴尺寸记忆：把当前窗口尺寸提交设置层（防抖落盘），下次呼出
    /// 的新磁贴按此尺寸打开。拖动（仅位移）与跨显示器 scale 变化不改
    /// 变逻辑尺寸，设置层同值跳过。
    fn record_size(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let size = window.viewport_size();
        // bounds 观察在窗口销毁阶段仍会触发一次，此时 viewport_size 可能
        // 已被置 0；这类无效尺寸不能覆盖用户调整过的尺寸记忆。
        if size.width.as_f32() <= 0. || size.height.as_f32() <= 0. {
            return;
        }
        let settings = cx.global::<SettingsStore>().0.clone();
        settings.update(cx, |state, cx| {
            state.set_tile_size(size.width, size.height, cx)
        });
    }

    fn new_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let title = self.title.read(cx).value().to_string();
        let content = self.source.value(cx);
        if !title.is_empty() || !content.is_empty() {
            let key = self.key.clone();
            let status = self.notes.update(cx, |state, cx| state.flush_now(key, cx));
            if status == Some(SaveStatus::Failed) {
                self.mode = TileMode::Editor;
                self.status = SaveIndicator::Failed;
                self.focus_editor(window, cx);
                cx.notify();
                return;
            }
        }
        let previous = self.key.clone();
        self.notes.update(cx, |state, _| state.cancel(&previous));
        self.key = NoteKey::Draft(cx.entity().entity_id());
        self.title
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.source.set_value("", window, cx);
        self.word_count = 0;
        self.status = SaveIndicator::Untouched;
        self.mode = TileMode::Editor;
        self.midnight_task = None;
        self.focus_title(window, cx);
        cx.notify();
    }

    fn toggle_mode(&mut self, target: TileMode, window: &mut Window, cx: &mut Context<Self>) {
        let next = if self.mode == target {
            TileMode::Editor
        } else {
            target
        };
        if self.mode == TileMode::Todos {
            self.midnight_task = None;
        }
        self.mode = next;
        match next {
            TileMode::Editor => self.focus_editor(window, cx),
            TileMode::Notes => {
                self.refresh_notes(cx);
                let focus = self.query_input.read(cx).focus_handle(cx);
                window.focus(&focus, cx);
            }
            TileMode::Todos => {
                self.refresh_todos(cx);
                // 动画状态随内容卸载丢弃：重进时起点对齐当前可见高，
                // 挂载帧静止呈现（from == to，不重播动画）
                self.todo_form_anim_from = if self.todo_form_open {
                    self.todo_form_height
                } else {
                    px(0.)
                };
                // 切入即录入：表单展开时焦点落新增表单的内容输入框
                if self.todo_form_open {
                    let focus = self.todo_form.read(cx).content_focus(cx);
                    window.focus(&focus, cx);
                }
            }
        }
        cx.notify();
    }

    fn focus_title(&self, window: &mut Window, cx: &mut App) {
        let focus = self.title.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
    }

    fn focus_editor(&self, window: &mut Window, cx: &mut App) {
        let focus = self.source.textarea().read(cx).focus_handle(cx);
        window.focus(&focus, cx);
    }

    fn refresh_notes(&mut self, cx: &mut Context<Self>) {
        let filter = NoteFilter {
            query: self.query_input.read(cx).value().to_string(),
            ..NoteFilter::default()
        };
        self.note_list = self
            .notes
            .read(cx)
            .notes(&filter)
            .map_err(|err| err.to_string());
        cx.notify();
    }

    fn refresh_todos(&mut self, cx: &mut Context<Self>) {
        self.todo_date = Local::now().date_naive();
        // 表单归属日期对齐"今天"（磁贴跨零点仍打开时经午夜任务触达此处）
        let date = self.todo_date;
        self.todo_form.update(cx, |form, _| form.set_date(date));
        self.todo_list = self
            .todos
            .read(cx)
            .todos_between(self.todo_date, self.todo_date)
            .map_err(|err| err.to_string());
        self.schedule_midnight_refresh(cx);
        cx.notify();
    }

    fn schedule_midnight_refresh(&mut self, cx: &mut Context<Self>) {
        self.midnight_task = None;
        let now = Local::now();
        let Some(tomorrow) = now.date_naive().succ_opt() else {
            return;
        };
        let Some(midnight) = tomorrow.and_hms_opt(0, 0, 0) else {
            return;
        };
        let Some(next) = Local.from_local_datetime(&midnight).earliest() else {
            return;
        };
        let delay = next
            .signed_duration_since(now)
            .to_std()
            .unwrap_or(Duration::ZERO);
        self.midnight_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = this.update(cx, |this, cx| {
                if this.mode == TileMode::Todos {
                    this.refresh_todos(cx);
                }
            });
        }));
    }

    fn open_note(&mut self, note_id: i64, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.key.clone();
        let current_status = self
            .notes
            .update(cx, |state, cx| state.flush_now(current, cx));
        if current_status == Some(SaveStatus::Failed) {
            self.mode = TileMode::Editor;
            self.status = SaveIndicator::Failed;
            self.focus_editor(window, cx);
            cx.notify();
            return;
        }
        let target_status = self
            .notes
            .update(cx, |state, cx| state.flush_now(NoteKey::Note(note_id), cx));
        if target_status == Some(SaveStatus::Failed) {
            toast::error("目标笔记保存失败，无法打开", window, cx);
            return;
        }

        let note = self
            .notes
            .read(cx)
            .notes(&NoteFilter::default())
            .map_err(|err| err.to_string())
            .map(|notes| notes.into_iter().find(|note| note.id == note_id));
        match note {
            Ok(Some(note)) => {
                self.key = NoteKey::Note(note.id);
                self.title
                    .update(cx, |state, cx| state.set_value(&note.title, window, cx));
                self.source.set_value(&note.content, window, cx);
                self.word_count = self.source.word_count(cx);
                self.status = SaveIndicator::Untouched;
                self.mode = TileMode::Editor;
                self.focus_editor(window, cx);
                cx.notify();
            }
            Ok(None) => {
                toast::error("笔记不存在", window, cx);
            }
            Err(err) => {
                toast::error(err, window, cx);
            }
        }
    }

    fn toggle_todo(&mut self, todo: Todo, window: &mut Window, cx: &mut Context<Self>) {
        let result = self.todos.update(cx, |state, cx| state.toggle(&todo, cx));
        if let Err(err) = result {
            toast::error(err.to_string(), window, cx);
        }
    }

    /// 新增表单展开 / 收起：高度动画在 0 ↔ 内容自然高之间过渡；展开时焦点
    /// 落内容输入框（切入即录入），收起时内容裁剪为 0 高、清单占满余下空间。
    fn toggle_todo_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // 动画起点 = 翻转前的可见高（展开态为自然高，收起态为 0）
        self.todo_form_anim_from = if self.todo_form_open {
            self.todo_form_height
        } else {
            px(0.)
        };
        self.todo_form_open = !self.todo_form_open;
        self.todo_form_anim_seq = self.todo_form_anim_seq.wrapping_add(1);
        if self.todo_form_open {
            let focus = self.todo_form.read(cx).content_focus(cx);
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    /// 内联表单提交：校验失败 toast 提示并保留输入；成功后复位表单
    /// （连续录入），清单经 `TodosEvent::Changed` 订阅自动刷新。
    fn submit_todo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let draft = self.todo_form.read(cx).draft(cx);
        match draft {
            Err(TodoFormError::Empty) => {}
            Err(TodoFormError::InvalidInterval) => {
                toast::warn("结束时间需晚于开始时间", window, cx);
            }
            Ok(draft) => {
                let result = self.todos.update(cx, |state, cx| {
                    state
                        .add(&draft.content, draft.date, draft.interval, cx)
                        .map(|_| ())
                });
                match result {
                    Ok(()) => self.todo_form.update(cx, |form, cx| form.reset(window, cx)),
                    Err(err) => toast::error(err.to_string(), window, cx),
                }
            }
        }
    }

    /// 行内 hover「删除」：直接落库删除（与主窗口待办页一致，无确认）。
    fn delete_todo(&mut self, todo_id: i64, cx: &mut Context<Self>) {
        self.todos.update(cx, |state, cx| state.delete(todo_id, cx));
    }
}
