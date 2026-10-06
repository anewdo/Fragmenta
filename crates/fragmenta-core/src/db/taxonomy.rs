//! 分类与标签管理（架构 §5.2）。
//!
//! 生命周期约定：分类为显式实体——建立后持续存在、支持重命名、删笔记不清理；
//! 标签为值对象——挂载时按需建立、失去全部引用即从库中清理。

use rusqlite::{Connection, params};

use super::{Db, DbError, Result};
use crate::model::normalize_tags;

impl Db {
    /// 全部分类名（按名称排序）。
    pub fn categories(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        list_names(&conn, "SELECT name FROM categories ORDER BY name")
    }

    /// 全部被引用的标签名（按名称排序；无引用标签随各写接口即时清理）。
    pub fn tags(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        list_names(&conn, "SELECT name FROM tags ORDER BY name")
    }

    /// 设置笔记分类：`Some` 挂载（分类名不存在时自动建立），`None` 清除挂载。
    ///
    /// 只改挂载关系，不动 updated_at（元数据操作不影响列表排序）。
    pub fn set_note_category(&self, note_id: i64, category: Option<&str>) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        ensure_note(&tx, note_id)?;
        let category_id: Option<i64> = match category {
            Some(name) => {
                tx.execute(
                    "INSERT OR IGNORE INTO categories (name) VALUES (?1)",
                    params![name],
                )?;
                Some(tx.query_row(
                    "SELECT id FROM categories WHERE name = ?1",
                    params![name],
                    |row| row.get(0),
                )?)
            }
            None => None,
        };
        tx.execute(
            "UPDATE notes SET category_id = ?1 WHERE id = ?2",
            params![category_id, note_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// 重命名分类；笔记归属经外键自动跟随。源不存在报错，目标重名报错。
    pub fn rename_category(&self, from: &str, to: &str) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let target_exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM categories WHERE name = ?1)",
            params![to],
            |row| row.get(0),
        )?;
        if target_exists {
            return Err(DbError::CategoryExists(to.to_string()));
        }
        let changed = tx.execute(
            "UPDATE categories SET name = ?1 WHERE name = ?2",
            params![to, from],
        )?;
        if changed == 0 {
            return Err(DbError::CategoryNotFound(from.to_string()));
        }
        tx.commit()?;
        Ok(())
    }

    /// 全量替换笔记标签：入参先经规范化（去首尾空白 / 丢弃空项 / 去重保序），
    /// 替换后清理失去全部引用的标签；笔记不存在报错。
    pub fn set_note_tags(&self, note_id: i64, tags: &[String]) -> Result<()> {
        let normalized = normalize_tags(tags.iter().cloned());
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        ensure_note(&tx, note_id)?;
        tx.execute("DELETE FROM note_tags WHERE note_id = ?1", params![note_id])?;
        for name in &normalized {
            tx.execute(
                "INSERT OR IGNORE INTO tags (name) VALUES (?1)",
                params![name],
            )?;
            let tag_id: i64 = tx.query_row(
                "SELECT id FROM tags WHERE name = ?1",
                params![name],
                |row| row.get(0),
            )?;
            tx.execute(
                "INSERT OR IGNORE INTO note_tags (note_id, tag_id) VALUES (?1, ?2)",
                params![note_id, tag_id],
            )?;
        }
        prune_orphan_tags(&tx)?;
        tx.commit()?;
        Ok(())
    }
}

/// 笔记存在性校验。
fn ensure_note(conn: &Connection, note_id: i64) -> Result<()> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM notes WHERE id = ?1)",
        params![note_id],
        |row| row.get(0),
    )?;
    if exists {
        Ok(())
    } else {
        Err(DbError::NoteNotFound(note_id))
    }
}

/// 清理失去全部引用的标签（标签生命周期：无引用即消失）。
pub(super) fn prune_orphan_tags(conn: &Connection) -> Result<()> {
    conn.execute(
        "DELETE FROM tags WHERE id NOT IN (SELECT tag_id FROM note_tags)",
        [],
    )?;
    Ok(())
}

/// 名称列表查询。
fn list_names(conn: &Connection, sql: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut names = Vec::new();
    for name in rows {
        names.push(name?);
    }
    Ok(names)
}
