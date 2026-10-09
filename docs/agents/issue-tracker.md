# Issue 跟踪器：本地 Markdown

本仓库的 issue 与规格说明以 markdown 文件形式存放在 `.scratch/` 下。

## 约定

- 每个功能一个目录：`.scratch/<feature-slug>/`
- 规格说明为 `.scratch/<feature-slug>/spec.md`
- 实现 issue 为每个 ticket 一个文件，位于 `.scratch/<feature-slug>/issues/<NN>-<slug>.md`，从 `01` 起编号，绝不合并为单个 tickets 文件
- Triage 状态记录在每个 issue 文件顶部附近的 `Status:` 行（角色字符串见 `triage-labels.md`）
- 评论与对话历史追加到文件底部 `## Comments` 标题之下

## 当技能说 "publish to the issue tracker"

在 `.scratch/<feature-slug>/` 下创建新文件（必要时创建目录）。

## 当技能说 "fetch the relevant ticket"

读取所引用路径的文件。用户通常会直接给出路径或 issue 编号。

## Wayfinding 操作

供 `/wayfinder` 使用。**map** 是一个文件，每个 ticket 对应一个 **child** 文件。

- **Map**：`.scratch/<effort>/map.md`（Notes / Decisions-so-far / Fog 正文）。
- **Child ticket**：`.scratch/<effort>/issues/NN-<slug>.md`，从 `01` 起编号，正文含问题。`Type:` 行记录 ticket 类型（`research`/`prototype`/`grilling`/`task`）；`Status:` 行记录 `claimed`/`resolved`。
- **Blocking**：文件顶部附近的 `Blocked by: NN, NN` 行。其所列文件全部 `resolved` 时，该 ticket 解除阻塞。
- **Frontier**：扫描 `.scratch/<effort>/issues/` 中开放、未阻塞、未认领的文件；编号最小者优先。
- **Claim**：先设 `Status: claimed` 并保存，再开始任何工作。
- **Resolve**：在 `## Answer` 标题下追加答案，设 `Status: resolved`，然后向 `map.md` 的 Decisions-so-far 追加一条上下文指针（要点 + 链接）。
