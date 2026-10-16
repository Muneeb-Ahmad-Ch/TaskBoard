//! Taskboard core: domain model, storage, queries. No UI code.

mod clients;
pub mod clock;
pub mod error;
pub mod events;
pub mod id;
pub mod model;
pub mod order;
pub mod planning;
mod projects;
pub mod query;
pub mod quickadd;
pub mod recur;
pub mod rrule;
pub mod slots;
mod statuses;
pub mod storage;
mod tasks;
pub mod tx;
pub mod views;

pub use error::{Error, ErrorInfo, Result};
pub use events::{
    Event, EventKind, EventPatch, EventStatus, NewEvent, Occurrence, OccurrencePatch,
};
pub use id::Id;
pub use model::*;
pub use planning::{
    ClientHours, DayOverride, DayPlan, FindSlots, Interval, LoggedTime, NewTimeEntry, OverrideKind,
    Slot, SlotDay, SlotResult, TimeEntryPatch, WorkingDay,
};
pub use query::{TaskPage, TaskQuery, TaskRow, TaskSort};
pub use statuses::{NewStatus, StatusPatch};
pub use storage::{Db, DbInfo, HistoryState};
pub use tasks::MoveTask;
pub use views::{ProjectRef, QuickAddPreview, SidebarCounts, TaskSummary};
