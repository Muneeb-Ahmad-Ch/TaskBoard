//! Taskboard core: domain model, storage, queries. No UI code.

mod clients;
pub mod clock;
pub mod error;
pub mod id;
pub mod model;
pub mod order;
mod projects;
pub mod query;
pub mod quickadd;
mod statuses;
pub mod storage;
mod tasks;
pub mod tx;
pub mod views;

pub use error::{Error, ErrorInfo, Result};
pub use id::Id;
pub use model::*;
pub use query::{TaskPage, TaskQuery, TaskRow, TaskSort};
pub use statuses::{NewStatus, StatusPatch};
pub use storage::{Db, DbInfo, HistoryState};
pub use tasks::MoveTask;
pub use views::{ProjectRef, QuickAddPreview, SidebarCounts, TaskSummary};
