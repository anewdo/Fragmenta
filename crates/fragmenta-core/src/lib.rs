//! fragmenta-core：纯逻辑层（零 gpui 依赖）。
//!
//! 架构铁律：本 crate 的依赖图不含 gpui，数据逻辑可脱离 UI 框架编译与测试；
//! 模块全部依赖注入、无全局静态（架构 §2 / D1 / D2）。

pub mod db;
pub mod export;
pub mod media;
pub mod model;
pub mod settings;
