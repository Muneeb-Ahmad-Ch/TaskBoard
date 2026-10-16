//! Domain types shared by the core, the CLI and (via ts-rs) the UI.

use rusqlite::types::Value;
use serde::{Deserialize, Deserializer, Serialize};
use ts_rs::TS;

use crate::error::{Result, invalid};
use crate::id::Id;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Priority {
    None,
    Low,
    Med,
    High,
    Urgent,
}

impl Priority {
    pub fn as_i64(self) -> i64 {
        self as i64
    }

    pub fn from_i64(v: i64) -> Self {
        match v {
            1 => Self::Low,
            2 => Self::Med,
            3 => Self::High,
            4 => Self::Urgent,
            _ => Self::None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Category {
    Todo,
    Active,
    Done,
    Cancelled,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Todo => "todo",
            Self::Active => "active",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "todo" => Some(Self::Todo),
            "active" => Some(Self::Active),
            "done" => Some(Self::Done),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    /// Closed tasks have `completed_at` set.
    pub fn is_closed(self) -> bool {
        matches!(self, Self::Done | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ProjectState {
    Active,
    OnHold,
    Completed,
}

impl ProjectState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::OnHold => "on_hold",
            Self::Completed => "completed",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "on_hold" => Self::OnHold,
            "completed" => Self::Completed,
            _ => Self::Active,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct Client {
    pub id: Id,
    pub name: String,
    pub color: String,
    pub icon: Option<String>,
    pub notes_md: String,
    pub contact: String,
    pub time_zone: Option<String>,
    pub work_start: Option<String>,
    pub work_end: Option<String>,
    pub meetings_count_as_time: bool,
    pub sort_key: String,
    pub archived_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct Project {
    pub id: Id,
    pub client_id: Option<Id>,
    pub name: String,
    pub code: Option<String>,
    pub color: String,
    pub icon: Option<String>,
    pub state: ProjectState,
    pub start_date: Option<String>,
    pub deadline: Option<String>,
    pub is_inbox: bool,
    pub sort_key: String,
    pub archived_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A project with its client name and task counts, for sidebars and pickers.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ProjectSummary {
    #[serde(flatten)]
    #[ts(flatten)]
    pub project: Project,
    pub client_name: Option<String>,
    pub open_tasks: i64,
    pub total_tasks: i64,
    pub done_tasks: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct Status {
    pub id: Id,
    pub project_id: Id,
    pub name: String,
    pub color: String,
    pub category: Category,
    pub wip_limit: Option<i64>,
    pub sort_key: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct Task {
    pub id: Id,
    pub project_id: Id,
    pub parent_id: Option<Id>,
    pub status_id: Id,
    pub title: String,
    pub description_md: String,
    pub priority: Priority,
    pub start_date: Option<String>,
    pub due_date: Option<String>,
    pub due_time: Option<String>,
    pub estimate_minutes: Option<i64>,
    pub sort_key: String,
    pub recurrence_rule: Option<String>,
    pub completed_at: Option<i64>,
    pub archived_at: Option<i64>,
    pub deleted_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub tags: Vec<String>,
}

// ---------- inputs ----------

#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(export)]
pub struct NewClient {
    pub name: String,
    #[ts(optional)]
    pub color: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub contact: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub notes_md: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub time_zone: Option<String>,
}

/// Field present = change it. Nullable fields use `null` to clear.
#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(export)]
pub struct ClientPatch {
    #[ts(optional)]
    pub name: Option<String>,
    #[ts(optional)]
    pub color: Option<String>,
    #[ts(optional)]
    pub contact: Option<String>,
    #[ts(optional)]
    pub notes_md: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    #[ts(optional, type = "string | null")]
    pub time_zone: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    #[ts(optional, type = "string | null")]
    pub work_start: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    #[ts(optional, type = "string | null")]
    pub work_end: Option<Option<String>>,
    #[ts(optional)]
    pub meetings_count_as_time: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(export)]
pub struct NewProject {
    pub name: String,
    #[serde(default)]
    #[ts(optional)]
    pub client_id: Option<Id>,
    #[serde(default)]
    #[ts(optional)]
    pub color: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub code: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub deadline: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(export)]
pub struct ProjectPatch {
    #[ts(optional)]
    pub name: Option<String>,
    #[ts(optional)]
    pub color: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    #[ts(optional, type = "string | null")]
    pub client_id: Option<Option<Id>>,
    #[serde(default, deserialize_with = "nullable")]
    #[ts(optional, type = "string | null")]
    pub code: Option<Option<String>>,
    #[ts(optional)]
    pub state: Option<ProjectState>,
    #[serde(default, deserialize_with = "nullable")]
    #[ts(optional, type = "string | null")]
    pub start_date: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    #[ts(optional, type = "string | null")]
    pub deadline: Option<Option<String>>,
}

#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(export)]
pub struct NewTask {
    pub title: String,
    /// Defaults to the Inbox.
    #[serde(default)]
    #[ts(optional)]
    pub project_id: Option<Id>,
    #[serde(default)]
    #[ts(optional)]
    pub parent_id: Option<Id>,
    /// Defaults to the project's default status ("To Do").
    #[serde(default)]
    #[ts(optional)]
    pub status_id: Option<Id>,
    #[serde(default)]
    #[ts(optional)]
    pub description_md: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub priority: Option<Priority>,
    #[serde(default)]
    #[ts(optional)]
    pub start_date: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub due_date: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub due_time: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub estimate_minutes: Option<i64>,
    #[serde(default)]
    #[ts(as = "Option<Vec<String>>", optional)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(export)]
pub struct TaskPatch {
    #[ts(optional)]
    pub title: Option<String>,
    #[ts(optional)]
    pub description_md: Option<String>,
    #[ts(optional)]
    pub priority: Option<Priority>,
    #[ts(optional)]
    pub project_id: Option<Id>,
    #[serde(default, deserialize_with = "nullable")]
    #[ts(optional, type = "string | null")]
    pub parent_id: Option<Option<Id>>,
    #[ts(optional)]
    pub status_id: Option<Id>,
    #[serde(default, deserialize_with = "nullable")]
    #[ts(optional, type = "string | null")]
    pub start_date: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    #[ts(optional, type = "string | null")]
    pub due_date: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    #[ts(optional, type = "string | null")]
    pub due_time: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    #[ts(optional, type = "number | null")]
    pub estimate_minutes: Option<Option<i64>>,
    #[ts(optional)]
    pub tags: Option<Vec<String>>,
}

/// `field: null` → `Some(None)` (clear); missing → `None` (leave unchanged).
fn nullable<'de, D, T>(de: D) -> std::result::Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(de).map(Some)
}

// ---------- validation ----------

const MAX_NAME: usize = 300;

pub(crate) fn clean_name(s: &str, what: &str) -> Result<String> {
    let s = s.trim();
    if s.is_empty() {
        return Err(invalid(format!("{what} cannot be empty")));
    }
    if s.chars().count() > MAX_NAME {
        return Err(invalid(format!(
            "{what} is longer than {MAX_NAME} characters"
        )));
    }
    Ok(s.to_owned())
}

/// Civil date, normalized to `YYYY-MM-DD`.
pub(crate) fn clean_date(s: &str) -> Result<String> {
    s.trim()
        .parse::<jiff::civil::Date>()
        .map(|d| d.to_string())
        .map_err(|_| invalid(format!("not a date (YYYY-MM-DD): {s:?}")))
}

/// Civil time, normalized to `HH:MM`.
pub(crate) fn clean_time(s: &str) -> Result<String> {
    let t = s
        .trim()
        .parse::<jiff::civil::Time>()
        .map_err(|_| invalid(format!("not a time (HH:MM): {s:?}")))?;
    Ok(format!("{:02}:{:02}", t.hour(), t.minute()))
}

pub(crate) fn clean_tz(s: &str) -> Result<String> {
    let s = s.trim();
    jiff::tz::TimeZone::get(s).map_err(|_| invalid(format!("unknown time zone {s:?}")))?;
    Ok(s.to_owned())
}

/// Colors are an OKLCH hue (`"0"`–`"360"`) or `"gray"`.
pub(crate) fn clean_color(s: &str) -> Result<String> {
    let s = s.trim();
    if s == "gray" || s.parse::<u16>().is_ok_and(|h| h <= 360) {
        Ok(s.to_owned())
    } else {
        Err(invalid(format!(
            "color must be a hue 0-360 or \"gray\": {s:?}"
        )))
    }
}

/// Hues for new clients / projects, picked round-robin.
pub(crate) const PALETTE: [u16; 12] = [262, 190, 45, 150, 10, 300, 230, 100, 330, 70, 210, 130];

pub(crate) fn palette_color(n: i64) -> String {
    let i = usize::try_from(n.rem_euclid(PALETTE.len() as i64)).unwrap_or(0);
    PALETTE.get(i).copied().unwrap_or(262).to_string()
}

// ---------- SQL value helpers ----------

pub(crate) fn opt_text(v: Option<String>) -> Value {
    v.map_or(Value::Null, Value::Text)
}

pub(crate) fn opt_id(v: Option<Id>) -> Value {
    v.map_or(Value::Null, Value::from)
}

pub(crate) fn opt_int(v: Option<i64>) -> Value {
    v.map_or(Value::Null, Value::Integer)
}

pub(crate) fn text(s: impl Into<String>) -> Value {
    Value::Text(s.into())
}

pub(crate) fn int(i: i64) -> Value {
    Value::Integer(i)
}

pub(crate) fn boolean(b: bool) -> Value {
    Value::Integer(i64::from(b))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn validators() {
        assert_eq!(clean_date(" 2026-10-20 ").unwrap(), "2026-10-20");
        assert!(clean_date("2026-02-30").is_err());
        assert_eq!(clean_time("09:05").unwrap(), "09:05");
        assert_eq!(clean_time("17:30:00").unwrap(), "17:30");
        assert!(clean_time("25:00").is_err());
        assert_eq!(clean_tz("Europe/London").unwrap(), "Europe/London");
        assert!(clean_tz("Mars/Base").is_err());
        assert!(clean_color("262").is_ok() && clean_color("gray").is_ok());
        assert!(clean_color("400").is_err() && clean_color("red").is_err());
        assert!(clean_name("   ", "name").is_err());
    }

    #[test]
    fn patch_distinguishes_missing_from_null() {
        let p: TaskPatch = serde_json::from_str(r#"{"due_date": null}"#).unwrap();
        assert_eq!(p.due_date, Some(None));
        assert_eq!(p.start_date, None);
        let p: TaskPatch = serde_json::from_str(r#"{"due_date": "2026-10-20"}"#).unwrap();
        assert_eq!(p.due_date, Some(Some("2026-10-20".into())));
    }
}
