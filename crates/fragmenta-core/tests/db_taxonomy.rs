//! 标签持久层黑盒集成测试。
//!
//! 经公共 API（`Db::open` + 读写接口，真实 SQLite）验证条目查询（颜色 /
//! 引用计数 / 名称排序）、标签实体化（失去引用不清理，ADR-0008）以及
//! 建 / 改 / 删与错误路径。

mod common;

use common::{TempDir, raw_conn};
use fragmenta_core::db::{Db, DbError};
use fragmenta_core::model::{Note, NoteFilter};

fn upsert(db: &Db, title: &str) -> i64 {
    let now = chrono::Local::now();
    let mut note = Note {
        id: 0,
        title: title.into(),
        content: String::new(),
        tags: Vec::new(),
        created_at: now,
        updated_at: now,
    };
    db.upsert_note(&mut note).unwrap();
    note.id
}

/// 条目查询：名称字典序、引用计数正确、颜色列可读回。
#[test]
fn overview_returns_counts_colors_and_sort_order() {
    let dir = TempDir::new("db-taxonomy-overview");
    let db_path = dir.path().join("overview.db");
    let db = Db::open(&db_path).unwrap();

    let a = upsert(&db, "A");
    let b = upsert(&db, "B");
    db.set_note_tags(a, &[String::from("急"), String::from("记录")])
        .unwrap();
    db.set_note_tags(b, &[String::from("急")]).unwrap();

    let tags = db.tags_with_usage().unwrap();
    let tag_names: Vec<&str> = tags.iter().map(|t| t.name.as_str()).collect();
    // 急 (U+6025) < 记 (U+8BB0)
    assert_eq!(tag_names, ["急", "记录"]);
    let counts: Vec<i64> = tags.iter().map(|t| t.note_count).collect();
    assert_eq!(counts, [2, 1]);

    // 直写颜色后读回（选色写接口属后续 ticket，此处验证读路径）
    raw_conn(&db_path)
        .execute("UPDATE tags SET color = '#2196F3' WHERE name = '急'", [])
        .unwrap();
    let tags = db.tags_with_usage().unwrap();
    let urgent = tags.iter().find(|t| t.name == "急").unwrap();
    assert_eq!(urgent.color.as_deref(), Some("#2196F3"));

    let tag_colors = db.tag_colors().unwrap();
    assert_eq!(
        tag_colors.get("急").cloned().flatten(),
        Some(String::from("#2196F3"))
    );
    assert_eq!(tag_colors.get("记录"), Some(&None));
}

/// 标签实体化（ADR-0008）：摘光挂载与删笔记后，标签仍留存于库。
#[test]
fn tags_survive_detachment_and_note_deletion() {
    let dir = TempDir::new("db-taxonomy-survival");
    let db_path = dir.path().join("survival.db");
    let db = Db::open(&db_path).unwrap();

    let a = upsert(&db, "A");
    let b = upsert(&db, "B");
    db.set_note_tags(a, &[String::from("急")]).unwrap();
    db.set_note_tags(b, &[String::from("急")]).unwrap();

    // 摘光 A 的挂载：标签留存，计数降为 1
    db.set_note_tags(a, &[]).unwrap();
    let tags = db.tags_with_usage().unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].note_count, 1);

    // 删除最后引用笔记 B：标签仍留存，计数降为 0
    db.delete_note(b).unwrap();
    let tags = db.tags_with_usage().unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].name, "急");
    assert_eq!(tags[0].note_count, 0);
    assert_eq!(db.tags().unwrap(), vec![String::from("急")]);
}

/// 新建标签：名称先 trim，默认无色零引用；重名、空白名报错且不产生实体。
#[test]
fn create_reports_duplicates_and_blank_names() {
    let dir = TempDir::new("db-taxonomy-create");
    let db = Db::open(&dir.path().join("create.db")).unwrap();

    // 成功：首尾空白被 trim，默认无色、零引用
    db.create_tag(" 急 ").unwrap();
    let tags = db.tags_with_usage().unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].name, "急");
    assert_eq!(tags[0].color, None);
    assert_eq!(tags[0].note_count, 0);

    // 重名报错，且不产生新实体
    assert!(matches!(
        db.create_tag("急").unwrap_err(),
        DbError::TagExists(_)
    ));

    // 纯空白报错，且不产生实体
    assert!(matches!(
        db.create_tag("   ").unwrap_err(),
        DbError::BlankName
    ));
    assert_eq!(db.tags_with_usage().unwrap().len(), 1);
}

/// 确保标签存在：新建时按给定颜色落库；已存在时保留原颜色与挂载。
#[test]
fn ensure_tag_creates_with_color_and_keeps_existing() {
    let dir = TempDir::new("db-taxonomy-ensure-tag");
    let db_path = dir.path().join("ensure-tag.db");
    let db = Db::open(&db_path).unwrap();

    assert!(db.ensure_tag(" 急 ", Some("#F44336")).unwrap());
    let tags = db.tags_with_usage().unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].name, "急");
    assert_eq!(tags[0].color.as_deref(), Some("#F44336"));

    // 已存在：不新建、不改颜色
    assert!(!db.ensure_tag("急", Some("#2196F3")).unwrap());
    let tags = db.tags_with_usage().unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].color.as_deref(), Some("#F44336"));

    // 空白名报错且不产生实体
    assert!(matches!(
        db.ensure_tag("   ", None).unwrap_err(),
        DbError::BlankName
    ));
    assert_eq!(db.tags_with_usage().unwrap().len(), 1);
}

/// 重命名标签：挂载与颜色随实体保留；源不存在 / 目标重名报错。
#[test]
fn rename_tag_moves_mounts_and_keeps_color() {
    let dir = TempDir::new("db-taxonomy-rename-tag");
    let db_path = dir.path().join("rename-tag.db");
    let db = Db::open(&db_path).unwrap();

    let a = upsert(&db, "A");
    let b = upsert(&db, "B");
    db.set_note_tags(a, &[String::from("急"), String::from("记录")])
        .unwrap();
    db.set_note_tags(b, &[String::from("急")]).unwrap();
    raw_conn(&db_path)
        .execute("UPDATE tags SET color = '#2196F3' WHERE name = '急'", [])
        .unwrap();

    db.rename_tag("急", "紧急").unwrap();

    // 挂载自动跟随：两篇笔记的标签名同步更新，记录不受影响
    let notes = db.notes(&NoteFilter::default()).unwrap();
    let note_a = notes.iter().find(|note| note.id == a).unwrap();
    let note_b = notes.iter().find(|note| note.id == b).unwrap();
    assert_eq!(note_a.tags, ["紧急", "记录"]);
    assert_eq!(note_b.tags, ["紧急"]);

    // 颜色与引用数随实体原样保留
    let tags = db.tags_with_usage().unwrap();
    let urgent = tags.iter().find(|tag| tag.name == "紧急").unwrap();
    assert_eq!(urgent.color.as_deref(), Some("#2196F3"));
    assert_eq!(urgent.note_count, 2);
    assert!(!tags.iter().any(|tag| tag.name == "急"));

    // 源不存在 / 目标重名报错
    assert!(matches!(
        db.rename_tag("不存在", "任意").unwrap_err(),
        DbError::TagNotFound(_)
    ));
    assert!(matches!(
        db.rename_tag("记录", "紧急").unwrap_err(),
        DbError::TagExists(_)
    ));
}

/// 删除标签：全部挂载被摘除、未涉及标签与笔记本身完好；条目不存在报错。
#[test]
fn delete_tag_detaches_all_mounts() {
    let dir = TempDir::new("db-taxonomy-delete-tag");
    let db = Db::open(&dir.path().join("delete-tag.db")).unwrap();

    let a = upsert(&db, "A");
    let b = upsert(&db, "B");
    db.set_note_tags(a, &[String::from("急"), String::from("记录")])
        .unwrap();
    db.set_note_tags(b, &[String::from("急")]).unwrap();

    db.delete_tag("急").unwrap();

    // 全部挂载摘除，未涉及标签保留，笔记内容与数量完好
    let notes = db.notes(&NoteFilter::default()).unwrap();
    assert_eq!(notes.len(), 2);
    let note_a = notes.iter().find(|note| note.id == a).unwrap();
    let note_b = notes.iter().find(|note| note.id == b).unwrap();
    assert_eq!(note_a.tags, ["记录"]);
    assert!(note_b.tags.is_empty());

    let tags = db.tags_with_usage().unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].name, "记录");
    assert_eq!(tags[0].note_count, 1);

    // 条目不存在报错
    assert!(matches!(
        db.delete_tag("急").unwrap_err(),
        DbError::TagNotFound(_)
    ));
}

/// 设色 / 置空：颜色落库并经条目查询与名称映射读回；重开 Db 颜色不变、
/// 重命名后颜色保留；置 None 恢复未选色；条目不存在报错。
#[test]
fn set_color_and_clear_persist_across_reopen() {
    let dir = TempDir::new("db-taxonomy-set-color");
    let db_path = dir.path().join("set-color.db");
    let db = Db::open(&db_path).unwrap();

    db.create_tag("急").unwrap();

    // 设色：条目查询与名称→颜色映射一致读回
    db.set_tag_color("急", Some("#F44336")).unwrap();
    let assert_color = |db: &Db| {
        let urgent = db
            .tags_with_usage()
            .unwrap()
            .into_iter()
            .find(|t| t.name == "急")
            .unwrap();
        assert_eq!(urgent.color.as_deref(), Some("#F44336"));
        assert_eq!(
            db.tag_colors().unwrap().get("急").cloned().flatten(),
            Some(String::from("#F44336"))
        );
    };
    assert_color(&db);

    // 持久性：重开 Db 颜色不变；重命名后颜色随实体保留
    drop(db);
    let db = Db::open(&db_path).unwrap();
    assert_color(&db);
    db.rename_tag("急", "紧急").unwrap();
    let tags = db.tags_with_usage().unwrap();
    let urgent = tags.iter().find(|t| t.name == "紧急").unwrap();
    assert_eq!(urgent.color.as_deref(), Some("#F44336"));

    // 置空：恢复未选色
    db.set_tag_color("紧急", None).unwrap();
    assert_eq!(db.tag_colors().unwrap().get("紧急"), Some(&None));

    // 条目不存在报错（设色与置空同路径）
    assert!(matches!(
        db.set_tag_color("不存在", Some("#F44336")).unwrap_err(),
        DbError::TagNotFound(_)
    ));
    assert!(matches!(
        db.set_tag_color("不存在", None).unwrap_err(),
        DbError::TagNotFound(_)
    ));
}
