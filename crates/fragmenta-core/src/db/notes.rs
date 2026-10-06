//! 笔记数据访问（架构 §5.2）：upsert / delete / 筛选查询。
//!
//! upsert 只写标题与正文；分类 / 标签由 taxonomy 接口管理（Phase 4），
//! 查询侧经 JOIN 读回完整 `Note`。

use std::collections::HashMap;

use chrono::{DateTime, Local};
use rusqlite::{Connection, params, params_from_iter};

use super::{Db, DbError, Result};
use crate::model::{Note, NoteFilter, SearchScope};

impl Db {
    /// 插入或更新笔记：`id == 0` 视为新笔记插入，否则按 id 更新。
    ///
    /// 回填 id 与 created_at / updated_at（以库中数据为准）；更新不覆盖 created_at。
    pub fn upsert_note(&self, note: &mut Note) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Local::now().to_rfc3339();
        if note.id == 0 {
            conn.execute(
                "INSERT INTO notes (title, content, category_id, created_at, updated_at)
                 VALUES (?1, ?2, NULL, ?3, ?3)",
                params![note.title, note.content, now],
            )?;
            note.id = conn.last_insert_rowid();
        } else {
            let changed = conn.execute(
                "UPDATE notes SET title = ?1, content = ?2, updated_at = ?3 WHERE id = ?4",
                params![note.title, note.content, now, note.id],
            )?;
            if changed == 0 {
                return Err(DbError::NoteNotFound(note.id));
            }
        }
        let (created, updated): (String, String) = conn.query_row(
            "SELECT created_at, updated_at FROM notes WHERE id = ?1",
            params![note.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        note.created_at = parse_timestamp(&created)?;
        note.updated_at = parse_timestamp(&updated)?;
        Ok(())
    }

    /// 删除笔记；note_tags 经外键级联清理。目标不存在时为无操作。
    pub fn delete_note(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM notes WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// 按条件查询笔记（updated_at 倒序，同刻按 id 倒序），
    /// 分类与标签经 JOIN 读回；标签按名称排序。
    pub fn notes(&self, filter: &NoteFilter) -> Result<Vec<Note>> {
        let conn = self.conn.lock().unwrap();
        query_notes(&conn, filter)
    }
}

/// 主查询 + 标签批量补齐。
fn query_notes(conn: &Connection, filter: &NoteFilter) -> Result<Vec<Note>> {
    let mut sql = String::from(
        "SELECT n.id, n.title, n.content, c.name, n.created_at, n.updated_at
         FROM notes n LEFT JOIN categories c ON c.id = n.category_id",
    );
    let mut clauses: Vec<&str> = Vec::new();
    let mut values: Vec<String> = Vec::new();

    if !filter.query.is_empty() {
        match filter.scope {
            SearchScope::Title => {
                clauses.push("n.title LIKE ? ESCAPE '\\'");
                values.push(like_pattern(&filter.query));
            }
            SearchScope::FullText => {
                clauses.push("(n.title LIKE ? ESCAPE '\\' OR n.content LIKE ? ESCAPE '\\')");
                let pattern = like_pattern(&filter.query);
                values.push(pattern.clone());
                values.push(pattern);
            }
        }
    }
    if let Some(category) = &filter.category {
        clauses.push("n.category_id = (SELECT id FROM categories WHERE name = ?)");
        values.push(category.clone());
    }
    for tag in &filter.tags {
        clauses.push(
            "EXISTS (SELECT 1 FROM note_tags nt JOIN tags t ON t.id = nt.tag_id \
             WHERE nt.note_id = n.id AND t.name = ?)",
        );
        values.push(tag.clone());
    }
    if !clauses.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&clauses.join(" AND "));
    }
    sql.push_str(" ORDER BY n.updated_at DESC, n.id DESC");

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(values.iter()), |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
        ))
    })?;
    let mut notes = Vec::new();
    for row in rows {
        let (id, title, content, category, created, updated) = row?;
        notes.push(Note {
            id,
            title,
            content,
            category,
            tags: Vec::new(),
            created_at: parse_timestamp(&created)?,
            updated_at: parse_timestamp(&updated)?,
        });
    }

    if !notes.is_empty() {
        let ids: Vec<i64> = notes.iter().map(|note| note.id).collect();
        let placeholders = vec!["?"; ids.len()].join(",");
        let tag_sql = format!(
            "SELECT nt.note_id, t.name FROM note_tags nt
             JOIN tags t ON t.id = nt.tag_id
             WHERE nt.note_id IN ({placeholders}) ORDER BY t.name"
        );
        let mut tag_stmt = conn.prepare(&tag_sql)?;
        let tag_rows = tag_stmt.query_map(params_from_iter(ids.iter()), |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut tags_by_note: HashMap<i64, Vec<String>> = HashMap::new();
        for row in tag_rows {
            let (note_id, name) = row?;
            tags_by_note.entry(note_id).or_default().push(name);
        }
        for note in &mut notes {
            note.tags = tags_by_note.remove(&note.id).unwrap_or_default();
        }
    }
    Ok(notes)
}

/// LIKE 模式：包裹 `%...%` 并转义 `\` `%` `_`（配合 ESCAPE '\'，字面匹配特殊字符）。
fn like_pattern(query: &str) -> String {
    let mut pattern = String::with_capacity(query.len() + 8);
    for ch in query.chars() {
        if matches!(ch, '\\' | '%' | '_') {
            pattern.push('\\');
        }
        pattern.push(ch);
    }
    format!("%{pattern}%")
}

/// 解析库中 RFC3339 本地时间。
fn parse_timestamp(s: &str) -> Result<DateTime<Local>> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Local))
        .map_err(|_| DbError::CorruptTimestamp(s.to_string()))
}
