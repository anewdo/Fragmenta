//! 关于页：应用身份与项目地址（更新告知职责，ADR-0011）。

use std::sync::Arc;

use gpui_kit::base::Link;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::ui::layout::content_column;
use crate::ui::tokens::{self, TextRole, TextRoleExt as _};

/// 项目主页（spec.md 信息常量）。
const REPO_URL: &str = "https://github.com/anewdo/Fragmenta";
/// 许可行（spec.md 信息常量）。
const LICENSE_LINE: &str = "MIT License © 2026 niuniuzz";
/// 应用图标展示边长：关于页局部特例，命名常量集中声明（ADR-0007）。
const ICON_SIZE: Pixels = px(64.);

/// 关于页视图：静态信息页，常驻于主窗口 shell；无状态、无订阅。
pub(crate) struct AboutPage {
    /// 应用图标：与 exe 资源、托盘同源的内嵌 `.ico` 解码所得。
    icon: Option<Arc<RenderImage>>,
}

impl AboutPage {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self {
            icon: decode_app_icon(),
        }
    }
}

/// 解码内嵌 `.ico` 为渲染图像：image 的 ICO 解码取最高分辨率帧，
/// RGBA 需转 BGRA（gpui 渲染层取样序，同其 `decode_static_image_from_decoder`）。
/// 失败不致命：仅图标缺席，页面其余信息照常展示。
fn decode_app_icon() -> Option<Arc<RenderImage>> {
    let decoded = image::load_from_memory_with_format(
        include_bytes!("../../../../assets/app.ico"),
        image::ImageFormat::Ico,
    )
    .map_err(|err| eprintln!("[fragmenta] 关于页图标解码失败: {err}"))
    .ok()?;
    let mut rgba = decoded.to_rgba8();
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(rgba)])))
}

impl Render for AboutPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (muted, foreground, link, link_hover, link_active) = {
            let theme = cx.theme();
            (
                theme.muted_foreground,
                theme.foreground,
                theme.link,
                theme.link_hover,
                theme.link_active,
            )
        };
        // 信息稀疏的静态页：内容在视口内整体居中，固定内容远小于
        // 最小窗口尺寸，无滚动需求。
        v_flex().size_full().items_center().justify_center().child(
            content_column(tokens::widths::ABOUT_CONTENT)
                .items_center()
                .gap(tokens::spacing::LG)
                // 应用身份：图标 + 名称 + 版本 + 开发者
                .when_some(self.icon.clone(), |el, icon| {
                    el.child(img(icon).size(ICON_SIZE))
                })
                .child(
                    v_flex()
                        .items_center()
                        .gap(tokens::spacing::SM)
                        .child(
                            div()
                                .text_role(TextRole::SectionTitle)
                                .text_color(foreground)
                                .child("Fragmenta"),
                        )
                        .child(
                            div()
                                .text_role(TextRole::Body)
                                .text_color(muted)
                                .child(concat!("版本 ", env!("CARGO_PKG_VERSION"))),
                        )
                        .child(
                            div()
                                .text_role(TextRole::Body)
                                .text_color(muted)
                                .child("开发者 anewdo"),
                        ),
                )
                // 项目地址：完整 URL 文本独立成行，点击调用系统浏览器；
                // link 语义色含 hover / active 变化，悬停指针变手型
                .child(
                    h_flex()
                        .items_center()
                        .gap(tokens::spacing::XS)
                        .child(
                            div()
                                .text_role(TextRole::Body)
                                .text_color(muted)
                                .child("项目地址: "),
                        )
                        .child(
                            Link::new("about-repo")
                                .href(REPO_URL)
                                .open_with(|href, _, _, cx| cx.open_url(href))
                                .text_role(TextRole::Body)
                                .text_color(link)
                                .underline()
                                .cursor_pointer()
                                .hover(|style| style.text_color(link_hover))
                                .active(|style| style.text_color(link_active))
                                .child(REPO_URL),
                        ),
                )
                .child(
                    div()
                        .text_role(TextRole::Meta)
                        .text_color(muted)
                        .child(LICENSE_LINE),
                ),
        )
    }
}
