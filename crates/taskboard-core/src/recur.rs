//! Repeating tasks (SPEC §4.3). A task's `recurrence_rule` is either an RRULE
//! (fixed schedule, e.g. every Monday) or `AFTER:<n><D|W|M>` (the next one is
//! due n days/weeks/months after you complete this one). Completing a
//! repeating task creates the next instance; COUNT counts down per instance.

use jiff::ToSpan;
use jiff::civil::Date;

use crate::error::{Result, invalid};
use crate::rrule::Rule;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskRule {
    Fixed(Rule),
    After { n: i64, unit: char },
}

impl TaskRule {
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();
        if let Some(rest) = s.strip_prefix("AFTER:") {
            let unit = rest
                .chars()
                .last()
                .map(|c| c.to_ascii_uppercase())
                .unwrap_or(' ');
            let n: i64 = rest
                .get(..rest.len().saturating_sub(1))
                .and_then(|n| n.parse().ok())
                .filter(|n| (1..=999).contains(n))
                .ok_or_else(|| invalid(format!("bad repeat rule {s:?}")))?;
            if !matches!(unit, 'D' | 'W' | 'M') {
                return Err(invalid("AFTER rules use D, W or M"));
            }
            return Ok(Self::After { n, unit });
        }
        Rule::parse(s).map(Self::Fixed)
    }

    pub fn to_rule_string(&self) -> String {
        match self {
            Self::Fixed(r) => r.to_string(),
            Self::After { n, unit } => format!("AFTER:{n}{unit}"),
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Self::Fixed(r) => r.describe(),
            Self::After { n, unit } => {
                let word = match unit {
                    'W' => "week",
                    'M' => "month",
                    _ => "day",
                };
                format!(
                    "{n} {word}{} after completion",
                    if *n == 1 { "" } else { "s" }
                )
            }
        }
    }

    /// First date a fixed rule produces on or after `from` (for tasks created
    /// with a rule but no date).
    pub fn first_on_or_after(&self, from: Date) -> Option<Date> {
        match self {
            Self::Fixed(r) => {
                // Anchor at `from`; for weekly BYDAY that yields the first matching day.
                let anchored = Rule {
                    count: None,
                    ..r.clone()
                };
                anchored
                    .dates(from, |d| d <= from.checked_add(400.days()).unwrap_or(from))
                    .into_iter()
                    .next()
            }
            Self::After { .. } => None,
        }
    }

    /// The next instance's due date and rule, or `None` when the series ends.
    /// Fixed rules return the first date after both the current due date and
    /// the completion day (so finishing an overdue task doesn't create another
    /// overdue one).
    pub fn next(&self, due: Option<Date>, completed_on: Date) -> Option<(Date, TaskRule)> {
        match self {
            Self::After { n, unit } => {
                let span = match unit {
                    'W' => n.weeks(),
                    'M' => n.months(),
                    _ => n.days(),
                };
                completed_on
                    .checked_add(span)
                    .ok()
                    .map(|d| (d, self.clone()))
            }
            Self::Fixed(r) => {
                let next_count = match r.count {
                    Some(c) if c <= 1 => return None,
                    Some(c) => Some(c - 1),
                    None => None,
                };
                let anchor = due.unwrap_or(completed_on);
                let floor = anchor.max(completed_on);
                let horizon = floor.checked_add(5.years()).ok()?;
                let walker = Rule {
                    count: None,
                    ..r.clone()
                };
                let date = walker
                    .dates(anchor, |d| d <= horizon)
                    .into_iter()
                    .find(|d| *d > floor)?;
                if r.until.is_some_and(|u| date > u) {
                    return None;
                }
                Some((
                    date,
                    Self::Fixed(Rule {
                        count: next_count,
                        ..r.clone()
                    }),
                ))
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use jiff::civil::date;

    fn next(rule: &str, due: Option<Date>, done: Date) -> Option<String> {
        TaskRule::parse(rule)
            .unwrap()
            .next(due, done)
            .map(|(d, _)| d.to_string())
    }

    #[test]
    fn fixed_rules() {
        // Weekly on Monday; due Mon 19 Oct.
        let mon = date(2026, 10, 19);
        assert_eq!(
            next("FREQ=WEEKLY;BYDAY=MO", Some(mon), date(2026, 10, 16)).as_deref(),
            Some("2026-10-26"),
            "done early"
        );
        assert_eq!(
            next("FREQ=WEEKLY;BYDAY=MO", Some(mon), date(2026, 11, 4)).as_deref(),
            Some("2026-11-09"),
            "done late skips missed weeks"
        );
        assert_eq!(
            next("FREQ=MONTHLY", Some(date(2026, 1, 31)), date(2026, 1, 31)).as_deref(),
            Some("2026-03-31")
        );
        assert_eq!(
            next(
                "FREQ=DAILY;UNTIL=20261020",
                Some(date(2026, 10, 20)),
                date(2026, 10, 20)
            ),
            None
        );
    }

    #[test]
    fn count_counts_down() {
        let r = TaskRule::parse("FREQ=DAILY;COUNT=2").unwrap();
        let (d, r2) = r.next(Some(date(2026, 10, 1)), date(2026, 10, 1)).unwrap();
        assert_eq!(d.to_string(), "2026-10-02");
        assert_eq!(r2.to_rule_string(), "FREQ=DAILY;COUNT=1");
        assert!(r2.next(Some(d), d).is_none());
    }

    #[test]
    fn after_completion_rules() {
        assert_eq!(
            next("AFTER:3D", Some(date(2026, 10, 1)), date(2026, 10, 16)).as_deref(),
            Some("2026-10-19")
        );
        assert_eq!(
            next("AFTER:2W", None, date(2026, 10, 16)).as_deref(),
            Some("2026-10-30")
        );
        assert_eq!(
            TaskRule::parse("AFTER:1M").unwrap().describe(),
            "1 month after completion"
        );
        assert!(TaskRule::parse("AFTER:0D").is_err());
        assert!(TaskRule::parse("AFTER:3Y").is_err());
    }

    #[test]
    fn first_date_for_new_tasks() {
        let thu = date(2026, 10, 15);
        let r = TaskRule::parse("FREQ=WEEKLY;BYDAY=MO").unwrap();
        assert_eq!(r.first_on_or_after(thu).unwrap().to_string(), "2026-10-19");
        let daily = TaskRule::parse("FREQ=DAILY").unwrap();
        assert_eq!(daily.first_on_or_after(thu).unwrap(), thu);
    }
}
