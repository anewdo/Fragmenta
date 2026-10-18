//! 共享编辑能力：源文本输入、图库与 Markdown 图片解析。

mod image_path;
mod source;

pub use image_path::ImagePathPlugin;
pub use source::SourceText;

use fragmenta_core::media::Images;
use gpui_kit::Global;

/// 图片图库（D6 修订：`imgs/` 平铺于 `data_dir` 内，随迁移整体走）。
pub(crate) struct ImageStore(pub(crate) Images);

impl Global for ImageStore {}
