# 数据迁移走 SQLite backup API

迁移 data_dir 时直接拷贝数据库文件有两条坑：库处于 WAL 模式时未 checkpoint 的数据在 `-wal` 文件里，只拷 `.db` 会丢数据；拷贝期间磁贴防抖落库会得到中间态副本。故迁移一律在新路径建立全新连接、由旧连接 `backup_to` 在线备份（rusqlite `backup` feature），journal 模式无关、在线一致；禁止文件系统裸拷。

补注（2026-10-23）：迁移的目标连接替换发生在 `Db` 门面内部——锁住 `Mutex<Connection>`，在同一临界区内 `backup_to` 到新路径连接并替换内部 `Connection` 后解锁；全应用共享单个 `Arc<Db>`，store 零感知，已开磁贴后续写入自动落新库。"checkpoint(FULL/TRUNCATE) + close + 文件搬运（rename 优先）"方案经评估否决：close 后重开扩大 downtime 与崩溃窗口，同盘 rename 的收益在 MB 级库体积下不存在，且重新引入 checkpoint 纪律与 `-wal`/`-shm` 残留清理负担。

状态：accepted（2026-10-05）
