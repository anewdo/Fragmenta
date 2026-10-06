//! 笔记模型。

use chrono::{DateTime, Local};

/// 摘要截断长度（字符数）。
const SUMMARY_MAX_CHARS: usize = 50;

/// 笔记：核心内容实体，标题 + Markdown 正文，至多一个分类、可挂多个标签（术语见 GLOSSARY.md）。
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub id: i64,
    pub title: String,
    /// Markdown 源文。
    pub content: String,
    /// 分类名，单值归属；至多一个。
    pub category: Option<String>,
    /// 标签列表。
    pub tags: Vec<String>,
    pub created_at: DateTime<Local>,
    pub updated_at: DateTime<Local>,
}

impl Note {
    /// 正文摘要：回车替换为空格，截取前 50 个字符（按字符边界截断，不破坏 CJK）。
    pub fn summary(&self) -> String {
        self.content
            .lines()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(SUMMARY_MAX_CHARS)
            .collect()
    }
}
