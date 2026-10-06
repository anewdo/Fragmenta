//! 导出笔记为 `.md` 文件（架构 §5.5）。
//!
//! 每篇笔记一个 `.md`——文件名取标题（非法字符替换、空标题回退、重名追加 id）；
//! 整个导出批次共用一个 `assets/`：正文中被引用的 `imgs/..` 改写为 `assets/..`，
//! 仅被引用的图库图片拷入；绝对路径引用原样保留。无导入功能。
//!
//! 改写目标 = markdown 链接 / 图片目标（`](imgs/..)` 中至 `)` 或空白前的路径段）；
//! 纯文本提及与越界相对路径不参与改写。

use std::collections::HashSet;
use std::path::Path;

use crate::media::Images;
use crate::model::Note;

/// export 层错误（批次级失败：输出目录无法建立等）。
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("IO 错误：{0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, ExportError>;

/// 导出失败项（单项失败只记录，不中断批次）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportFailure {
    /// 笔记 `.md` 写出失败。
    Note {
        id: i64,
        title: String,
        error: String,
    },
    /// 被引用的图库图片拷贝失败（源缺失或 IO 错误）。
    Asset { reference: String, error: String },
}

/// 导出结果报告。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExportReport {
    /// 成功写出的笔记 `.md` 数。
    pub notes: usize,
    /// 成功拷入 `assets/` 的图片数（按图库引用去重）。
    pub assets: usize,
    /// 失败项明细。
    pub failures: Vec<ExportFailure>,
}

/// 导出一批笔记到 `out_dir`（目录不存在时自动创建）。
///
/// 批次级目录失败返回 `Err`；单篇写出 / 单图拷贝失败记入报告，不中断批次。
pub fn export_notes(notes: &[Note], out_dir: &Path, images: &Images) -> Result<ExportReport> {
    let assets_dir = out_dir.join("assets");
    std::fs::create_dir_all(&assets_dir)?;

    let mut report = ExportReport::default();

    // 改写各篇正文并收集被引用的图库图片（整个批次去重，同名只拷一份）
    let mut contents = Vec::with_capacity(notes.len());
    let mut gallery_refs: Vec<String> = Vec::new();
    let mut seen_refs: HashSet<String> = HashSet::new();
    for note in notes {
        let (text, refs) = rewrite_content(&note.content);
        contents.push(text);
        for r in refs {
            if seen_refs.insert(r.clone()) {
                gallery_refs.push(r);
            }
        }
    }

    // 仅被引用的图库图片拷入 assets/（保持图库内相对结构）
    for r in &gallery_refs {
        let source = images.resolve(r).expect("收集阶段保证 imgs/ 前缀必可解析");
        let target = assets_dir.join(&r["imgs/".len()..]);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        match std::fs::copy(&source, &target) {
            Ok(_) => report.assets += 1,
            Err(e) => report.failures.push(ExportFailure::Asset {
                reference: r.clone(),
                error: e.to_string(),
            }),
        }
    }

    // 每篇笔记一个 .md
    let mut used_names: HashSet<String> = HashSet::new();
    for (note, content) in notes.iter().zip(&contents) {
        let name = note_filename(&note.title, note.id, &mut used_names);
        match std::fs::write(out_dir.join(format!("{name}.md")), content) {
            Ok(()) => report.notes += 1,
            Err(e) => report.failures.push(ExportFailure::Note {
                id: note.id,
                title: note.title.clone(),
                error: e.to_string(),
            }),
        }
    }

    Ok(report)
}

/// 改写正文：markdown 链接 / 图片目标中 `imgs/` 前缀的引用改写为 `assets/`，
/// 返回（新正文, 被引用的图库目标全名列表，按出现顺序去重前）。
fn rewrite_content(content: &str) -> (String, Vec<String>) {
    let mut out = String::with_capacity(content.len());
    let mut refs = Vec::new();
    let mut cursor = 0; // 已拷贝到的字节位置
    let mut from = 0; // 下一次搜索的起点
    while let Some(pos) = content[from..].find("](imgs/") {
        let start = from + pos + 2; // 目标（imgs/…）起点
        let rest = &content[start..];
        // markdown 目标不含未转义空白，终止于 ')' 或首个空白
        let end = rest
            .find(|c: char| c == ')' || c.is_whitespace())
            .unwrap_or(rest.len());
        let token = &rest[..end];
        out.push_str(&content[cursor..start]);
        out.push_str("assets/");
        out.push_str(&token["imgs/".len()..]);
        if token.len() > "imgs/".len() {
            refs.push(token.to_string());
        }
        cursor = start + end;
        from = cursor;
    }
    out.push_str(&content[cursor..]);
    (out, refs)
}

/// 笔记导出文件名（不含扩展名）：标题规范化 → 空标题回退 → 撞名或 Windows
/// 保留名追加 id。候选序列 `{base}` → `{base}-{id}` → `{base}-{id}-2` → …（保证终止）。
fn note_filename(title: &str, id: i64, used: &mut HashSet<String>) -> String {
    let mut base = sanitize_filename(title);
    if base.is_empty() {
        base = "未命名".to_string();
    }
    // 超长截断：最坏情形（字符全为代理对）100 字符占 200 个 UTF-16 单元，
    // 连同 id 后缀与 .md 仍低于 NTFS 255 单元的分量上限
    if base.chars().count() > 100 {
        base = base.chars().take(100).collect();
    }
    let mut candidate = base.clone();
    let mut seq: u32 = 0;
    while used.contains(&candidate) || (seq == 0 && is_windows_reserved(&candidate)) {
        seq += 1;
        candidate = if seq == 1 {
            format!("{base}-{id}")
        } else {
            format!("{base}-{id}-{seq}")
        };
    }
    used.insert(candidate.clone());
    candidate
}

/// 文件名规范化：Windows 非法字符（`< > : " / \ | ? *` 与控制字符）替换为 `_`，
/// 去除首尾空白与句点（尾部句点 / 空白在 Windows 上非法）。
fn sanitize_filename(title: &str) -> String {
    title
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
                '_'
            } else {
                c
            }
        })
        .collect::<String>()
        .trim_matches(|c: char| c == ' ' || c == '.')
        .to_string()
}

/// Windows 保留设备名（大小写不敏感）：作为文件名主干时建不出普通文件。
fn is_windows_reserved(stem: &str) -> bool {
    matches!(
        stem.to_ascii_uppercase().as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}
