//! 快捷键组：磁贴呼出热键的捕获式录入与红字错误提示。

use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::SettingsPage;
use super::common::{error_text, setting_row};
use crate::hotkey::{HotkeyController, HotkeyError};
use crate::settings::SettingsStore;
use crate::ui::layout::{CardStyle, setting_card};
use crate::ui::tokens::{self, TextRole, TextRoleExt as _};

/// 热键录入框宽度：设置页局部控件特例。
const HOTKEY_RECORDER_WIDTH: Pixels = px(240.);

impl SettingsPage {
    /// "快捷键"组：磁贴呼出热键捕获式录入框与红字错误提示。
    pub(super) fn hotkey_group(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let hotkey = cx
            .global::<SettingsStore>()
            .0
            .read(cx)
            .tile_hotkey()
            .to_string();
        let card = CardStyle::from_theme(cx.theme());
        let (muted, primary, border, danger) = {
            let colors = cx.theme();
            (
                colors.muted_foreground,
                colors.primary,
                colors.border,
                colors.danger,
            )
        };
        let focus = self.hotkey_focus.clone();
        let listening = self.hotkey_listening;
        let pending_mods = self.hotkey_pending_mods.clone();
        // 启动期已保存热键注册失败：红字提示改绑（改绑成功后控制器清除冲突）
        let conflict = cx
            .try_global::<HotkeyController>()
            .is_some_and(HotkeyController::conflict);
        let error = self
            .hotkey_error
            .or(conflict.then_some("检测到快捷键冲突，请更换组合"));

        let recorder = div()
            .id("hotkey-recorder")
            .track_focus(&focus)
            .cursor_pointer()
            .flex()
            .items_center()
            .justify_center()
            .w(HOTKEY_RECORDER_WIDTH)
            .h(tokens::heights::CONTROL)
            .rounded(tokens::radius::SM)
            .border_1()
            .border_color(if listening { primary } else { border })
            .child(match (listening, pending_mods.as_deref()) {
                (true, Some(mods)) => div()
                    .text_role(TextRole::ControlLabel)
                    .text_color(muted)
                    .child(format!("{mods} + …")),
                (true, None) => div()
                    .text_role(TextRole::ControlLabel)
                    .text_color(muted)
                    .child("按下新组合键…"),
                (false, _) => div()
                    .text_role(TextRole::ControlLabel)
                    .child(pretty_hotkey(&hotkey)),
            })
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                // 点击框外退出监听态，不误改热键
                if this.hotkey_listening {
                    this.hotkey_listening = false;
                    this.hotkey_pending_mods = None;
                    this.hotkey_error = None;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(|this, _, window, cx| {
                // 点击录入框：聚焦并进入监听态
                window.focus(&this.hotkey_focus, cx);
                this.hotkey_listening = true;
                this.hotkey_pending_mods = None;
                this.hotkey_error = None;
                cx.notify();
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if !this.hotkey_listening {
                    return;
                }
                // 监听态吞掉全部按键，避免触发其他快捷键
                window.prevent_default();
                cx.stop_propagation();
                let stroke = &event.keystroke;
                match stroke.key.as_str() {
                    // Esc 退出监听态，不误改热键
                    "escape" => {
                        this.hotkey_listening = false;
                        this.hotkey_pending_mods = None;
                        this.hotkey_error = None;
                    }
                    // 修饰键自身按下：和弦进行中，展示由 on_modifiers_changed 维护
                    "shift" | "control" | "alt" | "platform" | "meta" | "super" | "cmd" | "win"
                    | "function" | "fn" => {}
                    _ => {
                        this.hotkey_pending_mods = None;
                        let candidate = hotkey_candidate(stroke);
                        let store = cx.global::<SettingsStore>().0.clone();
                        match store.update(cx, |state, cx| state.set_hotkey(&candidate, cx)) {
                            Ok(()) => {
                                this.hotkey_listening = false;
                                this.hotkey_error = None;
                            }
                            Err(HotkeyError::Parse(_)) => {
                                this.hotkey_error = Some("无效组合，请按其他组合键");
                            }
                            Err(HotkeyError::Register(_)) => {
                                this.hotkey_listening = false;
                                this.hotkey_error = Some("组合键被其他程序占用，请更换组合");
                            }
                        }
                    }
                }
                cx.notify();
            }))
            .on_modifiers_changed(cx.listener(|this, event: &ModifiersChangedEvent, _, cx| {
                if !this.hotkey_listening {
                    return;
                }
                if event.modifiers.number_of_modifiers() == 0 {
                    // 和弦中松开全部修饰键而未按非修饰键：纯修饰键组合
                    if this.hotkey_pending_mods.take().is_some() {
                        this.hotkey_error = Some("纯修饰键组合无效，请按下一个非修饰键");
                        cx.notify();
                    }
                } else {
                    let mods = pretty_modifiers(&event.modifiers);
                    if this.hotkey_pending_mods.as_deref() != Some(mods.as_str()) {
                        this.hotkey_pending_mods = Some(mods);
                        cx.notify();
                    }
                }
            }));

        v_flex()
            .w_full()
            .gap(tokens::spacing::SM)
            .child(setting_card(
                card,
                setting_row(
                    IconName::Keyboard,
                    "磁贴呼出热键",
                    "点击录入框后直接按下新组合键完成改绑；Esc 或点击框外取消",
                    muted,
                    recorder,
                ),
            ))
            .when_some(error, |el, err| {
                el.child(
                    h_flex()
                        .w_full()
                        .justify_end()
                        .child(error_text(err, danger)),
                )
            })
    }
}

/// gpui 按键事件 → global_hotkey 解析串（如 "ctrl+alt+n"）。
///
/// 键名统一大写以匹配解析器（"n"→"N"、"f5"→"F5"，符号与数字原样）；
/// 组合序为 shift + ctrl + alt + win + 主键。gpui 对 shift 组合键可能上报
/// 变形字符（如 "<"），此类候选会在解析层被拒并红字提示。
fn hotkey_candidate(stroke: &Keystroke) -> String {
    let mods = &stroke.modifiers;
    let mut candidate = String::new();
    if mods.shift {
        candidate.push_str("shift+");
    }
    if mods.control {
        candidate.push_str("ctrl+");
    }
    if mods.alt {
        candidate.push_str("alt+");
    }
    if mods.platform {
        candidate.push_str("super+");
    }
    candidate.push_str(&stroke.key.to_uppercase());
    candidate
}

/// 修饰键状态 → 展示串（如 "Ctrl + Alt"）。
fn pretty_modifiers(mods: &Modifiers) -> String {
    let mut parts = Vec::new();
    if mods.control {
        parts.push("Ctrl");
    }
    if mods.alt {
        parts.push("Alt");
    }
    if mods.shift {
        parts.push("Shift");
    }
    if mods.platform {
        parts.push("Win");
    }
    parts.join(" + ")
}

/// 设置串 → 展示串："ctrl+alt+n" → "Ctrl + Alt + N"。
/// 兼容 global_hotkey 规范形式（"KeyN" / "Digit5"）与常见修饰键别名。
fn pretty_hotkey(hotkey: &str) -> String {
    hotkey
        .split('+')
        .map(pretty_token)
        .collect::<Vec<_>>()
        .join(" + ")
}

/// 单个热键 token → 展示形式。
fn pretty_token(token: &str) -> String {
    let trimmed = token.trim();
    let lower = trimmed.to_lowercase();
    match lower.as_str() {
        "ctrl" | "control" => return "Ctrl".to_string(),
        "alt" => return "Alt".to_string(),
        "shift" => return "Shift".to_string(),
        "super" | "cmd" | "win" => return "Win".to_string(),
        _ => {}
    }
    if let Some(letter) = lower.strip_prefix("key").filter(|rest| rest.len() == 1) {
        return letter.to_uppercase();
    }
    if let Some(digit) = lower.strip_prefix("digit").filter(|rest| rest.len() == 1) {
        return digit.to_string();
    }
    // 单字符键大写；命名键（f5 / space 等）首字母大写
    let mut chars = trimmed.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => String::new(),
    }
}
