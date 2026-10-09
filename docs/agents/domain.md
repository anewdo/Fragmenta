# 域文档

工程技能在探索代码库时应如何消费本仓库的领域文档。

## 探索之前，先读这些

- 仓库根目录的 `GLOSSARY.md`；若存在 `GLOSSARY-MAP.md`，则按其指向读取各相关上下文的 `GLOSSARY.md`
- `docs/adr/`：阅读与即将工作的领域相关的 ADR。多上下文仓库中，还需检查 `src/<context>/docs/adr/` 中上下文范围内的决策

若上述文件不存在，静默继续。不要标记其缺失，也不要预先建议创建。`/domain-modeling` 技能（经 `/grill-with-docs` 与 `/improve-codebase-architecture` 触达）会在术语或决策实际落定时惰性创建它们。

## 文件结构

单一上下文仓库（本仓库的布局）：

- 根目录 `GLOSSARY.md`
- `docs/adr/`：全部架构决策记录

多上下文仓库（根目录存在 `GLOSSARY-MAP.md` 时）：

- 根目录 `GLOSSARY-MAP.md`，指向每个上下文一份 `GLOSSARY.md`
- `docs/adr/`：系统级决策
- `src/<context>/GLOSSARY.md` 与 `src/<context>/docs/adr/`：上下文专属术语与决策

## 使用词汇表的术语

当输出涉及领域概念（issue 标题、重构提案、假设、测试名）时，使用 `GLOSSARY.md` 定义的术语。不要漂移到词汇表明确回避的同义词。

若所需概念尚未收入词汇表，这是一个信号：要么你在发明项目未使用的语言（请重新考虑），要么存在真实缺口（记下来交给 `/domain-modeling`）。

## 标记 ADR 冲突

若你的输出与现有 ADR 矛盾，显式指出，不要静默覆盖：

> _与 ADR-0007（事件溯源订单）矛盾，但值得重新讨论，因为……_
