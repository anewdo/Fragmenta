# Fragmenta 架构设计

> 状态：定案（2026-10-05；2026-10-06 spike 结论回写；2026-10-12 设置页定案回写；2026-10-18 功能模块组织调整）
> 技术基线：Rust (edition 2024) · gpui-kit 0.7.1 · rusqlite 0.40 (bundled) · global-hotkey 0.8 · tray-icon · Windows 10/11
> 形态：单进程多窗口桌面应用，无后台服务，轻量低内存
> 术语：以根目录 `GLOSSARY.md` 为准（笔记 / 磁贴（速记窗口）/ 待办 / 分类 / 标签）

## 1. 产品概述

### 磁贴（速记窗口）

全局热键呼出，触发一次新开一个，旧窗口保留。无边框置顶小窗（PopUp）：

- 顶栏：新增磁贴 / 设置（Icon 按钮）— 保存状态（中间，兼拖拽区）— 置顶 / 关闭（Icon 按钮）
- 下方：标题输入区 + 正文输入区，正文左下角实时字数统计
- 输入防抖自动保存

### 管理主窗口（单例）

左侧可收缩导航栏（首页 / 笔记 / 待办 / 设置）：

- **首页**：内容待定（原"月历 + 选中日待办"方案随 Phase 12 废弃，占位页维持现状）
- **笔记**：两栏布局、分界线可拖动
  - 左栏：搜索筛选（标题 / 全文 / 分类 / 标签）+ 笔记卡片列表（标题、创建时间、摘要、分类、标签最多 3 个；右键菜单设置分类与标签，分类唯一、标签可多；卡片右端垃圾桶图标删除，带确认）
  - 右栏：Markdown 编辑器（Source / Preview / Split 三模式），支持图片
- **待办**：左侧月历选日（初始选中今日）+ 右侧选中日清单；标题行提供「今天」跳回与「新建」；新建 / 编辑共用 Dialog 表单（内容必填，可选「指定时间区间」，`end <= start` 拒绝、不支持跨天）；圆形 check 勾选完成（完成后内容与区间均加删除线）；hover 出编辑 / 删除按钮（删除无确认）；选中日无待办时空态提示
- **设置**（四组）：
  - 外观：主题（Light / Dark / Auto，ADR-0009）、字号三档
  - 快捷键：磁贴呼出热键（捕获式录入；冲突红字不落库；不允许清空）
  - 数据：数据目录与迁移、导出 md
  - 系统：开机自启动（默认关）、关闭主窗口时直接退出（默认关，默认托盘驻留）、显式退出按钮（spike ⑤ 定案：`QuitMode::Explicit` 全窗口关闭进程驻留，退出前置 `flush_all` 后 `cx.quit()`）

### 常驻与托盘

进程常驻（`QuitMode::Explicit`）。托盘图标常驻（`tray-icon` crate，kit / gpui 无托盘能力）：左键 = 打开主窗口，右键菜单 = 打开主窗口 / 退出。主窗口关闭的默认语义为托盘驻留——热键呼出磁贴不受影响；设置开启"关闭主窗口时直接退出"后，关主窗口即 `flush_all` 落库后完全退出。开机自启动走注册表 `HKCU\...\Run` 键（写入 `"{exe}" --silent`）；带 `--silent` 参数启动为静默启动——不打开主窗口、仅托盘驻留；手动启动不静默。

### 数据

- **笔记**：来源为磁贴与主界面两处
- **待办**：仅来源于主界面"待办"页
- SQLite 与图片图库平铺于 `data_dir`（默认 exe 旁 `data/`，内含 `fragmenta.db` 与 `imgs/`），整体随迁移；`settings.json` 固定 exe 旁，是定位 `data_dir` 的唯一线索（ADR-0004）

## 2. 总体分层

```mermaid
flowchart TB
    APP["app — 启动装配与退出协调"]
    MW["windows/main — 主窗口与页面装配"]
    TW["windows/tile — 磁贴窗口"]
    NT["notes — 状态、页面、笔记编辑器"]
    TD["todos — 状态与页面"]
    ST["settings — 状态、页面与色系"]
    ED["editor — 共享文本输入与图片解析"]
    WG["widgets / utils — 通用组件与取色盘"]
    HK["hotkey / tray — 系统交互"]
    CORE["fragmenta-core — model / db / settings / media / export"]
    APP --> MW
    APP --> HK
    APP --> NT
    APP --> TD
    APP --> ST
    MW --> NT
    MW --> TD
    MW --> ST
    TW --> NT
    TW --> ST
    TW --> ED
    NT --> ED
    NT --> WG
    NT --> ST
    TD --> ST
    ED --> CORE
    NT --> CORE
    TD --> CORE
    ST --> CORE
    HK --> MW
    HK --> TW
```

两条铁律：

1. **`fragmenta-core` 零 gpui 依赖**：由 Cargo 依赖图约束，纯数据逻辑通过构造参数注入依赖。
2. **UI 层不出现 SQL**：各功能的状态模块私有持有 `Arc<Db>`，页面调用状态行为方法；页面实现不直接访问数据库连接。

## 3. Workspace 结构

工作区保留 `fragmenta-core`（lib）和 `fragmenta`（lib + bin）。以下应用路径相对于 `crates/fragmenta/src/`：

| 路径 | 内容 |
| --- | --- |
| `main.rs`、`lib.rs` | 程序入口、模块声明 |
| `app.rs`、`app/quit.rs` | 启动装配、退出请求与失败提示 |
| `notes/mod.rs`、`notes/state.rs` | 笔记功能接口、共享状态、事件、草稿与保存状态 |
| `notes/editor.rs` | 笔记专用编辑器及三种编辑模式 |
| `notes/page/` | 页面交互、视图、分类标签表单、标题栏操作区 |
| `todos/mod.rs`、`todos/state.rs`、`todos/page/` | 待办功能接口、状态、月历页面与表单 |
| `settings/mod.rs`、`settings/state.rs`、`settings/page.rs` | 设置功能接口、状态与页面 |
| `ui/theme.rs`、`themes/*.json` | 内置主题定义、加载、预览与开发期热重载 |
| `editor/mod.rs`、`editor/source.rs`、`editor/image_path.rs` | 共享图库登记、源文本输入与图片路径插件 |
| `windows/main/mod.rs`、`windows/main/home.rs` | 主窗口单例、导航与布局、首页占位 |
| `windows/tile/mod.rs`、`windows/tile/open.rs`、`windows/tile/view.rs` | 磁贴创建、定位与交互 |
| `widgets/`、`utils/palette.rs` | 通用分栏、徽片与取色盘 |
| `hotkey.rs`、`tray.rs`、`win32.rs` | 热键、托盘与平台能力 |

`fragmenta-core/src/` 保留 `model/`、`db/`、`settings.rs`、`media.rs`、`export.rs`。运行时 `settings.json` 固定 exe 旁，`fragmenta.db` 与 `imgs/` 位于数据目录。

每个功能拥有其状态与视图，实现子模块私有，通过 `mod.rs` 导出调用方所需的接口。`NotesStore`、`TodosStore`、`SettingsStore` 与状态定义同文件；`ImageStore` 位于共享 `editor`，`PaletteStore` 位于 `utils/palette.rs`。启动装配统一创建并登记资源。

## 4. 核心设计决策

- **D1 双 crate workspace**：`fragmenta-core`（纯逻辑）+ `fragmenta`（bin）。分层由编译器强制；core 可用纯 `cargo test` 覆盖。
- **D2 core 无全局静态**：验证期的 `static DB: OnceLock<Mutex<Connection>>` 模式废弃。`Db::open(path)` 构造注入，state 层持有 `Arc<Db>`；测试用 `Db::open_in_memory()`。
- **D3 共享数据模块 = App 级 Entity**：`NotesState` / `TodosState` / `SettingsState` 启动时创建一次，主窗口与全部磁贴持有同一 handle。gpui Entity 是应用级的，磁贴保存后事件广播，主窗口列表与日历自动刷新。
- **D4 写路径防抖收在 store 内部**：防抖定时、落库、事件广播都在 `save()` 实现里；磁贴与主界面编辑器共用同一份防抖实现，按笔记身份独立计时（`HashMap<NoteKey, PendingWrite>`，见 ADR-0001）；调用方接口保持一个方法。
- **D5 读路径 read-through 无缓存**：本地 SQLite 亚毫秒查询，`Changed` 事件触发页面重拉，省去缓存一致性维护。
- **D6 两个存储位置各司其职**：`settings.json` 固定在 exe 旁，是启动时定位 `data_dir` 的唯一线索（便携锚点，ADR-0004，禁止注册表 / %APPDATA% 存线索）；`data_dir`（默认 exe 旁 `data/`）存数据库与图片图库——`fragmenta.db` 与 `imgs/` 平铺于其内、整体随迁移（ADR-0006）。
- **D7 主窗口单例 / 磁贴多开**：`windows::main::open` 幂等（已开则激活既有窗口）；`windows::tile::open_new` 每次新窗口。
- **D8 副作用集中**：设置变更的持久化、主题/字号应用、热键重注册集中在 `SettingsState::apply` 系列；主题/字号应用后经 `App::refresh_windows()` 广播至全部存活窗口（磁贴即时换肤）；热键事件路由集中在 `HotkeyController`。
- **D9 无 repository trait**：单 SQLite 实现，in-memory 连接即测试替身，不引入假设性抽象层。

## 5. 模块接口设计

### 5.1 core::model

纯数据类型与派生逻辑：`Note`、`Todo`、`NoteFilter`、`DayActivity`；`Note::summary()`（回车替换为空格，截取前 50 字符）、标签规范化（去空白、去重）。零副作用，单测友好。

### 5.2 core::db

全部 SQL 藏在一个门面后：

```rust
pub struct Db { conn: Mutex<Connection> }

impl Db {
    pub fn open(path: &Path) -> Result<Self>;      // 含 PRAGMA user_version 迁移
    pub fn open_in_memory() -> Result<Self>;       // 测试入口
    pub fn backup_to(&self, dst: &Db) -> Result<()>;   // 在线一致备份（数据迁移用）

    // 笔记
    pub fn upsert_note(&self, note: &mut Note) -> Result<()>;   // 回填 id / 时间戳
    pub fn delete_note(&self, id: i64) -> Result<()>;
    pub fn notes(&self, filter: &NoteFilter) -> Result<Vec<Note>>;  // 标题/全文 LIKE + 分类 + 标签
    // 待办
    pub fn upsert_todo(&self, todo: &mut Todo) -> Result<()>;
    pub fn delete_todo(&self, id: i64) -> Result<()>;
    pub fn todos_between(&self, from: NaiveDate, to: NaiveDate) -> Result<Vec<Todo>>;
    // 首页日历聚合（一次查询返回每日 note_count / todo_count）
    pub fn activity_by_day(&self, from: NaiveDate, to: NaiveDate) -> Result<Vec<DayActivity>>;
    // 分类与标签
    pub fn categories(&self) -> Result<Vec<String>>;
    pub fn set_note_category(&self, note_id: i64, category: Option<&str>) -> Result<()>;
    pub fn rename_category(&self, from: &str, to: &str) -> Result<()>;
    pub fn tags(&self) -> Result<Vec<String>>;
    pub fn set_note_tags(&self, note_id: i64, tags: &[String]) -> Result<()>;
}
```

### 5.3 core::settings

```rust
pub struct Settings {
    pub theme: ThemeMode,        // Light | Dark | Auto（ADR-0009）
    pub font_scale: FontScale,   // Small | Medium | Large，映射 Theme.font_size
    pub data_dir: PathBuf,       // 默认 {exe_dir}/data；内部平铺 fragmenta.db 与 imgs/
    pub tile_hotkey: String,     // 如 "ctrl+alt+n"；不允许清空
    pub autostart: bool,         // 开机自启动（注册表 Run，写入 --silent），默认 false
    pub close_to_quit: bool,     // 关闭主窗口时直接退出，默认 false（托盘驻留）
}

impl Settings {
    pub fn load(path: &Path) -> Result<Self>;   // 文件不存在返回默认值
    pub fn save(&self, path: &Path) -> Result<()>;
}
```

`Auto` 根据 Windows 的 `observe_window_appearance` 回调在应用的 Light / Dark JSON 主题之间切换。主题定义统一归 `themes/*.json`，加载与应用集中于 `ui/theme.rs`，由 `SettingsState` 按主题选择调用。JSON 在编译期嵌入 EXE，开发构建额外监听源码主题目录；设置页预览读取同一份定义（ADR-0009）。

### 5.4 core::media

```rust
pub struct Images { base: PathBuf }   // base = {data_dir}/imgs，随 data_dir 迁移（D6）

impl Images {
    pub fn new(base: PathBuf) -> Self;
    pub fn import(&self, bytes: &[u8], ext: &str) -> Result<String>;
    // 内容哈希命名去重，写入 base，返回相对引用 "imgs/{hash}.{ext}"
    pub fn resolve(&self, ref_path: &str) -> Option<PathBuf>;
    // "imgs/.." -> {data_dir}/imgs/..；绝对路径原样；其余返回 None
}
```

### 5.5 core::export

```rust
pub fn export_notes(notes: &[Note], out_dir: &Path, images: &Images) -> Result<ExportReport>;
```

每篇笔记一个 `.md`（文件名取标题，非法字符替换，重名追加 id）；整个导出批次共用一个 `assets/` 文件夹：正文中被引用的 `imgs/{hash}.{ext}` 改写为 `assets/{hash}.{ext}`，仅被引用的图片拷入 `{out_dir}/assets/`（未引用的图库图片不导出）。无导入功能。

### 5.6 功能状态（主窗口与磁贴共用）

```rust
pub enum NotesEvent {
    Changed,                                              // 增删改 -> 列表/日历重拉；磁贴与编辑器不订阅
    Saved { key: NoteKey, id: i64, status: SaveStatus },  // 落库结果 -> 持有该 key 的磁贴更新保存状态与笔记 id
}

pub enum SaveStatus { Saved, Failed }             // Failed：磁贴持续提示，下次输入触发的防抖保存即自然重试
pub enum NoteKey { Draft(EntityId), Note(i64) }  // 防抖槽身份：未落库草稿用调用方稳定标识，落库后用笔记 id

impl NotesState {
    pub fn new(db: Arc<Db>) -> Self;
    pub fn save(&mut self, draft: NoteDraft, cx: &mut Context<Self>);   // 内部 300ms 防抖，按 NoteKey 分区计时
    pub fn flush_now(&mut self, key: NoteKey, cx: &mut Context<Self>) -> Option<SaveStatus>;
    // 关闭路径：立即落库该槽 pending 内容；None = 无待写槽，Some(Failed) 阻止关窗（磁贴持续提示）
    pub fn cancel(&mut self, key: &NoteKey);
    // 丢弃该槽待写内容与计时器：空草稿关闭 + Draft→Note 晋升清理（防旧槽带过期内容重建计时落出重复笔记）
    pub fn flush_all(&mut self, cx: &mut Context<Self>) -> SaveStatus;
    // 尝试全部 pending 槽；任一失败返回 Failed，失败内容保留供重试
    pub fn delete(&mut self, id: i64, cx: &mut Context<Self>) -> db::Result<()>;
    // 删除成功后取消 Note(id) 待写槽与计时器，再广播 Changed；失败保留待写内容
    pub fn set_category(&mut self, id: i64, cat: Option<&str>, cx: &mut Context<Self>);
    pub fn set_tags(&mut self, id: i64, tags: Vec<String>, cx: &mut Context<Self>);
    pub fn notes(&self, filter: &NoteFilter) -> db::Result<Vec<Note>>;   // read-through 直查；失败可观察（列表错误占位依赖于此）
}
```

`TodosState` 同构（add / toggle / update / delete / todos_between）。页面在 `Changed` 事件回调里拉取数据，render 内不做查询。

**订阅矩阵**：主窗口各页（列表 / 日历 / 待办）订阅 `Changed` 重拉数据；磁贴与编辑器不订阅 `Changed`——磁贴只响应携带自己 key 的 `Saved`，编辑器是当前笔记内容的事实来源（直到切换笔记）。分类 / 标签管理页订阅 `NotesEvent::Changed`（笔记侧写操作会改变实体集合），笔记列表页另订阅 `TaxonomyEvent::Changed`（管理页建 / 改 / 删实体）重拉列表与筛选选项。

**id 回填闭环**：新磁贴持 `Option<i64>`，保存以 `Draft` key 提交；收到匹配自己的 `Saved { key, id, .. }` 后记录 id，此后以 `Note(id)` 为 key 保存。

**删除协调**：删除入口为左栏卡片右端垃圾桶图标（带确认）。`NotesState::delete` 在数据库删除成功后取消对应 `Note(id)` 待写槽与计时器（含先前保存失败保留的槽），再广播 `Changed`。页面仅在删除成功后清空当前编辑器；删除失败则保留确认框、编辑内容与待写槽，并显示错误通知。

**错误呈现**：落库失败 → 磁贴顶栏持续显示“保存失败”，不打断输入，下一次防抖保存即自然重试；列表查询失败 → 列表区错误占位 + 重试；热键重注册失败（组合键被占用）→ 设置项旁红字提示、热键保持旧值不落库。全部用 kit 内联 UI，无系统弹窗。

**退出保存失败**：退出协调位于 `app::quit`，通过 `request_quit` 发起退出、`is_pending` 查询提示状态，标记类型由退出模块内部持有。所有退出入口先 `flush_all`，全部成功后 `cx.quit()`。任一失败时保留窗口、编辑内容与失败的 pending 槽，在主窗口显示 kit Dialog（主窗口已关闭时重新打开）：“仍要退出”丢弃未保存内容并直接退出；“重试”再次尝试全部 pending，成功后退出，失败则保留提示；“返回”取消退出、关闭提示，用户可继续编辑或复制内容到其他地方。Esc 等同“返回”，Enter 等同“重试”；提示期间重复退出不叠加对话框，主窗口关闭请求被阻止。

`SettingsState` 是副作用集中地：

```rust
impl SettingsState {
    pub fn set_theme(&mut self, mode: ThemeMode, window: &mut Window, cx: &mut Context<Self>);
    // 持久化 + Theme::global_mut + Theme::sync_base + App::refresh_windows()
    pub fn set_font_scale(&mut self, scale: FontScale, window: &mut Window, cx: &mut Context<Self>);
    // 持久化 + Theme.font_size + Theme::sync_base + App::refresh_windows()
    pub fn set_hotkey(&mut self, hotkey: &str, cx: &mut Context<Self>);
    // 持久化 + HotkeyController::rebind；注册失败则提示且不落库
    pub fn set_data_dir(&mut self, dir: PathBuf, cx: &mut Context<Self>);
    // 数据迁移编排，见下
    pub fn set_autostart(&mut self, enabled: bool, cx: &mut Context<Self>);
    // 持久化 + 注册表 HKCU\...\Run 写入/删除（"{exe}" --silent）；便携 exe 被移动后路径失效，开启时重写
    pub fn set_close_to_quit(&mut self, enabled: bool, cx: &mut Context<Self>);
    // 持久化；仅改变主窗口关闭语义（见 §5.8），无即时副作用
}
```

**Auto 主题桥接**（spike ③ 结论）：`observe_window_appearance` 回调 → 依外观 `Theme::global_mut` 切换 light/dark → `Theme::sync_base` + `App::refresh_windows()`。kit 主题不自动跟随系统外观，桥接逻辑收敛于 `SettingsState`（D8）。Auto 与色系主题互斥（枚举单选，ADR-0005）。

`SettingsState::attach_appearance_bridge` 校准当前外观并返回 `Subscription`，由 `MainShell` / `TileView` 各自持有；窗口视图释放时取消监听，设置 store 不累计保存订阅。

**数据迁移流程**（`set_data_dir` 内编排；一致性走 SQLite backup API，禁止文件系统裸拷，见 ADR-0002；布局与旧档处置见 ADR-0004 / ADR-0006）：前置 `flush_all` → 校验新路径可写 → 在新路径建立全新连接、由旧连接 `backup_to` 在线备份 → `Db::open(新路径)` 试开 → `imgs/` 图库文件随迁拷贝 → 各 store 与 `ImageStore` 原子替换 `Arc<Db>` / 图库根 → `settings.save` → 广播 `Changed`。磁贴窗口持有 store handle，自动使用新连接；旧目录改名留存为 `.bak`，不自动删除，用户确认后自行清理。

### 5.7 hotkey

```rust
pub struct HotkeyController { /* Box::leak 保活的 GlobalHotKeyManager */ }

impl HotkeyController {
    pub fn init(hotkey: &str, cx: &mut App);   // 注册 + 事件线程转发
    pub fn rebind(new_hotkey: &str);           // 注销旧 + 注册新
}
```

事件链（已验证）：`GlobalHotKeyEvent::receiver()`（独立线程，过滤 `Pressed`）→ smol channel → `cx.spawn` 循环 → `windows::tile::open_new(cx)`。

### 5.8 windows

- `main::open(cx)`：单例幂等，已开则激活；`MainShell` 持 `Page` 枚举路由，各页 Entity 常驻（切换保留筛选与滚动状态）；侧栏用 kit `Sidebar::collapsible(true)`
- 笔记页通过 `NotesPage::header_actions` 提供标题栏操作区，`MainShell` 持有笔记页观察句柄，模式切换后刷新选中态；按钮实现与编辑模式类型归笔记功能内部。
- 主窗口关闭语义：`close_to_quit` 关（默认）→ 进程不退，托盘驻留（§5.11），再开走 `main::open` 重开；开 → 走 §5.6 统一退出流程，保存失败时保留主窗口供用户选择
- `tile::open_new(cx)`：每次新窗口；`WindowKind::PopUp` + `titlebar: None`（Windows 后端原生置顶 + 不进任务栏）；弹出行为——首开磁贴中心对准光标（空间不足时钳制完整可见）；场上已有磁贴时取创建时机最新的现存磁贴的**当前实际位置**（登记表存 `AnyWindowHandle`，经 `update` 取 `window.bounds()`，磁贴被挪动后新磁贴跟随其现位置）级联偏移 26 逻辑像素，并经 `cx.displays()` 反查其中心点所在显示器做钳制；场上磁贴全部关闭后重置为首开行为；定位须绑定 `display_id`（坐标语义见 §9 条目 5）
- `tile::TileView`：顶栏中间不含交互元素的区域为拖拽移动区；标题与正文共处同一输入区（仅一条分隔线，与弹窗边缘零外距）；窗口尺寸经八向边缘热区拖拽调整（下限 240×280 逻辑像素——gpui 无无边框窗口的边缘 resize API，热区按下记录起始快照，`canvas` 的 paint 钩子每帧注册窗口级 mouse 流，从快照全量计算目标后经 Win32 `SetWindowPos` 物理像素通道一次到位，左 / 上向边缘锚定拖拽前的对侧缘）；圆角窗口区域按实际尺寸设置，并在窗口 bounds 变化时重设（`observe_window_bounds`）；关闭语义——空草稿直接丢弃，非空草稿先 `flush_now` 同步保存再关闭（草稿可弃，已落库不删）
- 统一经 kit 门面 `gpui_kit::open_window` 开窗（自动包 `base::Root`，获得 overlay hosting——右键菜单 / Popover 依赖）

磁贴关闭按钮与原生关闭请求（如 Alt+F4）共用保存检查：空磁贴取消待写槽；非空磁贴同步保存，失败则保留窗口与输入内容。原生回调直接更新视图实体，避免经窗口 handle 重入当前窗口。

### 5.9 共享编辑能力与笔记编辑器

`editor` 提供共享输入能力，`notes/editor.rs` 组装笔记页专用编辑器：

- **共享源文本输入组件**——`TextareaState` 封装 + 图片粘贴钩子 + 字数统计回调 + 光标处插入 md 引用（见 §8）。磁贴正文区与编辑器 Source 模式共同消费，图片粘贴钩子全应用仅此一份
- **笔记编辑器组装**：模式枚举 `Source | Preview | Split`——Source = 源文本输入组件，Preview = `base::markdown(...).plugin(图片路径插件).scrollable(true)`，Split 左右双栏
- 图片粘贴机制（spike ① 验证）：kit 0.7.1 提供 `Textarea::on_paste` 官方拦截点（`capture_action(Paste)` + `stop_propagation`），handler 签名 `Fn(&ClipboardItem, &mut Window, &mut App) -> bool`，返回 `true` 消费本次粘贴；图与文本并存时图片优先；图片经 `Image::from_bytes` 取字节 → `Images::import` → `TextareaState::insert` 光标处插入引用。handler 内实体更新必须经 `Entity::update` + handler 持有的 `&mut Window`——`WeakEntity::update_in` 走 `App::with_window`，在 capture_action 派发栈内窗口已被占用，查找必然失败
- Split 双栏各自独立滚动为最终行为（见 ADR-0003）
- 编辑器只含标题与正文，不含分类 / 标签 UI（分类标签唯一查看编辑入口 = 左栏卡片右键菜单）

### 5.10 widgets

`ResizableSplit`（拖分界调宽）、`TagBadge`、`CategoryTag` 等。待办页月历直接消费 kit `base::Calendar`（选日事件驱动清单刷新），无需自研 widget。

### 5.11 tray

kit / gpui 无托盘能力，引入 `tray-icon` crate（Tauri 同款）。托盘图标常驻；事件（左键单击 / 菜单项）经 channel 回收，转发模式与热键一致（channel → `cx.spawn` 主循环消费）。共存性已由 spike ⑥ 验证通过（`examples/spike6_tray_icon.rs`，2026-10-12）：托盘在 gpui 主线程创建即可，其内部消息窗口由 gpui 主循环的 GetMessage/DispatchMessage 泵分发，无需自建消息循环；`TrayIcon` 非 `Send`，存 gpui 全局；既有热键链路不受干扰，drop 即移除图标。交互：左键 = `main::open`（幂等激活）；右键菜单 = 打开主窗口 / 退出（退出前置 `flush_all` + `cx.quit()`，与显式退出按钮同路径）。静默启动由 `app.rs` 启动编排判定：启动参数含 `--silent` 时不打开主窗口、仅托盘驻留；开机自启动注册表 Run 键写入的就是带 `--silent` 的命令行。

## 6. 数据模型与 Schema

```sql
-- 迁移仅存在于 core::db::migrate，PRAGMA user_version 驱动
CREATE TABLE notes (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  title       TEXT NOT NULL DEFAULT '',
  content     TEXT NOT NULL DEFAULT '',        -- Markdown 源文
  category_id INTEGER REFERENCES categories(id) ON DELETE SET NULL,
  created_at  TEXT NOT NULL,                   -- RFC3339 本地时间
  updated_at  TEXT NOT NULL
);
CREATE TABLE categories (
  id   INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL UNIQUE
);
CREATE TABLE tags (
  id   INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL UNIQUE
);
CREATE TABLE note_tags (
  note_id INTEGER NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
  tag_id  INTEGER NOT NULL REFERENCES tags(id)  ON DELETE CASCADE,
  PRIMARY KEY (note_id, tag_id)
);
CREATE TABLE todos (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  content    TEXT NOT NULL,
  date       TEXT NOT NULL,                    -- YYYY-MM-DD
  start_min  INTEGER,                         -- 自午夜分钟数 0–1439；start/end 同 NULL = 全天
  end_min    INTEGER,                         -- 同上；两列由 Rust tuple 类型保证同 NULL 或同值
  done       INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX idx_notes_updated ON notes(updated_at DESC);
CREATE INDEX idx_todos_date ON todos(date);
```

## 7. 关键数据流：磁贴保存 → 主窗口刷新

```mermaid
sequenceDiagram
    participant T as 磁贴 TileView
    participant N as NotesState（共享 Entity）
    participant D as core::Db
    participant L as 主窗口·笔记页

    T->>T: InputEvent::Change
    T->>N: save(NoteDraft)
    N->>N: 重置该 key 的 300ms 防抖
    Note over N: 防抖到期
    N->>D: upsert_note(&mut note)
    D-->>N: Ok（回填 id）
    N-->>T: Event::Saved(key, id, Saved)
    T->>T: 顶栏显示“已保存”
    N-->>L: Event::Changed
    L->>D: notes(filter) 重拉列表
```

## 8. 图片处理策略（定案）

**粘贴流程**：向输入框粘贴图片 → App 将图片存储到 `{data_dir}/imgs/`（内容哈希命名，天然去重）→ 在光标处插入 md 语法引用 `![图片](imgs/{hash}.{ext})`，路径指向 `{data_dir}/imgs/{name}`。

**手写引用**：用户可使用 md 语法手动指定绝对路径的图片。

**渲染解析规则**：

| 正文中的引用          | 解析结果                 |
| --------------------- | ------------------------ |
| `imgs/...` 相对引用 | `{data_dir}/imgs/...` |
| 绝对路径              | 原样加载                 |
| 其他相对路径          | 不渲染为图片，按原文展示 |

三条解析规则与回退行为经 spike ② 验证通过。

## 9. gpui-kit 0.7.1 API 基线

**facade 分层**（`gpui-kit` 是门面 crate，应用只依赖它）：

| 路径                    | 实际 crate          | 内容                                                               |
| ----------------------- | ------------------- | ------------------------------------------------------------------ |
| `gpui_kit::*`         | gpui（gpui-pre 系） | GPUI 全量 API                                                      |
| `gpui_kit::base`      | gpui-base           | `Root` / `TextView` / `markdown()` / `html()`              |
| `gpui_kit::component` | gpui-component      | `Button` / `Input` / `Textarea` / `Sidebar` / `Theme` 等 |
| `gpui_kit::assets`    | gpui-kit-assets     | 图标；`AllAssets::new("")` 全量 Lucide                           |

**开窗门面**（本架构统一入口）：

```rust
pub fn open_window<V: Render>(
    options: WindowOptions,
    cx: &mut App,
    build: impl FnOnce(&mut Window, &mut App) -> Entity<V>,
) -> Result<(AnyWindowHandle, Entity<V>)>
// 自动包 base::Root（overlay hosting），builder 只返回业务视图
```

**主题**：`Theme` 全局单例，`ActiveTheme` trait（`cx.theme()`）；`ThemeRegistry` 加载内置 JSON 配置，`ui/theme.rs` 通过 `Theme::update` 与 `apply_config` 应用颜色并同步全部窗口。开发构建通过 `ThemeRegistry::watch_dir` 监听主题源文件，重载后重新应用尺寸规范。主题配色归 JSON，尺寸与文字角色归 `ui/tokens.rs`。

**侧栏**：`Sidebar::new().collapsible(true).collapsed(state)` + `SidebarToggleButton`。

**多行输入**：单行 `InputState`、多行 `TextareaState`（`component::input::*`），构造需 `(window, cx)`。

**测试**：`test-support` feature + `#[gpui_kit::test]` 宏——headless 窗口渲染真实组件、派发指针/键盘事件、断言状态。

**0.6 期验证经验**（0.7.1 经 spike ④/⑤ 复验，条目 1–5 通过、条目 6 留待 Phase 8 release 手动验收；0.6 原始记录见 `docs/experiences/gpui-kit-validation.md`）：

1. `PopUp` + `titlebar: None` 的窗口 Windows 后端自带 `WS_EX_TOPMOST | WS_EX_TOOLWINDOW`；取消置顶需 `SetWindowPos(HWND_NOTOPMOST)`
2. `window_control_area(Drag)` 不能覆盖含按钮的区域（左键点击会被系统拖拽循环吞掉），只放在无交互元素的中间区域
3. `GlobalHotKeyManager` 析构即注销热键，须 `Box::leak` 保活整个进程
4. 热键事件须过滤 `HotKeyState::Pressed`
5. `GetCursorPos` 返回物理像素，逻辑坐标 = 物理坐标 ÷ 目标显示器有效 DPI——`MonitorFromPoint` + `GetDpiForMonitor`（多显示器不同缩放时 `GetDpiForSystem` 取错屏）。`window_bounds` origin 即目标显示器逻辑坐标：后端按目标显示器 scale 换算回物理坐标，且 bounds 中心点必须落在目标显示器上，否则整个 bounds 被丢弃、窗口回退到默认居中；鼠标定位开窗必须同时设置 `display_id`（未设置时目标显示器 = 主显示器）
6. 磁贴打开性能验证用 release 构建

**0.7.1 spike 新增结论**（Phase 2 验证；各阶段实施要点见 `docs/experiences/phase2-spike-conclusions.md`）：

7. `Textarea::on_paste` 官方粘贴拦截点与 handler 内实体更新约束（§5.9）
8. kit 主题不自动跟随系统外观，Auto 为应用层桥接（§5.3 / §5.6）
9. `QuitMode::Explicit` 全窗口关闭进程驻留、热键可用，`cx.quit()` 完全退出（Phase 8 进程常驻与退出的实现依据）

## 10. spike 验证结论（原待验证技术点，Phase 2 全部落定）

1. `Textarea` 粘贴图片钩子：kit 0.7.1 已提供 `on_paste` 官方拦截点，机制与 handler 内实体更新约束见 §5.9（spike ① 通过）
2. markdown 自定义图片渲染：`MarkdownPlugin` 在 `parse` 阶段拦截 `mdast::Node::Image`、`render` 落地 §8 三条解析规则与回退（spike ② 通过）；无现成 "src → 路径解析器" API，插件自行实现
3. Auto 主题：Windows 下 `observe_window_appearance` 可区分 Light/Dark，kit 主题不自动跟随系统外观，Auto 为应用层桥接（§5.3 / §5.6；spike ③ 通过）

## 11. 测试策略

- **core**：`Db::open_in_memory()` 单测——筛选 / 标签 / 分类 / 迁移 / 导出 / settings 往返 / 摘要截断
- **state / UI**：`#[gpui_kit::test]` headless 集成测试——防抖保存、事件广播、跨窗口同步
- **视觉与交互**：人工验收

## 12. 实施顺序

1. workspace 骨架 + core（model → db → settings → media / export，附测试）
2. spike（与步骤 1 并行）：① 图片粘贴——Ctrl+V 监听 + 剪贴板图片检测（图与文本并存时图片优先）；② markdown 图片渲染——`MarkdownPlugin` 拦截 image 节点落地 §8 规则；③ Auto 主题——Windows 下 `observe_window_appearance` 能否区分 Light/Dark；④ §9 六条 0.6 期约束在 0.7.1 复验。结论回写 §9 / §10 后再进入编辑器实现
3. state 层 + tile 迁移到新架构（恢复到验证期同等功能：热键呼出、防抖自动保存、置顶；新增关闭 flush 语义）
4. 主窗口 shell + 笔记页（列表 / 筛选 / 右键分类标签 / 卡片删除 / 编辑器三模式 / 图片粘贴）
5. 待办页（首页日历随 Phase 12 废弃，首页内容待定）
6. 设置页（主题 Light / Dark / Auto、字号、热键、数据目录与迁移、导出、自启动、关闭退出、托盘与静默启动，ADR-0004/0006/0009）
