//! 应用图标共享解码：与 exe 资源、托盘同源的内嵌 `.ico`。

use std::sync::Arc;

use gpui_kit::RenderImage;

/// 解码内嵌 `.ico` 为渲染图像：image 的 ICO 解码取最高分辨率帧（256px），
/// 按 `target_px` 在 CPU 端预缩放，避免 GPU 无 mipmap 的大倍率缩小产生锯齿；
/// RGBA 需转 BGRA（gpui 渲染层取样序，同其 `decode_static_image_from_decoder`）。
/// 失败不致命：仅图标缺席，调用方界面照常展示。
pub(crate) fn decode_render_image(target_px: u32) -> Option<Arc<RenderImage>> {
    let decoded = image::load_from_memory_with_format(
        include_bytes!("../../../../assets/app.ico"),
        image::ImageFormat::Ico,
    )
    .map_err(|err| eprintln!("[fragmenta] 应用图标解码失败: {err}"))
    .ok()?;
    let resized = decoded.resize_exact(target_px, target_px, image::imageops::FilterType::Lanczos3);
    let mut bgra = resized.into_rgba8();
    for pixel in bgra.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(bgra)])))
}
