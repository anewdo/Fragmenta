//! 数据目录迁移编排（ADR-0002 / ADR-0006，ticket 06）：
//! 目标目录校验、imgs 随迁拷贝与数据产物清理。
//!
//! 迁移顺序由 [`SettingsState::set_data_dir`](super::state::SettingsState::set_data_dir)
//! 编排：flush_all → 校验 → 在线备份试开 → imgs 随迁 → settings.save
//! （失败清理目标产物并中止）→ 连接热替换 → 广播 → 删除旧数据文件。

use std::io;
use std::path::Path;

/// 数据目录迁移错误：红字展示于设置页，返回时数据不动。
#[derive(Debug, thiserror::Error)]
pub enum DataDirError {
    /// 目标目录与当前数据目录相同或互为父子嵌套。
    #[error("目标目录与当前数据目录相同或相互嵌套，请另选位置")]
    SameOrNested,
    /// 目标目录已存在且非空。
    #[error("目标目录非空，迁移要求空目录或不存在（将自动新建）")]
    NotEmpty,
    /// 目标位置不是文件夹。
    #[error("目标位置不是文件夹")]
    NotDirectory,
    /// 目标目录不可写。
    #[error("目标目录不可写：{0}")]
    NotWritable(io::Error),
    /// 迁移前 pending 草稿落库失败（内容保留，可重试）。
    #[error("有未保存内容落库失败，请稍后重试")]
    FlushFailed,
    /// 在线备份或试开失败。
    #[error("数据库在线备份失败：{0}")]
    Backup(#[from] fragmenta_core::db::DbError),
    /// imgs 图库随迁拷贝失败。
    #[error("图库随迁失败：{0}")]
    Copy(#[source] io::Error),
    /// settings.json 持久化失败（目标产物已清理，内存态未变）。
    #[error("保存设置失败：{0}")]
    Save(#[from] fragmenta_core::settings::SettingsError),
}

/// 校验迁移目标目录（ADR-0006 配套）：不存在则新建；已存在时必须为空；
/// canonicalize 后与当前数据目录相同或互为父子嵌套一律拒绝；写入探针
/// 验证可写。
///
/// 校验失败时若目标目录系本次新建，顺带移除（仅空目录可删，不留垃圾）。
pub(super) fn validate_target(old_dir: &Path, new_dir: &Path) -> Result<(), DataDirError> {
    let created = !new_dir.exists();
    let check = || -> Result<(), DataDirError> {
        std::fs::create_dir_all(new_dir).map_err(DataDirError::NotWritable)?;
        if !new_dir.is_dir() {
            return Err(DataDirError::NotDirectory);
        }
        // canonicalize 归一（大小写 / .. / 短路径名）后再判同目录与嵌套：
        // 同目录会让备份源目标同文件；嵌套则"删除旧数据文件"收尾会毁掉新库
        let old_canon = old_dir.canonicalize().map_err(DataDirError::NotWritable)?;
        let new_canon = new_dir.canonicalize().map_err(DataDirError::NotWritable)?;
        if new_canon == old_canon
            || new_canon.starts_with(&old_canon)
            || old_canon.starts_with(&new_canon)
        {
            return Err(DataDirError::SameOrNested);
        }
        // 目标必须为空目录（本次新建的自然为空）
        if std::fs::read_dir(&new_canon)
            .map_err(DataDirError::NotWritable)?
            .next()
            .is_some()
        {
            return Err(DataDirError::NotEmpty);
        }
        // 可写探针：建删一个临时文件
        let probe = new_canon.join(".fragmenta-write-probe");
        std::fs::write(&probe, b"").map_err(DataDirError::NotWritable)?;
        if let Err(err) = std::fs::remove_file(&probe) {
            let _ = std::fs::remove_file(&probe);
            return Err(DataDirError::NotWritable(err));
        }
        Ok(())
    };
    let result = check();
    if result.is_err() && created {
        // 本次新建的空目录，失败时移除；预存的旧目录不动
        let _ = std::fs::remove_dir(new_dir);
    }
    result
}

/// imgs 图库随迁拷贝（源目录不存在视为空图库，目标建空目录）。
pub(super) fn copy_imgs(old_dir: &Path, new_dir: &Path) -> Result<(), DataDirError> {
    let src = old_dir.join("imgs");
    let dst = new_dir.join("imgs");
    if !src.exists() {
        std::fs::create_dir_all(&dst).map_err(DataDirError::Copy)?;
        return Ok(());
    }
    copy_dir_recursive(&src, &dst).map_err(DataDirError::Copy)
}

/// 递归拷贝目录（图库为哈希文件平铺，此处仅按通用目录处理）。
fn copy_dir_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let to = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

/// 删除目录内的数据产物（`fragmenta.db` 三件套 + `imgs/`），目录本身保留。
///
/// 迁移失败回滚（清目标产物）与迁移成功收尾（删旧数据文件）共用；
/// 单个文件删除失败即返回，由调用方决定提示语义。
pub(super) fn remove_data_products(dir: &Path) -> io::Result<()> {
    for name in ["fragmenta.db", "fragmenta.db-wal", "fragmenta.db-shm"] {
        let path = dir.join(name);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
    }
    let imgs = dir.join("imgs");
    if imgs.exists() {
        std::fs::remove_dir_all(&imgs)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    //! 迁移编排 headless 测试（ticket 06 / spec Testing Decisions）：真实
    //! SQLite + 真实临时目录（仓库 `temp/tests/`），经
    //! `SettingsState::set_data_dir` 公共 API 驱动，断言磁盘产物与库中数据，
    //! 无 mock 无桩。需访问 `pub(crate)` 的 NotesStore / ImageStore 全局，
    //! 故按项目"模块内部 headless 测试"模式置于模块内（先例 quit.rs）。

    use std::cell::RefCell;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use fragmenta_core::db::Db;
    use fragmenta_core::media::Images;
    use fragmenta_core::model::{Note, NoteFilter, Todo};
    use fragmenta_core::settings::Settings;
    use gpui_kit::{AppContext as _, TestAppContext};

    use crate::editor::ImageStore;
    use crate::notes::{NoteDraft, NoteKey, NotesState, NotesStore};
    use crate::settings::{SettingsEvent, SettingsState, SettingsStore};

    use super::DataDirError;

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    /// 测试临时目录（同 tests/common：仓库 temp/tests/ 下唯一命名，析构删除）。
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../temp/tests")
                .join(format!("{name}-{}-{n}", std::process::id()));
            std::fs::create_dir_all(&path).expect("创建测试临时目录失败");
            TempDir(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// 迁移测试装配：真实旧库（一条笔记 + 一条待办 + 一张图片）、notes
    /// store（含 pending 草稿通道）、图库与设置 store。
    struct Fixture {
        old_dir: PathBuf,
        settings_path: PathBuf,
    }

    /// 装配（root 为测试自有临时目录；settings.json 路径可指定，
    /// save 失败回滚用例指向不存在的父目录）。
    fn setup(root: &Path, settings_path: PathBuf, cx: &mut TestAppContext) -> Fixture {
        cx.update(gpui_kit::component::theme::init);
        let old_dir = root.join("data");
        std::fs::create_dir_all(old_dir.join("imgs")).unwrap();
        let db = Arc::new(Db::open(&old_dir.join("fragmenta.db")).unwrap());
        let now = chrono::Local::now();
        let mut note = Note {
            id: 0,
            title: "已落库笔记".into(),
            content: "迁移前内容".into(),
            tags: vec![],
            created_at: now,
            updated_at: now,
        };
        db.upsert_note(&mut note).unwrap();
        let mut todo = Todo {
            id: 0,
            content: "迁移前待办".into(),
            date: chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap(),
            interval: None,
            done: false,
            created_at: now,
            updated_at: now,
        };
        db.upsert_todo(&mut todo).unwrap();
        std::fs::write(old_dir.join("imgs").join("pic.png"), b"fake-png-bytes").unwrap();
        let settings = Settings {
            data_dir: old_dir.clone(),
            ..Settings::default()
        };
        let settings_state =
            cx.new(|cx| SettingsState::new(settings, settings_path.clone(), db.clone(), cx));
        let notes = cx.new(|_| NotesState::new(db));
        cx.update(|cx| {
            cx.set_global(SettingsStore(settings_state));
            cx.set_global(NotesStore(notes));
            cx.set_global(ImageStore(Images::new(old_dir.join("imgs"))));
        });
        Fixture {
            old_dir,
            settings_path,
        }
    }

    /// 订阅设置广播并收集为序列。
    fn track_events(cx: &TestAppContext) -> Rc<RefCell<Vec<SettingsEvent>>> {
        let events = Rc::new(RefCell::new(Vec::new()));
        let sink = events.clone();
        cx.update(|cx| {
            let state = cx.global::<SettingsStore>().0.clone();
            cx.subscribe(&state, move |_, event: &SettingsEvent, _| {
                sink.borrow_mut().push(*event);
            })
            .detach();
        });
        events
    }

    /// 经公共 API 触发迁移。
    fn migrate(new_dir: PathBuf, cx: &mut TestAppContext) -> Result<(), DataDirError> {
        cx.update(|cx| cx.global::<SettingsStore>().0.clone())
            .update(cx, |state, cx| state.set_data_dir(new_dir, cx))
    }

    /// 库中全部笔记正文。
    fn note_contents(db_path: &Path) -> Vec<String> {
        Db::open(db_path)
            .unwrap()
            .notes(&NoteFilter::default())
            .unwrap()
            .into_iter()
            .map(|note| note.content)
            .collect()
    }

    /// 迁移成功：pending 草稿完整落库、新库数据一致、imgs 随迁、旧数据文件
    /// 删除且旧文件夹保留、图库换根、后续写入落新库、Changed 广播。
    #[gpui_kit::test]
    fn migrate_moves_data_and_keeps_running(cx: &mut TestAppContext) {
        let root = TempDir::new("data-dir-migrate");
        let fx = setup(root.path(), root.path().join("settings.json"), cx);
        let new_dir = root.path().join("new-data");
        let events = track_events(cx);

        // 磁贴 pending 草稿：迁移时才经 flush_all 落库（Draft 键 = 新建路径）
        let notes = cx.update(|cx| cx.global::<NotesStore>().0.clone());
        notes.update(cx, |state, cx| {
            let draft_id = cx.new(|_| ()).entity_id();
            state.save(
                NoteDraft {
                    key: NoteKey::Draft(draft_id),
                    title: "磁贴".into(),
                    content: "pending 草稿".into(),
                },
                cx,
            )
        });

        migrate(new_dir.clone(), cx).unwrap();

        // settings.json 指向新目录，内存态同步
        assert_eq!(Settings::load(&fx.settings_path).unwrap().data_dir, new_dir);
        // 新库数据一致：种子笔记 + 迁移前 flush 的 pending 草稿 + 待办
        let contents = note_contents(&new_dir.join("fragmenta.db"));
        assert!(contents.contains(&"迁移前内容".to_string()));
        assert!(contents.contains(&"pending 草稿".to_string()));
        let todos = Db::open(&new_dir.join("fragmenta.db"))
            .unwrap()
            .todos_between(
                chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap(),
                chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap(),
            )
            .unwrap();
        assert_eq!(todos.len(), 1);
        // imgs 随迁
        assert!(new_dir.join("imgs").join("pic.png").exists());
        // 旧目录数据文件删除，旧文件夹保留
        assert!(fx.old_dir.exists());
        assert!(!fx.old_dir.join("fragmenta.db").exists());
        assert!(!fx.old_dir.join("imgs").exists());
        // 图库换根：解析根指向新目录
        let resolved = cx
            .update(|cx| cx.global::<ImageStore>().0.resolve("imgs/pic.png"))
            .unwrap();
        assert_eq!(resolved, new_dir.join("imgs").join("pic.png"));
        // Changed 广播恰好一次
        assert_eq!(*events.borrow(), vec![SettingsEvent::Changed]);
        // 后续写入（已开 store 的共享 Arc<Db>）落新库
        notes.update(cx, |state, cx| {
            let draft_id = cx.new(|_| ()).entity_id();
            state.save(
                NoteDraft {
                    key: NoteKey::Draft(draft_id),
                    title: "迁移后".into(),
                    content: "迁移后草稿".into(),
                },
                cx,
            )
        });
        notes.update(cx, |state, cx| state.flush_all(cx));
        assert!(note_contents(&new_dir.join("fragmenta.db")).contains(&"迁移后草稿".to_string()));
    }

    /// 目标目录非空：红字拒绝（NotEmpty），数据不动、不落盘、不广播。
    #[gpui_kit::test]
    fn rejects_non_empty_target(cx: &mut TestAppContext) {
        let root = TempDir::new("data-dir-nonempty");
        let fx = setup(root.path(), root.path().join("settings.json"), cx);
        let new_dir = root.path().join("occupied");
        std::fs::create_dir_all(&new_dir).unwrap();
        std::fs::write(new_dir.join("无关文件.txt"), b"x").unwrap();
        let events = track_events(cx);

        let err = migrate(new_dir.clone(), cx).unwrap_err();

        assert!(matches!(err, DataDirError::NotEmpty));
        assert!(fx.old_dir.join("fragmenta.db").exists());
        assert!(fx.old_dir.join("imgs").exists());
        assert!(!fx.settings_path.exists());
        assert!(events.borrow().is_empty());
        // 拒绝不动目标目录中的既有文件
        assert!(new_dir.join("无关文件.txt").exists());
    }

    /// 新旧目录相同或互为父子嵌套（含大小写变体与父目录）：红字拒绝，
    /// 本次新建的嵌套空目录移除，数据不动。
    #[gpui_kit::test]
    fn rejects_same_or_nested_target(cx: &mut TestAppContext) {
        let root = TempDir::new("data-dir-nested");
        let fx = setup(root.path(), root.path().join("settings.json"), cx);
        // 新目录在旧目录内（不存在 → 校验创建后随拒绝移除）
        let nested = fx.old_dir.join("sub");
        assert!(matches!(
            migrate(nested.clone(), cx).unwrap_err(),
            DataDirError::SameOrNested
        ));
        assert!(!nested.exists());
        // 大小写变体的同目录
        let mut same = fx.old_dir.clone();
        if let Some(last) = same
            .file_name()
            .map(|name| name.to_string_lossy().to_uppercase())
        {
            same.set_file_name(last);
        }
        assert!(matches!(
            migrate(same, cx).unwrap_err(),
            DataDirError::SameOrNested
        ));
        // 旧目录的父目录（旧在新内）：拒绝优先于非空检查
        assert!(matches!(
            migrate(root.path().to_path_buf(), cx).unwrap_err(),
            DataDirError::SameOrNested
        ));
        assert!(fx.old_dir.join("fragmenta.db").exists());
        assert!(fx.old_dir.join("imgs").exists());
        assert!(!fx.settings_path.exists());
    }

    /// settings.save 失败：目标产物清理、内存态不变、旧数据完整、无广播。
    #[gpui_kit::test]
    fn save_failure_cleans_target_and_keeps_old_data(cx: &mut TestAppContext) {
        let root = TempDir::new("data-dir-rollback");
        // settings.json 父目录不存在 → save 真实失败（NotFound）
        let settings_path = root.path().join("missing-parent").join("settings.json");
        let fx = setup(root.path(), settings_path, cx);
        let new_dir = root.path().join("new-data");
        let events = track_events(cx);

        let err = migrate(new_dir.clone(), cx).unwrap_err();

        assert!(matches!(err, DataDirError::Save(_)));
        // 内存态不变：数据目录仍是旧目录
        cx.update(|cx| {
            let state = cx.global::<SettingsStore>().0.clone();
            state.read_with(cx, |state, _| state.data_dir() == fx.old_dir.as_path())
        });
        // 旧数据完整
        assert!(fx.old_dir.join("fragmenta.db").exists());
        assert!(fx.old_dir.join("imgs").exists());
        assert!(
            note_contents(&fx.old_dir.join("fragmenta.db")).contains(&"迁移前内容".to_string())
        );
        // 目标产物已清理（空目录可留）
        assert!(!new_dir.join("fragmenta.db").exists());
        assert!(!new_dir.join("imgs").exists());
        assert!(events.borrow().is_empty());
    }
}
