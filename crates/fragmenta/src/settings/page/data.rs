//! 数据组：数据目录展示 / 迁移 / 重置与笔记 Markdown 导出。

use std::path::PathBuf;

use fragmenta_core::export::{ExportFailure, ExportReport};
use fragmenta_core::model::NoteFilter;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::SettingsPage;
use super::common::{error_text, setting_row};
use crate::editor::ImageStore;
use crate::notes::NotesStore;
use crate::settings::SettingsStore;
use crate::ui::layout::{CardStyle, setting_card};
use crate::ui::tokens;
use crate::widgets::toast::{Toast, ToastKind, ToastStore};

impl SettingsPage {
    /// "数据"组：当前目录（可打开、可更改位置）／重置为默认位置／
    /// 笔记导出为 md；错误红字展示。
    pub(super) fn data_group(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let card = CardStyle::from_theme(cx.theme());
        let (muted, danger) = {
            let colors = cx.theme();
            (colors.muted_foreground, colors.danger)
        };
        let current = cx
            .global::<SettingsStore>()
            .0
            .read(cx)
            .data_dir()
            .display()
            .to_string();
        let error = self.data_dir_error.clone();

        v_flex()
            .w_full()
            .gap(tokens::spacing::SM)
            // 行 1：当前数据存储目录 + 打开 / 更改位置（目录选择器选完即迁移）
            .child(setting_card(
                card,
                setting_row(
                    IconName::FolderOpen,
                    "当前数据存储目录",
                    v_flex()
                        .gap(tokens::spacing::XS)
                        .child(current)
                        .child("更改位置时选择空目录，旧目录的数据文件随后清理"),
                    muted,
                    h_flex()
                        .gap(tokens::spacing::SM)
                        .child(Button::new("data-dir-open").label("打开当前目录").on_click(
                            |_, _, cx| {
                                let dir = cx
                                    .global::<SettingsStore>()
                                    .0
                                    .read(cx)
                                    .data_dir()
                                    .to_path_buf();
                                crate::win32::open_folder(&dir);
                            },
                        ))
                        .child(
                            Button::new("data-dir-change")
                                .label("更改存储路径")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    // 系统目录选择器（COM IFileOpenDialog）；取消则不迁移
                                    let owner = crate::win32::window_hwnd(window);
                                    let Some(picked) = crate::win32::pick_folder(owner) else {
                                        return;
                                    };
                                    let store = cx.global::<SettingsStore>().0.clone();
                                    match store
                                        .update(cx, |state, cx| state.set_data_dir(picked, cx))
                                    {
                                        Ok(()) => this.data_dir_error = None,
                                        Err(err) => this.data_dir_error = Some(err.to_string()),
                                    }
                                    cx.notify();
                                })),
                        ),
                ),
            ))
            // 行 2：重置为默认位置（exe 旁 data/）
            .child(setting_card(
                card,
                setting_row(
                    IconName::RotateCcw,
                    "重置为默认位置",
                    "将数据存储位置重置为 exe 所在目录",
                    muted,
                    Button::new("data-dir-reset")
                        .label("重置")
                        .on_click(cx.listener(|this, _, _, cx| {
                            // 默认位置与启动逻辑同源（{exe_dir}/data）
                            let target = fragmenta_core::settings::Settings::default().data_dir;
                            let store = cx.global::<SettingsStore>().0.clone();
                            let already_default =
                                store.read_with(cx, |state, _| state.data_dir() == target);
                            if already_default {
                                return;
                            }
                            match store.update(cx, |state, cx| state.set_data_dir(target, cx)) {
                                Ok(()) => this.data_dir_error = None,
                                Err(err) => this.data_dir_error = Some(err.to_string()),
                            }
                            cx.notify();
                        })),
                ),
            ))
            // 行 3：笔记导出为 md——目录选择器选目录，后台导出，toast 报告
            .child(setting_card(
                card,
                setting_row(
                    IconName::FileText,
                    "笔记导出为 md",
                    "将全部笔记导出为 Markdown 文件到所选目录",
                    muted,
                    Button::new("data-export-md")
                        .label("导出")
                        .on_click(cx.listener(|_, _, window, cx| {
                            // 系统目录选择器（COM IFileOpenDialog）；取消则不导出
                            let owner = crate::win32::window_hwnd(window);
                            let Some(out_dir) = crate::win32::pick_folder(owner) else {
                                return;
                            };
                            run_export(out_dir, window, cx);
                        })),
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

/// 执行导出：待写草稿先落库并直查全量笔记，导出在后台线程进行
/// （应用可用不阻塞），完成后以 toast 报告（笔记数 / 资产数 / 失败项）。
fn run_export(out_dir: PathBuf, window: &mut Window, cx: &mut Context<SettingsPage>) {
    // toast 只在触发窗口渲染：先取本窗口句柄供后台完成后回填
    let window = window.window_handle();
    let notes_store = cx.global::<NotesStore>().0.clone();
    // 导出内容与界面一致：先落库全部待写草稿（失败槽保留重试，不影响导出）
    let _ = notes_store.update(cx, |state, cx| state.flush_all(cx));
    let query = notes_store.update(cx, |state, _| state.notes(&NoteFilter::default()));
    let images = cx.global::<ImageStore>().0.clone();
    let toast = cx.global::<ToastStore>().0.clone();
    match query {
        Ok(notes) => {
            // 导出在后台线程执行；完成后回主线程 toast 报告
            let export = cx.background_spawn(async move {
                fragmenta_core::export::export_notes(&notes, &out_dir, &images)
            });
            cx.spawn(async move |_, cx| {
                let report = export.await;
                toast.update(cx, |state, cx| {
                    state.show(export_result_toast(report), window, cx);
                });
            })
            .detach();
        }
        Err(err) => {
            toast.update(cx, |state, cx| {
                state.show(
                    Toast::report(
                        "导出失败",
                        Some(format!("读取笔记列表失败：{err}")),
                        ToastKind::Error,
                    ),
                    window,
                    cx,
                );
            });
        }
    }
}

/// 导出结果 → toast：成功项计数入标题，失败项入明细（最多列 3 条，余下计数）。
fn export_result_toast(result: fragmenta_core::export::Result<ExportReport>) -> Toast {
    match result {
        Ok(report) => {
            let failures = report.failures.len();
            let title = if failures == 0 {
                format!("已导出 {} 篇笔记、{} 张图片", report.notes, report.assets)
            } else {
                format!(
                    "已导出 {} 篇笔记、{} 张图片，{} 项失败",
                    report.notes, report.assets, failures
                )
            };
            let detail = (failures > 0).then(|| {
                let mut lines: Vec<String> = report
                    .failures
                    .iter()
                    .take(3)
                    .map(|failure| match failure {
                        ExportFailure::Note { title, error, .. } => {
                            format!("《{title}》：{error}")
                        }
                        ExportFailure::Asset { reference, error } => {
                            format!("{reference}：{error}")
                        }
                    })
                    .collect();
                let rest = failures - lines.len();
                if rest > 0 {
                    lines.push(format!("……另有 {rest} 项失败"));
                }
                lines.join("\n")
            });
            Toast::report(
                title,
                detail,
                if failures > 0 {
                    ToastKind::Warn
                } else {
                    ToastKind::Success
                },
            )
        }
        // 批次级失败（输出目录不可建立 / 不可写）：尚未写出任何文件，无半成品
        Err(err) => Toast::report(
            "导出失败",
            Some(format!("无法写入目标目录：{err}")),
            ToastKind::Error,
        ),
    }
}
