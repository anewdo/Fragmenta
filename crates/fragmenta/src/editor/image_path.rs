//! markdown 图片路径插件（架构 §8 三条规则，spike ② 验证结论落地）：
//! `imgs/` 前缀 → 图库路径、绝对路径 → 原样、其余（越界相对路径 / 协议
//! URL）→ 回退显示 markdown 原文。
//!
//! 解析复用 [`Images::resolve`]（与 media 层语义同源）；不做存在性检查，
//! 缺失文件由 `img` 元素自行呈现占位。

use fragmenta_core::media::Images;
use gpui_kit::base::text::{
    MarkdownNode, MarkdownParseContext, MarkdownPlugin, markdown_ast as mdast,
};
use gpui_kit::prelude::*;
use gpui_kit::*;

/// 图片路径插件：拦截 `![alt](url)` 节点并落地三条解析规则。
pub struct ImagePathPlugin {
    pub images: Images,
}

impl MarkdownPlugin for ImagePathPlugin {
    fn name(&self) -> &str {
        "fragmenta-image-path"
    }

    fn parse(&self, node: &mdast::Node, cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let mdast::Node::Image(image) = node else {
            return None;
        };
        // parse 运行于后台线程，仅存 url；原文留给规则 3 回退
        Some(
            MarkdownNode::new(self.name(), image.url.clone())
                .text(cx.node_source(node).unwrap_or(&image.url)),
        )
    }

    fn render(&self, node: &MarkdownNode, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let url = node
            .data::<String>()
            .map(|s| s.as_str())
            .unwrap_or_default();
        match self.images.resolve(url) {
            Some(path) => img(path).max_w_full().into_any_element(),
            None => node.as_text().to_string().into_any_element(),
        }
    }
}
