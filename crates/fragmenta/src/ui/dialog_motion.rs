//! 弹窗动效（ADR-0010）：kit Dialog 与自绘弹窗共用一组主题 motion 参数。
//!
//! GPUI 没有 CSS 的 `filter: blur()`，动效以淡入淡出 + 短距离位移表达；
//! 时长、曲率与位移全部取自主题 `motion_tokens`，减少动态效果时退化为即时呈现。

use std::time::Duration;

use gpui_kit::base::Easing;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::animation::EffectTransition;
use gpui_kit::component::dialog::{AlertDialog, Dialog, DialogEntrance};
use gpui_kit::{App, Pixels, Window, px};

/// kit Dialog 的统一入场方式：淡入并自上方落位到最终位置。
pub const ENTRANCE: DialogEntrance = DialogEntrance::FadeSlide;

/// 给 kit Dialog 套用统一入场动效；调用方仍可继续链式设置其余属性。
pub fn kit_dialog(dialog: Dialog) -> Dialog {
    dialog.entrance(ENTRANCE)
}

/// 给 kit AlertDialog 套用同一入场动效（用于确认弹窗）。
pub fn alert_dialog(alert: AlertDialog) -> AlertDialog {
    alert.entrance(ENTRANCE)
}

/// 自绘弹窗的动效参数，与 kit Dialog 的 [`ENTRANCE`] 同源。
#[derive(Clone)]
pub struct DialogMotion {
    duration: Duration,
    travel: Pixels,
    easing: Easing,
}

impl DialogMotion {
    /// 按当前主题与减少动态效果设置解析动效参数。
    pub fn for_window(window: &Window, cx: &App) -> Self {
        let motion = cx.theme().motion_tokens();
        Self {
            duration: if cx.reduce_motion() {
                Duration::ZERO
            } else {
                motion.duration_normal
            },
            travel: motion.distance_short.to_pixels(window.rem_size()),
            easing: motion.easing_enter.clone(),
        }
    }

    /// 动画时长：关闭计时与 kit Dialog 的退场时长一致。
    pub fn duration(&self) -> Duration {
        self.duration
    }

    /// 卡片动效：入场自上方短距离落位，退场反向升出。
    pub fn card(&self, entering: bool) -> EffectTransition {
        let (from, to) = if entering { (0., 1.) } else { (1., 0.) };
        let (slide_from, slide_to) = if entering {
            (-self.travel, px(0.))
        } else {
            (px(0.), -self.travel)
        };
        self.transition()
            .slide_y(slide_from, slide_to)
            .fade(from, to)
    }

    /// 遮罩动效：只随阶段淡入淡出，不参与位移。
    pub fn backdrop(&self, entering: bool) -> EffectTransition {
        let (from, to) = if entering { (0., 1.) } else { (1., 0.) };
        self.transition().fade(from, to)
    }

    fn transition(&self) -> EffectTransition {
        let easing = self.easing.clone();
        EffectTransition::new(self.duration).ease(move |t| easing.sample(t).clamp(0., 1.))
    }
}
