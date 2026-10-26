//! 内置色板（ADR-0008）：6 色 Material Design 500 档，作为分类 / 标签
//! 管理页选色 Popover 的内置色来源；另有 `#RRGGBB` 颜色解析，供读回
//! DB 中用户选定的持久颜色。
//!
//! 应用层纯逻辑模块（仅用颜色数据类型，零 UI 组件依赖）：
//! - 分类与标签的展示颜色由用户选定、持久存于 DB（`color` 列）；
//! - 未选色的条目以主题中性样式渲染；
//! - 任意取色器（ticket 03）产出的颜色同样以 `#RRGGBB` 落库。

use gpui_kit::{Hsla, Rgba, rgb};

/// 色盘色的填充 Tag 前景（白）：色盘统一 500 亮度档，白字深 / 浅主题下均可读。
pub(crate) fn filled_foreground() -> Hsla {
    gpui_kit::hsla(0., 0., 1., 1.)
}

/// 6 色 Material Design 取色盘：六个不同 hue 家族、同一 500 亮度档。
/// 分类 / 标签选色 Popover 的内置色来源。
pub(crate) fn palette_colors() -> [Hsla; 6] {
    [
        rgb(0xF44336).into(), // Red 500
        rgb(0xFF9800).into(), // Orange 500
        rgb(0x4CAF50).into(), // Green 500
        rgb(0x009688).into(), // Teal 500
        rgb(0x2196F3).into(), // Blue 500
        rgb(0x9C27B0).into(), // Purple 500
    ]
}

/// `#RRGGBB`（可省略 `#`）→ Hsla；格式非法返回 `None`（按未选色渲染）。
pub fn parse_hex_color(s: &str) -> Option<Hsla> {
    let hex = s.strip_prefix('#').unwrap_or(s);
    if hex.len() != 6 || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(hex, 16).ok()?;
    Some(rgb(value).into())
}

/// Hsla → `#RRGGBB`（落库格式）：透明度丢弃，统一按不透明存储，
/// 与 [`parse_hex_color`] 的读回格式对齐。
pub(crate) fn format_hex_color(color: Hsla) -> String {
    let rgba = Rgba::from(Hsla { a: 1., ..color });
    let channel = |value: f32| (value * 255.).round() as u32;
    format!(
        "#{:02X}{:02X}{:02X}",
        channel(rgba.r),
        channel(rgba.g),
        channel(rgba.b)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rrggbb_with_and_without_hash() {
        let with_hash = parse_hex_color("#2196F3").unwrap();
        let bare = parse_hex_color("2196F3").unwrap();
        assert_eq!(with_hash, bare);
    }

    #[test]
    fn rejects_malformed_input() {
        assert_eq!(parse_hex_color(""), None);
        assert_eq!(parse_hex_color("#12345"), None);
        assert_eq!(parse_hex_color("#1234567"), None);
        assert_eq!(parse_hex_color("#GGGGGG"), None);
        assert_eq!(parse_hex_color("#2196F3 "), None);
    }

    #[test]
    fn formats_opaque_rrggbb_and_round_trips() {
        assert_eq!(format_hex_color(parse_hex_color("#2196F3").unwrap()), "#2196F3");
        assert_eq!(
            format_hex_color(parse_hex_color("#F44336").unwrap()),
            "#F44336"
        );
        // 透明度统一丢弃：任意取色器产出按不透明落库
        let translucent = Hsla {
            a: 0.4,
            ..parse_hex_color("#FF0000").unwrap()
        };
        assert_eq!(format_hex_color(translucent), "#FF0000");
    }
}
