//! Spike 6 — tray-icon 与 gpui Windows 消息循环共存性验证（Phase 13 第一步）
//!
//! 验证点：
//! - 托盘图标在 gpui 应用运行期间常驻（窗口关闭后仍在）
//! - 左键单击托盘 → 事件经 channel 回到 gpui 主循环，翻转窗口内开关（证明事件链路通）
//! - 右键菜单项事件同样可达并驱动界面
//! - 托盘运行期间，既有全局热键（ctrl+alt+n 呼出窗口）不受干扰
//! - 「移除托盘图标」后图标消失、无崩溃；「重建」可反复创建（无泄漏迹象）
//!
//! 关键机制：tray-icon 在 Windows 上要求创建线程有 Win32 消息循环；
//! gpui Windows 后端主线程就是 GetMessage/DispatchMessage 泵（platform.rs run()），
//! 故托盘在主线程创建后其内部消息窗口由 gpui 的泵分发——本 spike 验证此共存。
//!
//! 操作步骤：
//! 1. 运行本程序：出现说明窗口，系统托盘出现绿色圆点图标（进程常驻，QuitMode::Explicit）
//! 2. 关闭说明窗口 —— 预期：托盘图标仍在（进程驻留）
//! 3. 按 ctrl+alt+n —— 预期：窗口重新呼出（热键不回归）
//! 4. 左键单击托盘图标 —— 预期：窗口内开关翻转（若窗口已关，先 ctrl+alt+n 呼出再点）
//! 5. 右键托盘 → 「切换开关」—— 预期：开关同样翻转
//! 6. 右键托盘 → 「重建托盘图标」—— 预期：图标消失后立即重现，可反复多轮（无泄漏迹象）
//! 7. 右键托盘 → 「移除托盘图标」—— 预期：图标消失且不再重现，进程不崩、热键仍有效
//!    （移除后无菜单入口，退出只能靠热键）
//! 8. 测试收尾：ctrl+alt+q 或菜单「退出进程」—— 预期：进程完全退出、托盘消失
//!
//! 注意：移除托盘后就没有菜单入口了，退出只能靠 ctrl+alt+q；测试完务必收尾。

#[path = "_common/mod.rs"]
mod common;

use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::prelude::*;
use gpui_kit::*;
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{MouseButton, TrayIcon, TrayIconBuilder, TrayIconEvent};

/// 主循环统一事件：托盘交互与菜单点击经转发线程汇入同一 smol channel。
enum SpikeEvent {
    Tray(TrayIconEvent),
    Menu(MenuEvent),
}

/// 托盘对象与窗口视图弱引用同存 gpui 全局（TrayIcon 非 Send，
/// 但 gpui 全局仅主循环线程访问；创建 / drop 均在该线程完成）。
struct SpikeState {
    tray: Option<TrayIcon>,
    view: Option<WeakEntity<Spike6View>>,
}
impl Global for SpikeState {}

struct Spike6View {
    toggled: bool,
    tray_clicks: u32,
    menu_clicks: u32,
}

impl Render for Spike6View {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tray_present = cx.global::<SpikeState>().tray.is_some();
        div()
            .flex()
            .flex_col()
            .size_full()
            .items_center()
            .justify_center()
            .gap_3()
            .p_6()
            .child(div().text_size(px(22.)).child("Spike 6 — 托盘共存性验证"))
            .child(
                div()
                    .text_size(px(14.))
                    .child(format!("托盘图标：{}", if tray_present { "存在" } else { "已移除" })),
            )
            .child(
                div()
                    .text_size(px(14.))
                    .child(format!(
                        "左键点击次数 {} / 菜单切换次数 {}",
                        self.tray_clicks, self.menu_clicks
                    )),
            )
            .child(
                div()
                    .text_size(px(14.))
                    .child("左键单击托盘或菜单「切换开关」→ 下方开关翻转"),
            )
            .child(
                Checkbox::new("tray-toggle")
                    .checked(self.toggled)
                    .on_click({
                        let this = cx.entity().downgrade();
                        move |checked: &bool, _window, cx| {
                            if let Some(this) = this.upgrade() {
                                this.update(cx, |v, _| v.toggled = *checked);
                            }
                        }
                    }),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .child("热键：ctrl+alt+n 呼出本窗口；ctrl+alt+q 完全退出"),
            )
    }
}

fn open_help(cx: &mut App) {
    let bounds = Bounds::centered(None, size(px(460.), px(320.)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some("Spike 6 — 托盘共存性".into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    if let Ok((_, view)) = gpui_kit::open_window(options, cx, |_, cx| {
        cx.new(|_| Spike6View {
            toggled: false,
            tray_clicks: 0,
            menu_clicks: 0,
        })
    }) {
        cx.global_mut::<SpikeState>().view = Some(view.downgrade());
    }
}

/// 32×32 纯色圆点占位图标（正式图标资源不在本计划范围）。
fn placeholder_icon() -> tray_icon::Icon {
    const S: u32 = 32;
    let center = S as f32 / 2.0 - 0.5;
    let radius = S as f32 / 2.0 - 2.0;
    let mut rgba = Vec::with_capacity((S * S * 4) as usize);
    for y in 0..S {
        for x in 0..S {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let inside = dx * dx + dy * dy <= radius * radius;
            // 圆内豆沙绿，圆外透明
            rgba.extend_from_slice(if inside {
                &[0x7f, 0xa8, 0x6a, 0xff]
            } else {
                &[0, 0, 0, 0]
            });
        }
    }
    tray_icon::Icon::from_rgba(rgba, S, S).expect("占位图标构建失败")
}

fn create_tray() -> TrayIcon {
    let menu = Menu::new();
    let toggle = MenuItem::with_id("toggle", "切换开关", true, None);
    let recreate = MenuItem::with_id("recreate", "重建托盘图标", true, None);
    let remove = MenuItem::with_id("remove", "移除托盘图标", true, None);
    let quit = MenuItem::with_id("quit", "退出进程", true, None);
    menu.append_items(&[&toggle, &recreate, &remove, &quit])
        .expect("构建托盘菜单失败");

    TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Spike 6 — 托盘共存性验证")
        .with_icon(placeholder_icon())
        .build()
        .expect("创建托盘失败")
}

/// 托盘事件（crossbeam）与菜单事件分别起线程阻塞读，转发进同一 smol channel。
fn spawn_event_forwarders(tx: smol::channel::Sender<SpikeEvent>) {
    let tx_tray = tx.clone();
    std::thread::spawn(move || {
        let receiver = TrayIconEvent::receiver();
        while let Ok(event) = receiver.recv() {
            let _ = tx_tray.try_send(SpikeEvent::Tray(event));
        }
    });
    std::thread::spawn(move || {
        let receiver = MenuEvent::receiver();
        while let Ok(event) = receiver.recv() {
            let _ = tx.try_send(SpikeEvent::Menu(event));
        }
    });
}

fn toggle_from_tray(source: &str, cx: &mut App) {
    let Some(view) = cx.global::<SpikeState>().view.clone() else {
        return;
    };
    if let Some(view) = view.upgrade() {
        view.update(cx, |v, cx| {
            v.toggled = !v.toggled;
            if source == "tray" {
                v.tray_clicks += 1;
            } else {
                v.menu_clicks += 1;
            }
            cx.notify();
        });
    }
}

fn handle_event(event: SpikeEvent, cx: &mut App) {
    match event {
        SpikeEvent::Tray(TrayIconEvent::Click { button, .. }) => {
            if button == MouseButton::Left {
                println!("[spike6] 托盘左键点击 → 翻转开关");
                toggle_from_tray("tray", cx);
            }
        }
        SpikeEvent::Menu(menu_event) => match menu_event.id.0.as_str() {
            "toggle" => {
                println!("[spike6] 菜单「切换开关」→ 翻转开关");
                toggle_from_tray("menu", cx);
            }
            "recreate" => {
                println!("[spike6] 菜单「重建托盘图标」");
                let state = cx.global_mut::<SpikeState>();
                state.tray = None;
                state.tray = Some(create_tray());
                notify_view(cx);
            }
            "remove" => {
                println!("[spike6] 菜单「移除托盘图标」→ drop TrayIcon");
                cx.global_mut::<SpikeState>().tray = None;
                notify_view(cx);
            }
            "quit" => {
                println!("[spike6] 菜单「退出进程」→ cx.quit()");
                cx.quit();
            }
            other => println!("[spike6] 未知菜单 id: {other}"),
        },
        _ => {}
    }
}

fn notify_view(cx: &mut App) {
    if let Some(view) = cx
        .global::<SpikeState>()
        .view
        .clone()
        .and_then(|view| view.upgrade())
    {
        view.update(cx, |_, cx| cx.notify());
    }
}

fn main() {
    gpui_kit::application().run(|cx| {
        gpui_kit::init(cx);
        // Explicit：关窗进程驻留，托盘常驻的前提
        cx.set_quit_mode(QuitMode::Explicit);
        cx.set_global(SpikeState {
            tray: None,
            view: None,
        });

        // 托盘在主线程（gpui 消息泵线程）创建，消息由 gpui 的 GetMessage 泵分发
        cx.global_mut::<SpikeState>().tray = Some(create_tray());

        // 既有热键链路（与托盘并行，验证互不干扰）
        let rx = common::register_hotkeys(&["ctrl+alt+n", "ctrl+alt+q"]);
        common::spawn_hotkey_loop(rx, cx, |idx, cx| match idx {
            0 => open_help(cx),
            1 => {
                println!("[spike6] ctrl+alt+q → cx.quit()，进程退出");
                cx.quit();
            }
            _ => {}
        });

        // 托盘事件链路：channel → cx.spawn 主循环消费（与热键同构）
        let (tx, tray_rx) = smol::channel::unbounded::<SpikeEvent>();
        spawn_event_forwarders(tx);
        cx.spawn(async move |cx| {
            while let Ok(event) = tray_rx.recv().await {
                cx.update(|cx| handle_event(event, cx));
            }
        })
        .detach();

        open_help(cx);
    });
}
