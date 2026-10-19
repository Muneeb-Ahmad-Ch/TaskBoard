//! Taskboard core: domain model, storage, queries. No UI code.

pub mod backup;
mod clients;
pub mod clock;
pub mod columns;
pub mod dashboard;
pub mod error;
pub mod events;
pub mod exchange;
pub mod id;
pub mod markdown;
pub mod model;
pub mod notify;
pub mod order;
mod pins;
pub mod planning;
mod projects;
pub mod query;
pub mod quickadd;
pub mod recur;
pub mod reorder;
pub mod rrule;
mod sample;
pub mod saved_views;
pub mod sections;
pub mod slots;
mod statuses;
pub mod storage;
mod tasks;
pub mod tx;
pub mod views;

pub use backup::BackupInfo;
pub use columns::DefaultColumn;
pub use dashboard::{ClientSummary, Dashboard, DayCount, ProjectProgress};
pub use error::{Error, ErrorInfo, Result};
pub use events::{
    Event, EventKind, EventPatch, EventStatus, NewEvent, Occurrence, OccurrencePatch,
};
pub use exchange::ImportReport;
pub use id::Id;
pub use model::*;
pub use notify::{AppSettings, DisplaySettings, Notification, NotificationKind, NotifySettings};
pub use planning::{
    ClientHours, DayOverride, DayPlan, FindSlots, Interval, LoggedTime, NewTimeEntry, OverrideKind,
    Slot, SlotDay, SlotResult, TimeEntryPatch, WorkingDay,
};
pub use query::{TaskPage, TaskQuery, TaskRow, TaskSort};
pub use reorder::{Direction, ProjectPlace};
pub use saved_views::{SavedView, SavedViewPatch};
pub use sections::{NewSection, Section, SectionDelete, SectionPatch, Setup};
pub use statuses::{NewStatus, StatusPatch};
pub use storage::{Db, DbInfo, HistoryState};
pub use tasks::MoveTask;
pub use views::{ProjectRef, QuickAddPreview, SidebarCounts, TaskSummary};
