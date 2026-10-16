//! Taskboard core: domain model, storage, queries. No UI code.

pub mod error;
pub mod storage;

pub use error::{Error, ErrorInfo, Result};
pub use storage::{Db, DbInfo};
