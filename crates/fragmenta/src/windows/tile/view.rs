//! 磁贴视图：顶栏操作 + 标题 / 正文输入 + 防抖保存闭环 + 原生边缘 resize。

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::app::{ImageStore, NotesStore};
use crate::editor::SourceText;
use crate::state::{Event, NoteDraft, NoteKey, NotesState, SaveStatus};
use crate::win32;

use super::{CORNER, open_new};

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

/// 磁贴视图：顶栏操作 + 标题 / 正文输入 + 防抖保存闭环。
pub(super) struct TileView {
    /// 防抖槽身份：未落库草稿为 `Draft`(自身实体 id)，首存成功后切换为 `Note(id)`。
    key: NoteKey,
    notes: Entity<NotesState>,
    title: Entity<InputState>,
    source: SourceText,
    topmost: bool,
    status: SaveIndicator,
    word_count: usize,
    /// 最近一次圆角区域参数 (宽, 高, scale)：未变则跳过重设。
    region: Option<(f32, f32, f32)>,
    _title_sub: Subscription,
    _word_sub: Subscription,
    _saved_sub: Subscription,
    _bounds_sub: Subscription,
}

impl TileView {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let notes = cx.global::<NotesStore>().0.clone();
        let source = SourceText::new(
            cx.global::<ImageStore>().0.clone(),
            "写点什么……",
            window,
            cx,
        );
        let title = cx.new(|cx| InputState::new(window, cx).placeholder("标题"));

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
        // 订阅矩阵（§5.6）：磁贴只响应携带自己 key 的 Saved，不订阅 Changed
        let saved_sub = cx.subscribe(&notes, |this, _, event: &Event, cx| {
            if let Event::Saved { key, id, status } = event
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

        // 八向边缘 resize：WM_NCHITTEST 子类化补齐框架缺口（隐藏标题栏时
        // 框架只算顶边），命中后由系统原生模态循环接管；最小尺寸经
        // window_min_size 由后端 WM_GETMINMAXINFO 原生钳制
        let _ = win32::install_edge_subclass(window, EDGE_HOT, CORNER_HOT);
        // 圆角窗口区域（失败不阻断：界面层圆角描边兜底）
        let _ = win32::set_rounded_region(window, CORNER);
        // 窗口 bounds 变化（用户 resize / 拖动 / 跨显示器 scale 变化）时重设区域
        let bounds_sub = cx.observe_window_bounds(window, |this, window, _| {
            this.refresh_rounded_region(window);
        });

        // 焦点落正文：热键呼出后立即输入
        let focus = source.textarea().read(cx).focus_handle(cx);
        window.focus(&focus, cx);

        Self {
            key: NoteKey::Draft(cx.entity().entity_id()),
            notes,
            title,
            source,
            topmost: true, // PopUp 默认携带 WS_EX_TOPMOST（spike ④ 验证）
            status: SaveIndicator::Untouched,
            word_count: 0,
            region: None,
            _title_sub: title_sub,
            _word_sub: word_sub,
            _saved_sub: saved_sub,
            _bounds_sub: bounds_sub,
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

    /// 关闭语义（§5.8）：空草稿直接丢弃（cancel 待写槽）；非空先 `flush_now`
    /// 同步保存再关闭；落库失败时阻止关闭并持续提示，用户处理后自然重试。
    fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let title = self.title.read(cx).value().to_string();
        let content = self.source.value(cx);
        if title.is_empty() && content.is_empty() {
            let key = self.key.clone();
            self.notes.update(cx, |state, _| state.cancel(&key));
            window.remove_window();
        } else {
            let key = self.key.clone();
            let status = self.notes.update(cx, |state, cx| state.flush_now(key, cx));
            if status == Some(SaveStatus::Failed) {
                self.status = SaveIndicator::Failed;
                cx.notify();
            } else {
                window.remove_window();
            }
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
}

impl Render for TileView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.entity().downgrade();
        let (background, border, muted_foreground, danger) = {
            let theme = cx.theme();
            (
                theme.background,
                theme.border,
                theme.muted_foreground,
                theme.danger,
            )
        };

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
            .border_b_1()
            .border_color(border)
            .child(
                Button::new("tile-new")
                    .ghost()
                    .compact()
                    .icon(IconName::Plus)
                    .on_click(|_, _, cx| open_new(cx)),
            )
            .child(
                Button::new("tile-settings")
                    .ghost()
                    .compact()
                    .icon(IconName::Settings)
                    .on_click(|_, _, cx| crate::windows::main::open(cx)),
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
                Button::new("tile-topmost")
                    .ghost()
                    .compact()
                    .icon(if self.topmost {
                        IconName::PinOff
                    } else {
                        IconName::Pin
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
                    .on_click({
                        let weak = weak.clone();
                        move |_, window, cx| {
                            let _ = weak.update(cx, |this, cx| this.request_close(window, cx));
                        }
                    }),
            );

        // 标题与正文共处同一"输入框"：双双 appearance(false) 去组件装饰，
        // 共享窗口背景、仅隔一条分隔线；Textarea 编辑区内衬左右 10px
        // （Medium 档），标题行 px(10) 与正文横向对齐
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
            .child(
                h_flex()
                    .flex_shrink_0()
                    .h_9()
                    .items_center()
                    .overflow_hidden()
                    .border_b_1()
                    .border_color(border)
                    .child(Input::new(&self.title).appearance(false).px(px(10.))),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(self.source.element().appearance(false).h_full()),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .h_6()
                    .px_3()
                    .items_center()
                    .text_size(px(11.))
                    .text_color(muted_foreground)
                    .child(format!("{} 字", self.word_count)),
            )
    }
}
