//! 嵌入应用图标（ADR-0011）：assets/app.ico 以资源 ID 1 写入 exe。
//! gpui-pre-windows 的窗口类图标经 LoadImageW(.., 1, IMAGE_ICON) 加载，
//! 资源管理器同样取最低 ID 图标，故 exe 图标与全部窗口图标同源。
//! 托盘图标由 tray.rs 运行时解码同一 .ico，替换素材后重新构建即全量生效。

fn main() {
    println!("cargo:rerun-if-changed=../../assets/app.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("../../assets/app.ico");
        resource
            .compile()
            .expect("嵌入 assets/app.ico 失败（确认文件存在且为合法多尺寸 .ico）");
    }
}
