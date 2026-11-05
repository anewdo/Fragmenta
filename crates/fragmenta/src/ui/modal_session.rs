//! 弹窗会话：关闭动画、动画序号与焦点归还的共用状态机。
//!
//! 各弹窗只保留自己的提交逻辑与关闭事件；阶段迁移、计时与焦点归还
//! 统一经本模块，避免三份实现各自维护时长与广播时机。

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::ui::dialog_motion::DialogMotion;

/// 弹窗阶段：关闭动画期间仍保持挂载，计时结束才真正移除。
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModalPhase {
    Open,
    Closing,
}

/// 关闭动作发起前的会话快照：阶段、动画序号、焦点陷阱与归还焦点。
pub(crate) struct ModalSession {
    phase: ModalPhase,
    /// 动画序号：阶段切换时递增，让淡入淡出以新 id 重新播放。
    seq: usize,
    /// 焦点陷阱宿主焦点（Esc / Enter 在弹窗内响应）。
    focus: FocusHandle,
    /// 打开前的焦点，关闭动画结束后归还。
    previous_focus: Option<FocusHandle>,
}

impl ModalSession {
    pub(crate) fn new(seq: usize, focus: FocusHandle, previous_focus: Option<FocusHandle>) -> Self {
        Self {
            phase: ModalPhase::Open,
            seq,
            focus,
            previous_focus,
        }
    }

    /// 当前动画序号：渲染侧据此生成动画元素 id。
    pub(crate) fn seq(&self) -> usize {
        self.seq
    }

    /// 焦点陷阱句柄：渲染时交给 `Dialog`。
    pub(crate) fn focus(&self) -> &FocusHandle {
        &self.focus
    }

    pub(crate) fn is_closing(&self) -> bool {
        self.phase == ModalPhase::Closing
    }

    fn take_previous_focus(&mut self) -> Option<FocusHandle> {
        self.previous_focus.take()
    }
}

/// 持有弹窗会话的宿主；`None` 表示弹窗已从宿主移除。
pub(crate) trait ModalSessionHost {
    fn modal_session(&mut self) -> Option<&mut ModalSession>;
}

/// 进入关闭阶段：播放反向动画，计时结束后归还焦点并调用 `finish`。
///
/// 返回 `false` 表示会话已不存在或已在关闭中，调用方无需再处理。
pub(crate) fn begin_close<R>(
    session: &mut ModalSession,
    window: &mut Window,
    cx: &mut Context<R>,
    finish: impl FnOnce(&mut R, &mut Window, &mut Context<R>) + 'static,
) -> bool
where
    R: ModalSessionHost + 'static,
{
    if session.is_closing() {
        return false;
    }
    session.phase = ModalPhase::Closing;
    session.seq = session.seq.wrapping_add(1);
    let seq = session.seq;
    cx.notify();
    let duration = DialogMotion::for_window(window, cx).duration();

    let weak = cx.entity().downgrade();
    cx.spawn_in(window, async move |_, cx| {
        cx.background_executor().timer(duration).await;
        let _ = weak.update_in(cx, |this, window, cx| {
            let previous = this.modal_session().and_then(|session| {
                (session.seq == seq && session.is_closing()).then(|| session.take_previous_focus())
            });
            let Some(previous) = previous else {
                return;
            };
            if let Some(handle) = previous {
                window.focus(&handle, cx);
            }
            finish(this, window, cx);
        });
    })
    .detach();
    true
}
