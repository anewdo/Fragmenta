# 防抖槽按笔记身份分区

多个磁贴可并发编辑各自的笔记。若 NotesState 只设单一 pending 槽与一个 300ms 定时器，后到的 save 会重置定时器并顶掉先到的草稿，先到的内容静默丢失。故防抖按笔记身份分区（`HashMap<NoteKey, PendingWrite>`）：已落库笔记用 id，未落库草稿用调用方稳定标识；`Event::Saved` 携带 key 供磁贴匹配。磁贴与编辑器共用的是防抖实现，计时互相独立。

状态：accepted（2026-10-05）
