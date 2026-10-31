//! 笔记模型。

use chrono::{DateTime, Local};

/// 摘要截断长度（字符数）。
const SUMMARY_MAX_CHARS: usize = 50;

/// 笔记：核心内容实体，标题 + Markdown 正文，可挂多个标签（术语见 GLOSSARY.md）。
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub id: i64,
    pub title: String,
    /// Markdown 源文。
    pub content: String,
    /// 标签列表。
    pub tags: Vec<String>,
    pub created_at: DateTime<Local>,
    pub updated_at: DateTime<Local>,
}

impl Note {
    /// 正文摘要：回车替换为空格，按字符边界截断到 [`SUMMARY_MAX_CHARS`]（不破坏 CJK）；
    /// 显示层再按容器宽度换行并以省略号截断。
    ///
    /// 逐字符累积而非先拼接全文再截断：卡片每帧调用，长正文不应整段拷贝。
    pub fn summary(&self) -> String {
        let mut summary = String::with_capacity(SUMMARY_MAX_CHARS);
        let mut used = 0;
        for (ix, line) in self.content.lines().enumerate() {
            if ix > 0 {
                if used == SUMMARY_MAX_CHARS {
                    break;
                }
                summary.push(' ');
                used += 1;
            }
            for ch in line.chars() {
                if used == SUMMARY_MAX_CHARS {
                    return summary;
                }
                summary.push(ch);
                used += 1;
            }
        }
        summary
    }
}
