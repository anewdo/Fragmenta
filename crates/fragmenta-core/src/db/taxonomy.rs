//! 分类与标签管理
//! （`#RRGGBB` 或 NULL = 未选色）。

use std::collections::HashMap;

use rusqlite::{Connection, params};

use super::{Db, DbError, Result};
use crate::model::{TaxonomyEntry, normalize_tags};

impl Db {
    /// 全部分类名（按名称排序）。
    pub fn categories(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        list_names(&conn, "SELECT name FROM categories ORDER BY name")
    }

    /// 全部标签名（按名称排序；标签为显式实体，无引用亦留存）。
    pub fn tags(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        list_names(&conn, "SELECT name FROM tags ORDER BY name")
    }

    /// 全部分类条目（名称排序）：颜色 + 引用该分类的笔记数。
    pub fn categories_with_usage(&self) -> Result<Vec<TaxonomyEntry>> {
        let conn = self.conn.lock().unwrap();
        list_entries(
            &conn,
            "SELECT c.id, c.name, c.color, COUNT(n.id)
             FROM categories c LEFT JOIN notes n ON n.category_id = c.id
             GROUP BY c.id ORDER BY c.name",
        )
    }

    /// 全部标签条目（名称排序）：颜色 + 挂载该标签的笔记数。
    pub fn tags_with_usage(&self) -> Result<Vec<TaxonomyEntry>> {
        let conn = self.conn.lock().unwrap();
        list_entries(
            &conn,
            "SELECT t.id, t.name, t.color, COUNT(nt.note_id)
             FROM tags t LEFT JOIN note_tags nt ON nt.tag_id = t.id
             GROUP BY t.id ORDER BY t.name",
        )
    }

    /// 分类名 → 颜色映射（供卡片徽片与表单按名取色）。
    pub fn category_colors(&self) -> Result<HashMap<String, Option<String>>> {
        let conn = self.conn.lock().unwrap();
        list_colors(&conn, "SELECT name, color FROM categories")
    }

    /// 标签名 → 颜色映射（供卡片徽片与表单按名取色）。
    pub fn tag_colors(&self) -> Result<HashMap<String, Option<String>>> {
        let conn = self.conn.lock().unwrap();
        list_colors(&conn, "SELECT name, color FROM tags")
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

    /// 新建分类（管理页入口）：名称先经 trim，空白报错；重名报错；默认无色。
    pub fn create_category(&self, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(DbError::BlankName);
        }
        let conn = self.conn.lock().unwrap();
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM categories WHERE name = ?1)",
            params![name],
            |row| row.get(0),
        )?;
        if exists {
            return Err(DbError::CategoryExists(name.to_string()));
        }
        conn.execute("INSERT INTO categories (name) VALUES (?1)", params![name])?;
        Ok(())
    }

    /// 新建标签（管理页入口）：语义同 [`Db::create_category`]。
    pub fn create_tag(&self, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(DbError::BlankName);
        }
        let conn = self.conn.lock().unwrap();
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM tags WHERE name = ?1)",
            params![name],
            |row| row.get(0),
        )?;
        if exists {
            return Err(DbError::TagExists(name.to_string()));
        }
        conn.execute("INSERT INTO tags (name) VALUES (?1)", params![name])?;
        Ok(())
    }

    /// 确保标签存在：不存在时按给定颜色建立；已存在时保留原名称与原颜色。
    /// 返回是否新建（`false` 表示标签此前已在库中）。
    pub fn ensure_tag(&self, name: &str, color: Option<&str>) -> Result<bool> {
        let name = name.trim();
        if name.is_empty() {
            return Err(DbError::BlankName);
        }
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute(
            "INSERT OR IGNORE INTO tags (name, color) VALUES (?1, ?2)",
            params![name, color],
        )?;
        Ok(changed > 0)
    }

    /// 删除分类（管理页显式删除，ADR-0008）：相关笔记的归属经外键
    /// `ON DELETE SET NULL` 随本条语句置空，笔记本身完好；条目不存在报错。
    pub fn delete_category(&self, name: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute("DELETE FROM categories WHERE name = ?1", params![name])?;
        if changed == 0 {
            return Err(DbError::CategoryNotFound(name.to_string()));
        }
        Ok(())
    }

    /// 删除标签（管理页显式删除）：全部挂载行经外键 `ON DELETE CASCADE`
    /// 随本条语句摘除，笔记本身完好；条目不存在报错。
    pub fn delete_tag(&self, name: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute("DELETE FROM tags WHERE name = ?1", params![name])?;
        if changed == 0 {
            return Err(DbError::TagNotFound(name.to_string()));
        }
        Ok(())
    }

    /// 设置分类颜色：`Some("#RRGGBB")` 设色、`None` 恢复未选色（中性）；
    /// 条目不存在报错。
    pub fn set_category_color(&self, name: &str, color: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute(
            "UPDATE categories SET color = ?1 WHERE name = ?2",
            params![color, name],
        )?;
        if changed == 0 {
            return Err(DbError::CategoryNotFound(name.to_string()));
        }
        Ok(())
    }

    /// 设置标签颜色：语义同 [`Db::set_category_color`]。
    pub fn set_tag_color(&self, name: &str, color: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute(
            "UPDATE tags SET color = ?1 WHERE name = ?2",
            params![color, name],
        )?;
        if changed == 0 {
            return Err(DbError::TagNotFound(name.to_string()));
        }
        Ok(())
    }

    /// 重命名标签；笔记挂载经外键自动跟随。源不存在报错，目标重名报错。
    pub fn rename_tag(&self, from: &str, to: &str) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let target_exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM tags WHERE name = ?1)",
            params![to],
            |row| row.get(0),
        )?;
        if target_exists {
            return Err(DbError::TagExists(to.to_string()));
        }
        let changed = tx.execute(
            "UPDATE tags SET name = ?1 WHERE name = ?2",
            params![to, from],
        )?;
        if changed == 0 {
            return Err(DbError::TagNotFound(from.to_string()));
        }
        tx.commit()?;
        Ok(())
    }

    /// 全量替换笔记标签：入参先经规范化（去首尾空白 / 丢弃空项 / 去重保序）。
    /// 标签为显式实体，失去引用的标签留存于库（ADR-0008）。
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

/// 条目查询：id / 名称 / 颜色 / 引用计数。
fn list_entries(conn: &Connection, sql: &str) -> Result<Vec<TaxonomyEntry>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |row| {
        Ok(TaxonomyEntry {
            id: row.get(0)?,
            name: row.get(1)?,
            color: row.get(2)?,
            note_count: row.get(3)?,
        })
    })?;
    let mut entries = Vec::new();
    for entry in rows {
        entries.push(entry?);
    }
    Ok(entries)
}

/// 名字 → 颜色映射查询。
fn list_colors(conn: &Connection, sql: &str) -> Result<HashMap<String, Option<String>>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
    })?;
    let mut map = HashMap::new();
    for row in rows {
        let (name, color) = row?;
        map.insert(name, color);
    }
    Ok(map)
}
