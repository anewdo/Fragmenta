# 数据迁移走 SQLite backup API

迁移 data_dir 时直接拷贝数据库文件有两条坑：库处于 WAL 模式时未 checkpoint 的数据在 `-wal` 文件里，只拷 `.db` 会丢数据；拷贝期间磁贴防抖落库会得到中间态副本。故迁移一律在新路径建立全新连接、由旧连接 `backup_to` 在线备份（rusqlite `backup` feature），journal 模式无关、在线一致；禁止文件系统裸拷。

状态：accepted（2026-10-05）
