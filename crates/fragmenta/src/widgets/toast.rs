//! toast（架构 §5.10）：App 级共享浮层，主窗口与磁贴窗各自挂载一份渲染。
//!
//! 以 deferred 图层绘制在弹窗（`10 + layer`）与弹层（`POPUP_PRIORITY`）之上：
//! 点击提示条（含关闭按钮）不会落入其下弹窗的遮罩，遮罩自身的点击关闭也不
//! 受影响。条目携带触发窗口句柄，仅在该窗口可见；同一窗口至多同屏
//! [`MAX_ON_SCREEN`] 条，超出时最老的存活条目立即进入退场腾出容量。滞留
//! 时长由条目自带（notice 短 / report 长），到点播放退场动画后移除，期间
//! 关闭按钮可提前触发退场。条目可选出现位置（右上 / 顶部居中），同位置的
//! 条目纵向堆叠；入场动画随位置取横向或纵向滑动（respect 系统减少动态
//! 效果设置）。

use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::ui::tokens;

/// 短提示（单行成功 / 出错文案）滞留时长。
const NOTICE_DWELL: Duration = Duration::from_secs(2);
/// 报告（含明细行）滞留时长。
const REPORT_DWELL: Duration = Duration::from_secs(4);
/// 图层级次：弹窗为 `10 + layer`、弹层为 100，提示条位于二者之上；
/// tooltip（200）仍盖在提示条之上。
const TOAST_LAYER: usize = 150;
/// 入场动画时长：滑入 + 淡入。
const ENTER_DURATION: Duration = Duration::from_millis(240);
/// 退场动画时长：沿原路滑回 + 淡出。
const EXIT_DURATION: Duration = Duration::from_millis(180);
/// 滑动行程（逻辑像素）。
const SLIDE: Pixels = px(24.);
/// 同一触发窗口同屏可见（含退场中）的条目上限；超出时最老存活条目立即退场。
const MAX_ON_SCREEN: usize = 3;

/// toast 级别：决定状态图标与强调色。
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToastKind {
    /// 信息：中性说明文案。
    #[allow(dead_code)] // 待接入调用点
    Info,
    /// 成功：绿圈对勾。
    Success,
    /// 警告：黄色叹号。
    Warn,
    /// 错误：红圈叉号。
    Error,
}

impl ToastKind {
    fn icon(self) -> IconName {
        match self {
            ToastKind::Info => IconName::Info,
            ToastKind::Success => IconName::CircleCheck,
            ToastKind::Warn => IconName::CircleAlert,
            ToastKind::Error => IconName::CircleX,
        }
    }

    fn color(self, theme: &gpui_kit::component::Theme) -> Hsla {
        match self {
            ToastKind::Info => theme.info,
            ToastKind::Success => theme.success,
            ToastKind::Warn => theme.warning,
            ToastKind::Error => theme.danger,
        }
    }
}

/// 出现位置：决定堆叠锚点与入场滑动方向。
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToastPlacement {
    /// 右上角，沿水平方向滑入 / 滑出。
    TopRight,
    /// 顶部居中，沿垂直方向从上方滑入 / 滑出。
    TopCenter,
}

/// 一条 toast 的展示内容。
pub(crate) struct Toast {
    /// 主报告（一行），如保存结果或导出摘要。
    pub title: String,
    /// 明细（多行，可无），如失败项列表。
    pub detail: Option<String>,
    /// 级别：决定图标与强调色。
    pub kind: ToastKind,
    /// 滞留时长：到时自动关闭。
    pub dwell: Duration,
    /// 出现位置与动画方向，默认右上。
    pub placement: ToastPlacement,
}

impl Toast {
    /// 短提示：单行文案，短滞留。
    pub(crate) fn notice(title: impl Into<String>, kind: ToastKind) -> Self {
        Self {
            title: title.into(),
            detail: None,
            kind,
            dwell: NOTICE_DWELL,
            placement: ToastPlacement::TopCenter,
        }
    }

    /// 报告：标题 + 可选明细，滞留较长时间。
    pub(crate) fn report(
        title: impl Into<String>,
        detail: Option<String>,
        kind: ToastKind,
    ) -> Self {
        Self {
            title: title.into(),
            detail,
            kind,
            dwell: REPORT_DWELL,
            placement: ToastPlacement::TopRight,
        }
    }

    /// 指定出现位置（默认右上）。
    #[allow(dead_code)] // 待接入调用点
    pub(crate) fn placement(mut self, placement: ToastPlacement) -> Self {
        self.placement = placement;
        self
    }
}

/// 信息提示：中性说明文案。
#[allow(dead_code)] // 待接入调用点
pub(crate) fn info(message: impl Into<String>, window: &mut Window, cx: &mut App) {
    push(Toast::notice(message, ToastKind::Info), window, cx);
}

/// 成功提示：触发窗口的右上角浮层，短滞留后自动关闭。
pub(crate) fn success(message: impl Into<String>, window: &mut Window, cx: &mut App) {
    push(Toast::notice(message, ToastKind::Success), window, cx);
}

/// 警告提示：非致命的校验或部分失败。
pub(crate) fn warn(message: impl Into<String>, window: &mut Window, cx: &mut App) {
    push(Toast::notice(message, ToastKind::Warn), window, cx);
}

/// 失败提示：状态图标与强调色标红。
pub(crate) fn error(message: impl Into<String>, window: &mut Window, cx: &mut App) {
    push(Toast::notice(message, ToastKind::Error), window, cx);
}

/// 推送一条自定义 toast（如需指定出现位置，经 [`Toast::placement`]）。
pub(crate) fn push(toast: Toast, window: &mut Window, cx: &mut App) {
    let handle = window.window_handle();
    let store = cx.global::<ToastStore>().0.clone();
    store.update(cx, |state, cx| state.show(toast, handle, cx));
}

/// App 级共享 toast store：任意功能可推报，各窗口 shell 挂载浮层渲染。
pub(crate) struct ToastStore(pub(crate) Entity<ToastState>);

impl Global for ToastStore {}

/// toast 队列中的一条：自带滞留计时与退场状态，条目间互不影响。
struct ToastEntry {
    /// 自增序号：动画与关闭按钮的元素 id、计时回调定位。
    id: u64,
    toast: Toast,
    /// 触发窗口：浮层仅在该窗口渲染（主窗口 / 磁贴窗各自挂载同一实体）。
    window: AnyWindowHandle,
    /// 退场动画播放中：播完移除条目。
    leaving: bool,
    /// 到时自动关闭的计时；`Task` drop 即取消。
    timer: Option<Task<()>>,
}

/// toast 状态：同屏多条队列，新条目入队而非替换。
pub(crate) struct ToastState {
    entries: Vec<ToastEntry>,
    next_id: u64,
}

impl ToastState {
    pub(crate) fn new() -> Self {
        Self {
            entries: Vec::new(),
            next_id: 0,
        }
    }

    /// 展示一条 toast（入队），并启动该条目的滞留计时；同屏超出上限时
    /// 最老的存活条目立即进入退场。
    pub(crate) fn show(
        &mut self,
        toast: Toast,
        window: AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        let id = self.next_id;
        self.next_id += 1;
        let dwell = toast.dwell;
        self.entries.push(ToastEntry {
            id,
            toast,
            window,
            leaving: false,
            timer: Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(dwell).await;
                let _ = this.update(cx, |state, cx| state.begin_leave(id, cx));
            })),
        });
        // 同屏容量按触发窗口计（含退场中的条目）
        let alive = self
            .entries
            .iter()
            .filter(|entry| !entry.leaving && entry.window == window)
            .count();
        if alive > MAX_ON_SCREEN {
            let oldest = self
                .entries
                .iter()
                .find(|entry| !entry.leaving && entry.window == window)
                .map(|entry| entry.id);
            if let Some(oldest) = oldest {
                self.begin_leave(oldest, cx);
            }
        }
        cx.notify();
    }

    /// 进入退场：滞留到时、点击关闭或容量驱逐触发；退场动画播完后移除。
    fn begin_leave(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) else {
            return;
        };
        if entry.leaving {
            return;
        }
        entry.leaving = true;
        entry.timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(EXIT_DURATION).await;
            let _ = this.update(cx, |state, cx| state.remove(id, cx));
        }));
        cx.notify();
    }

    /// 退场结束（或条目已不存在的防御）：从队列移除。
    fn remove(&mut self, id: u64, cx: &mut Context<Self>) {
        self.entries.retain(|entry| entry.id != id);
        cx.notify();
    }

    /// 构建单条卡片：icon 居左、文本居右、关闭按钮收尾。
    /// 卡片 `relative` 定位，滑动动画以 inset 偏移实现，堆叠互不干扰。
    fn render_card(spec: &CardSpec, cx: &mut Context<Self>) -> AnyElement {
        let (foreground, muted, border, popover) = {
            let theme = cx.theme();
            (
                theme.foreground,
                theme.muted_foreground,
                theme.border,
                theme.tokens.popover,
            )
        };
        let icon_color = spec.kind.color(cx.theme());
        let icon = spec.kind.icon();
        let title = spec.title.clone();
        let detail = spec.detail.clone();
        let id = spec.id;
        let leaving = spec.leaving;
        let placement = spec.placement;

        // occlude：卡片区域遮挡其下图层的命中测试，点击（含关闭按钮）
        // 不会穿透到弹窗遮罩。卡片在堆叠容器流内 `relative` 定位，
        // 动画偏移不改变其占位。
        let card = h_flex()
            .relative()
            .w_full()
            .items_start()
            .gap(tokens::spacing::SM)
            .p(tokens::spacing::SM)
            .rounded(tokens::radius::MD)
            .border_1()
            .border_color(border)
            .bg(popover)
            .shadow_sm()
            .occlude()
            .child(Icon::new(icon).size_4().text_color(icon_color))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .text_color(foreground)
                            .child(title),
                    )
                    .when_some(detail, |el, detail| {
                        el.child(div().text_sm().text_color(muted).child(detail))
                    }),
            )
            .child(
                Button::new(ElementId::NamedInteger("toast-close".into(), id))
                    .ghost()
                    .xsmall()
                    .icon(IconName::X)
                    .on_click(cx.listener(move |this, _, _, cx| this.begin_leave(id, cx))),
            )
            .with_animation(
                ElementId::NamedInteger(format!("toast-slide-{id}").into(), leaving as u64),
                Animation::new(if leaving {
                    EXIT_DURATION
                } else {
                    ENTER_DURATION
                })
                .with_easing(ease_in_out),
                move |this, delta| match placement {
                    // 右上：沿水平滑入 / 滑出。
                    ToastPlacement::TopRight => {
                        if leaving {
                            this.opacity(1. - delta).left(delta * SLIDE)
                        } else {
                            this.opacity(delta).left((1. - delta) * SLIDE)
                        }
                    }
                    // 顶部居中：沿垂直从上方滑入 / 滑出。
                    ToastPlacement::TopCenter => {
                        if leaving {
                            this.opacity(1. - delta).top(-delta * SLIDE)
                        } else {
                            this.opacity(delta).top(-(1. - delta) * SLIDE)
                        }
                    }
                },
            );
        card.into_any_element()
    }
}

/// 单条卡片的渲染数据：从队列条目拷贝，避免渲染期间持有状态借用。
struct CardSpec {
    id: u64,
    leaving: bool,
    kind: ToastKind,
    placement: ToastPlacement,
    title: String,
    detail: Option<String>,
}

impl Render for ToastState {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let handle = window.window_handle();
        let cards: Vec<CardSpec> = self
            .entries
            .iter()
            .filter(|entry| entry.window == handle)
            .map(|entry| CardSpec {
                id: entry.id,
                leaving: entry.leaving,
                kind: entry.toast.kind,
                placement: entry.toast.placement,
                title: entry.toast.title.clone(),
                detail: entry.toast.detail.clone(),
            })
            .collect();
        if cards.is_empty() {
            // 无内容时仅占 absolute 定位，不参与 flex 布局
            return div().absolute().into_any_element();
        }

        // 按出现位置分组堆叠（保持入队顺序）
        let top_right: Vec<&CardSpec> = cards
            .iter()
            .filter(|spec| spec.placement == ToastPlacement::TopRight)
            .collect();
        let top_center: Vec<&CardSpec> = cards
            .iter()
            .filter(|spec| spec.placement == ToastPlacement::TopCenter)
            .collect();

        let viewport = window.viewport_size();
        // 顶部居中锚点：水平居中（窗口窄于卡片时贴左缘防溢出）
        let center_left = px(
            ((viewport.width.as_f32() - tokens::widths::TOAST.as_f32()) / 2.).max(0.),
        );
        let anchor_top = tokens::heights::TITLE_BAR + tokens::spacing::LG;
        let mut stack = |specs: &[&CardSpec]| {
            v_flex()
                .gap(tokens::spacing::SM)
                .children(specs.iter().map(|spec| Self::render_card(spec, cx)))
        };

        // 与弹窗同构的 deferred 图层宿主：卡片窗口坐标定位；宿主自身无
        // 鼠标处理，卡片之外的点击照常到达其下的弹窗遮罩。
        deferred(
            anchored().position(point(px(0.), px(0.))).child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .w(viewport.width)
                    .h(viewport.height)
                    .when(!top_right.is_empty(), |this| {
                        this.child(
                            div()
                                .absolute()
                                .top(anchor_top)
                                .right(tokens::spacing::LG)
                                .w(tokens::widths::TOAST)
                                .child(stack(&top_right)),
                        )
                    })
                    .when(!top_center.is_empty(), |this| {
                        this.child(
                            div()
                                .absolute()
                                .top(anchor_top)
                                .left(center_left)
                                .w(tokens::widths::TOAST)
                                .child(stack(&top_center)),
                        )
                    }),
            ),
        )
        .with_priority(TOAST_LAYER)
        .into_any_element()
    }
}
