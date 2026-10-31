//! 笔记列表查询黑盒测试：排序方式与筛选条件（真实 SQLite）。

mod common;

use chrono::Local;
use common::{raw_conn, TempDir};
use fragmenta_core::db::Db;
use fragmenta_core::model::{Note, NoteFilter, NoteSort};

fn note(title: &str) -> Note {
    note_with(title, "")
}

fn note_with(title: &str, content: &str) -> Note {
    Note {
        id: 0,
        title: title.to_string(),
        content: content.to_string(),
        tags: Vec::new(),
        created_at: Local::now(),
        updated_at: Local::now(),
    }
}

fn titles(notes: Vec<Note>) -> Vec<String> {
    notes.into_iter().map(|note| note.title).collect()
}

fn sorted_titles(notes: Vec<Note>) -> Vec<String> {
    let mut titles = titles(notes);
    titles.sort();
    titles
}

/// 三种排序各自按对应字段生效；名称排序忽略大小写。
#[test]
fn notes_follow_selected_sort() {
    let dir = TempDir::new("db-notes-sort");
    let db_path = dir.path().join("notes.db");
    let db = Db::open(&db_path).unwrap();

    let mut zulu = note("Zulu");
    let mut beta = note("beta");
    let mut gamma = note("Gamma");
    for note in [&mut zulu, &mut beta, &mut gamma] {
        db.upsert_note(note).unwrap();
    }

    // 固定时间戳：创建时间与更新时间顺序相反，便于区分两种时间排序
    let conn = raw_conn(&db_path);
    for (note, created, updated) in [
        (
            &zulu,
            "2026-10-01T09:00:00+08:00",
            "2026-10-03T09:00:00+08:00",
        ),
        (
            &beta,
            "2026-10-02T09:00:00+08:00",
            "2026-10-02T09:00:00+08:00",
        ),
        (
            &gamma,
            "2026-10-03T09:00:00+08:00",
            "2026-10-01T09:00:00+08:00",
        ),
    ] {
        conn.execute(
            "UPDATE notes SET created_at = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![created, updated, note.id],
        )
        .unwrap();
    }

    let filter = |sort| NoteFilter {
        sort,
        ..NoteFilter::default()
    };
    assert_eq!(
        titles(db.notes(&filter(NoteSort::UpdatedAt)).unwrap()),
        ["Zulu", "beta", "Gamma"]
    );
    assert_eq!(
        titles(db.notes(&filter(NoteSort::CreatedAt)).unwrap()),
        ["Gamma", "beta", "Zulu"]
    );
    assert_eq!(
        titles(db.notes(&filter(NoteSort::Title)).unwrap()),
        ["beta", "Gamma", "Zulu"]
    );
}

/// 关键词同时匹配标题与正文；标签筛选要求全部所选标签同时挂载。
#[test]
fn fulltext_search_and_tag_filter() {
    let dir = TempDir::new("db-notes-filter");
    let db = Db::open(&dir.path().join("notes.db")).unwrap();

    let mut work = note_with("Work plan", "design review");
    let mut reading = note_with("Reading notes", "Work retrospective");
    let mut misc = note_with("Misc", "unrelated");
    for note in [&mut work, &mut reading, &mut misc] {
        db.upsert_note(note).unwrap();
    }
    db.set_note_tags(work.id, &[String::from("工作")])
        .unwrap();
    db.set_note_tags(reading.id, &[String::from("读书")])
        .unwrap();

    let filter = |query: &str| NoteFilter {
        query: query.to_string(),
        ..NoteFilter::default()
    };

    // 标题命中（Work plan）与正文命中（Reading notes）都返回
    assert_eq!(
        sorted_titles(db.notes(&filter("Work")).unwrap()),
        ["Reading notes", "Work plan"]
    );
    // 标签筛选：仅挂载「工作」的笔记命中
    let tagged = NoteFilter {
        tags: vec![String::from("工作")],
        ..NoteFilter::default()
    };
    assert_eq!(sorted_titles(db.notes(&tagged).unwrap()), ["Work plan"]);
}
