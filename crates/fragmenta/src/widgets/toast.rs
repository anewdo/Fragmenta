//! 轻量 toast（架构 §5.10）：主窗口右上角浮层，滞留 4 秒后自动关闭，
//! 滞留期间可点关闭按钮提前消失；新 toast 替换当前 toast 并重置滞留计时。

use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::base::StyledExt as _;
use gpui_kit::prelude::*;
use gpui_kit::*;

/// toast 滞留时长：到时自动关闭；期间关闭按钮可提前关闭。
const DWELL: Duration = Duration::from_secs(4);

/// App 级共享 toast store：任意功能可推报，主窗口 shell 渲染浮层。
pub(crate) struct ToastStore(pub(crate) Entity<ToastState>);

impl Global for ToastStore {}

/// 一条 toast 的展示内容。
pub(crate) struct Toast {
    /// 主报告（一行），如导出结果摘要。
    pub title: String,
    /// 明细（多行，可无），如失败项列表。
    pub detail: Option<String>,
    /// 失败类报告：标题以 danger 色强调。
    pub failure: bool,
}

/// toast 状态：当前至多一条，新报告替换旧报告并重置滞留计时。
pub(crate) struct ToastState {
    toast: Option<Toast>,
    /// 到时自动关闭的滞留计时；`Task` drop 即取消，新 toast 替换旧任务即重置计时。
    timer: Option<Task<()>>,
}

impl ToastState {
    pub(crate) fn new() -> Self {
        Self {
            toast: None,
            timer: None,
        }
    }

    /// 展示一条 toast（替换当前），并重启 4 秒滞留计时。
    pub(crate) fn show(&mut self, toast: Toast, cx: &mut Context<Self>) {
        self.toast = Some(toast);
        self.timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DWELL).await;
            let _ = this.update(cx, |state, cx| state.dismiss(cx));
        }));
        cx.notify();
    }

    /// 关闭当前 toast（滞留到时或点击关闭按钮）。
    pub(crate) fn dismiss(&mut self, cx: &mut Context<Self>) {
        self.toast = None;
        // 丢弃滞留计时任务即取消未触发的自动关闭
        self.timer = None;
        cx.notify();
    }
}

impl Render for ToastState {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(toast) = &self.toast else {
            // 无内容时仅占 absolute 定位，不参与 flex 布局
            return div().absolute().into_any_element();
        };
        let (foreground, muted, danger, border, popover) = {
            let theme = cx.theme();
            (
                theme.foreground,
                theme.muted_foreground,
                theme.danger,
                theme.border,
                theme.tokens.popover,
            )
        };
        let title = toast.title.clone();
        let detail = toast.detail.clone();
        let title_color = if toast.failure { danger } else { foreground };

        h_flex()
            .absolute()
            .top(px(16.))
            .right(px(16.))
            .max_w(px(360.))
            .items_start()
            .gap_2()
            .p_3()
            .pl_4()
            .rounded(px(8.))
            .border_1()
            .border_color(border)
            .bg(popover)
            .shadow_sm()
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .text_color(title_color)
                            .child(title),
                    )
                    .when_some(detail, |el, detail| {
                        el.child(div().text_sm().text_color(muted).child(detail))
                    }),
            )
            .child(
                Button::new("toast-close")
                    .ghost()
                    .compact()
                    .icon(IconName::X)
                    .on_click(cx.listener(|this, _, _, cx| this.dismiss(cx))),
            )
            .into_any_element()
    }
}
