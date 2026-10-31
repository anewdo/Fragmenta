# 单 exe 免安装分发与 GitHub Actions 发布

应用接近完工，需要确定分发形态、版本管理与 CI/CD。项目定位轻量低占用，gpui-kit 官方文档认可 Windows 便携单 exe 为合法发布形态（"A ZIP can be enough for a portable app"），且数据目录本就锚定 exe 旁（ADR-0004），免安装与数据模型天然契合。经权衡确定以下方案。

定案一：分发形态为 Windows x64 单 exe 免安装，经 GitHub Release 分发。每个版本附带三样产物：`Fragmenta_vX.Y.Z_x64.exe`、`SHA256SUMS.txt`（sha256 校验清单，格式同 Tauri 项目）与 `LICENSE` 文件（MIT，版权人 niuniuzz，2026）。不制作 installer，不做代码签名；README 与 Release notes 说明 SmartScreen「更多信息 → 仍要运行」的绕过方式。

定案二：不做应用内检查更新。理由：目标用户以国内为主，GitHub 连通性不可控使检查成功率无法保证；为这一单一功能引入 ureq+rustls 约 1MB 依赖与轻量定位冲突。更新告知由关于页（主窗口独立导航页）承担：应用图标、版本号（取自 `env!("CARGO_PKG_VERSION")`）、开发者 GitHub 用户名、指向 Releases 的仓库链接与 LICENSE 链接，链接一律以完整 URL 文本独立成行显示。更新流程为用户手动：退出应用 → 经关于页链接前往 GitHub 下载 → 覆盖 exe。

定案三：版本与构建。根 `Cargo.toml` 设 `[workspace.package] version`，两 crate 以 `version = { workspace = true }` 引用，代码经 `env!("CARGO_PKG_VERSION")` 读取；`git tag v*` 推送触发发布，首版 `v0.1.0`。`[profile.release]` 取 `lto = "fat"`、`codegen-units = 1`、`strip = true`，`opt-level` 保持默认 3（输入延迟等运行性能优先于极限体积）；不设 `panic = "abort"`——gpui-pre 在运行时路径依赖 `catch_unwind` 兜住文本排版与卸载阶段的 panic，abort 会把这些可恢复 panic 升级为进程死亡。CI 构建命令为 `cargo build --release --locked`，`Cargo.lock` 入库。两条 workflow：`ci.yml` 于 push/PR 到 main 在 windows-latest 跑 `cargo fmt --check`、`cargo clippy -D warnings` 与全量测试；`release.yml` 由 `v*` tag 触发，构建后以 `gh release create` 发布产物，notes 自 commit 生成。图标：多尺寸 `.ico`（16/24/32/48/64/128/256 七档，32-bit RGBA，256 档 PNG 压缩）以资源 ID 1 经 `build.rs` + `winresource` 嵌入 exe——gpui-pre-windows 的窗口类图标即 `LoadImageW(.., 1, IMAGE_ICON)`，资源管理器亦取最低 ID 图标，故 exe、主窗口/磁贴窗口共用此资源；托盘运行时经 image 解码同一 `.ico` 的最高分辨率帧缩放至 32px。先以占位 `.ico` 接通管线，正式素材放入 `assets/app.ico` 原位替换即可，无需改代码。

代价与约束（已知限制，接受）：

- Windows 锁定运行中的 exe，覆盖更新前必须先退出应用。
- autostart 注册表 Run 键写死 exe 绝对路径，用户将新版解压到新目录后自启动失效，需在设置中重新开启。
- 关于页使主窗口导航从四页变为五页（笔记、待办、标签、设置、关于），GLOSSARY 已同步。
- 无签名发布的 SmartScreen 警告将随下载量积累信誉后放宽，短期内以文档说明兜底。

状态：accepted（2026-10-31）
