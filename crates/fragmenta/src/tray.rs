//! 托盘驻留：进程运行期间图标常驻。

#[cfg(windows)]
mod imp {
    use gpui_kit::{App, Global};
    use tray_icon::menu::{Menu, MenuEvent, MenuItem};
    use tray_icon::{MouseButton, TrayIcon, TrayIconBuilder, TrayIconEvent};

    use crate::app::quit;
    use crate::windows;

    /// 托盘对象存全局（非 `Send`，仅 gpui 主循环线程访问）。
    /// 字段持有即保活（drop 会导致图标移除），无读取方。
    struct TrayGlobal(#[allow(dead_code)] TrayIcon);

    impl Global for TrayGlobal {}

    /// 主循环统一事件：托盘交互与菜单点击经转发线程汇入同一 smol channel。
    enum TrayAction {
        OpenMain,
        Quit,
    }

    /// 初始化托盘：创建图标 + 菜单，并启动事件转发与主循环消费。
    /// 创建失败不致命（应用照常运行，仅无托盘）。
    pub fn init(cx: &mut App) {
        let tray = match create_tray() {
            Ok(tray) => tray,
            Err(err) => {
                eprintln!("[fragmenta] 托盘创建失败（应用继续运行）: {err}");
                return;
            }
        };
        cx.set_global(TrayGlobal(tray));
        let (tx, rx) = smol::channel::unbounded::<TrayAction>();
        spawn_event_forwarders(tx);

        cx.spawn(async move |cx| {
            while let Ok(action) = rx.recv().await {
                cx.update(|cx| match action {
                    TrayAction::OpenMain => windows::main::open(cx),
                    TrayAction::Quit => quit::request_quit(cx),
                });
            }
        })
        .detach();
    }

    /// 应用图标（与 exe 资源、窗口图标同源）。image 的 ICO 解码取最高
    /// 分辨率帧，缩放到托盘尺寸后交给 tray-icon。
    fn app_icon() -> Result<tray_icon::Icon, Box<dyn std::error::Error>> {
        const TRAY_SIZE: u32 = 32;
        let decoded = image::load_from_memory_with_format(
            include_bytes!("../../../assets/app.ico"),
            image::ImageFormat::Ico,
        )
        .map_err(|err| format!("解码 assets/app.ico 失败: {err}"))?;
        let rgba = decoded
            .resize_exact(TRAY_SIZE, TRAY_SIZE, image::imageops::FilterType::Lanczos3)
            .to_rgba8();
        tray_icon::Icon::from_rgba(rgba.into_raw(), TRAY_SIZE, TRAY_SIZE)
            .map_err(|err| format!("托盘图标构建失败: {err}").into())
    }

    fn create_tray() -> Result<TrayIcon, Box<dyn std::error::Error>> {
        let menu = Menu::new();
        let open = MenuItem::with_id("open", "打开主窗口", true, None);
        let quit = MenuItem::with_id("quit", "退出", true, None);
        menu.append_items(&[&open, &quit])?;

        Ok(TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            // 默认 true：左键也会弹菜单；关闭后左键只发 Click 事件
            .with_menu_on_left_click(false)
            .with_tooltip("Fragmenta")
            .with_icon(app_icon()?)
            .build()?)
    }

    /// 托盘事件（左键）与菜单事件分别起线程阻塞读 crossbeam，
    /// 映射为语义动作后转发进同一 smol channel。
    fn spawn_event_forwarders(tx: smol::channel::Sender<TrayAction>) {
        let tx_tray = tx.clone();
        std::thread::spawn(move || {
            let receiver = TrayIconEvent::receiver();
            while let Ok(event) = receiver.recv() {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    ..
                } = event
                {
                    let _ = tx_tray.try_send(TrayAction::OpenMain);
                }
            }
        });
        std::thread::spawn(move || {
            let receiver = MenuEvent::receiver();
            while let Ok(event) = receiver.recv() {
                let action = match event.id.0.as_str() {
                    "open" => TrayAction::OpenMain,
                    "quit" => TrayAction::Quit,
                    _ => continue,
                };
                let _ = tx.try_send(action);
            }
        });
    }
}

#[cfg(windows)]
pub use imp::init;

/// 非 Windows：无托盘，空实现（编译不破坏）。
#[cfg(not(windows))]
pub fn init(_cx: &mut gpui_kit::App) {}
