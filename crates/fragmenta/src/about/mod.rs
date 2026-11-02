//! 关于页：应用身份与项目地址

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

/// 应用图标纹理边长：显示尺寸的 2 倍，覆盖 200% 缩放下的 1:1 采样。
const ICON_TEXTURE: u32 = 128;

/// 关于页视图：静态信息页，常驻于主窗口 shell；无状态、无订阅。
pub(crate) struct AboutPage {
    /// 应用图标：与 exe 资源、托盘同源的内嵌 `.ico` 解码所得。
    icon: Option<Arc<RenderImage>>,
}

impl AboutPage {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self {
            icon: crate::utils::app_icon::decode_render_image(ICON_TEXTURE),
        }
    }
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
