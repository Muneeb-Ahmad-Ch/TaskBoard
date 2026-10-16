//! RFC 5545 RRULE subset (SPEC §4.3), expanded with `jiff` in the event's own
//! time zone so a weekly 09:00 meeting stays at 09:00 across DST changes.
//!
//! Supported: `FREQ=DAILY|WEEKLY|MONTHLY|YEARLY`, `INTERVAL`, `BYDAY` (with
//! ordinals for monthly, e.g. `2TU`, `-1FR`), `BYMONTHDAY`, `COUNT`, `UNTIL`
//! (date or UTC date-time; inclusive, compared as a local date).

use std::fmt;

use jiff::ToSpan;
use jiff::civil::{Date, DateTime, Weekday};
use jiff::tz::TimeZone;

use crate::error::{Result, invalid};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freq {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub freq: Freq,
    pub interval: u32,
    /// `(ordinal, weekday)`; ordinal only meaningful for MONTHLY.
    pub by_day: Vec<(Option<i8>, Weekday)>,
    pub by_month_day: Vec<i8>,
    pub count: Option<u32>,
    pub until: Option<Date>,
}

const DAYS: [(&str, Weekday); 7] = [
    ("MO", Weekday::Monday),
    ("TU", Weekday::Tuesday),
    ("WE", Weekday::Wednesday),
    ("TH", Weekday::Thursday),
    ("FR", Weekday::Friday),
    ("SA", Weekday::Saturday),
    ("SU", Weekday::Sunday),
];

fn day_code(w: Weekday) -> &'static str {
    DAYS.iter().find(|(_, d)| *d == w).map_or("MO", |(c, _)| c)
}

/// Safety cap on generated periods, so a malformed rule can't spin forever.
const MAX_PERIODS: usize = 20_000;

impl Rule {
    pub fn parse(s: &str) -> Result<Self> {
        let body = s.trim().trim_start_matches("RRULE:");
        let mut freq = None;
        let mut rule = Rule {
            freq: Freq::Daily,
            interval: 1,
            by_day: Vec::new(),
            by_month_day: Vec::new(),
            count: None,
            until: None,
        };
        for part in body.split(';').filter(|p| !p.is_empty()) {
            let (key, value) = part
                .split_once('=')
                .ok_or_else(|| invalid(format!("bad repeat rule part {part:?}")))?;
            match key.to_ascii_uppercase().as_str() {
                "FREQ" => {
                    freq = Some(match value.to_ascii_uppercase().as_str() {
                        "DAILY" => Freq::Daily,
                        "WEEKLY" => Freq::Weekly,
                        "MONTHLY" => Freq::Monthly,
                        "YEARLY" => Freq::Yearly,
                        other => return Err(invalid(format!("unsupported FREQ {other}"))),
                    })
                }
                "INTERVAL" => {
                    rule.interval = value
                        .parse()
                        .ok()
                        .filter(|n| (1..=999).contains(n))
                        .ok_or_else(|| invalid("INTERVAL must be 1-999"))?
                }
                "COUNT" => {
                    rule.count = Some(
                        value
                            .parse()
                            .ok()
                            .filter(|n| (1..=10_000).contains(n))
                            .ok_or_else(|| invalid("COUNT must be 1-10000"))?,
                    )
                }
                "UNTIL" => {
                    let date = value.get(..8).unwrap_or("");
                    let parsed = format!(
                        "{}-{}-{}",
                        date.get(..4).unwrap_or(""),
                        date.get(4..6).unwrap_or(""),
                        date.get(6..8).unwrap_or("")
                    );
                    rule.until = Some(
                        parsed
                            .parse()
                            .map_err(|_| invalid(format!("bad UNTIL {value}")))?,
                    );
                }
                "BYDAY" => {
                    for item in value.split(',') {
                        let item = item.trim().to_ascii_uppercase();
                        let split = item.len().saturating_sub(2);
                        let (ord, code) = item.split_at(split);
                        let day = DAYS
                            .iter()
                            .find(|(c, _)| *c == code)
                            .map(|(_, d)| *d)
                            .ok_or_else(|| invalid(format!("bad BYDAY {item}")))?;
                        let ord = if ord.is_empty() {
                            None
                        } else {
                            Some(
                                ord.parse::<i8>()
                                    .ok()
                                    .filter(|n| *n != 0 && (-5..=5).contains(n))
                                    .ok_or_else(|| invalid(format!("bad BYDAY {item}")))?,
                            )
                        };
                        if !rule.by_day.contains(&(ord, day)) {
                            rule.by_day.push((ord, day));
                        }
                    }
                }
                "BYMONTHDAY" => {
                    for item in value.split(',') {
                        let d: i8 = item
                            .trim()
                            .parse()
                            .ok()
                            .filter(|d: &i8| *d != 0 && (-31..=31).contains(d))
                            .ok_or_else(|| invalid(format!("bad BYMONTHDAY {item}")))?;
                        if !rule.by_month_day.contains(&d) {
                            rule.by_month_day.push(d);
                        }
                    }
                }
                // Accepted and ignored: we always start weeks on Monday.
                "WKST" => {}
                other => return Err(invalid(format!("unsupported repeat rule part {other}"))),
            }
        }
        rule.freq = freq.ok_or_else(|| invalid("repeat rule needs FREQ"))?;
        Ok(rule)
    }

    /// Dates of the period `k` (0-based) that are on or after `start`.
    fn period_dates(&self, start: Date, k: i64) -> Vec<Date> {
        let step = i64::from(self.interval) * k;
        let mut out = match self.freq {
            Freq::Daily => start.checked_add(step.days()).ok().into_iter().collect(),
            Freq::Weekly => {
                let monday = start
                    .checked_sub(i64::from(start.weekday().to_monday_zero_offset()).days())
                    .ok();
                let days: Vec<Weekday> = if self.by_day.is_empty() {
                    vec![start.weekday()]
                } else {
                    self.by_day.iter().map(|(_, d)| *d).collect()
                };
                let mut dates: Vec<Date> = monday
                    .and_then(|m| m.checked_add((7 * step).days()).ok())
                    .map(|m| {
                        days.iter()
                            .filter_map(|d| {
                                m.checked_add(i64::from(d.to_monday_zero_offset()).days())
                                    .ok()
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                dates.sort();
                dates.dedup();
                dates
            }
            Freq::Monthly => {
                let Ok(first) = start.first_of_month().checked_add(step.months()) else {
                    return Vec::new();
                };
                let (y, m) = (first.year(), first.month());
                let days_in = first.days_in_month();
                let mut dates = Vec::new();
                if self.by_day.is_empty()
                    && self.by_month_day.is_empty()
                    && let Ok(d) = Date::new(y, m, start.day())
                {
                    dates.push(d);
                }
                for &md in &self.by_month_day {
                    let day = if md > 0 { md } else { days_in + md + 1 };
                    if let Ok(d) = Date::new(y, m, day) {
                        dates.push(d);
                    }
                }
                for &(ord, wd) in &self.by_day {
                    let all: Vec<Date> = (1..=days_in)
                        .filter_map(|d| Date::new(y, m, d).ok())
                        .filter(|d| d.weekday() == wd)
                        .collect();
                    match ord {
                        None => dates.extend(all),
                        Some(n) if n > 0 => {
                            dates.extend(all.get(usize::from(n.unsigned_abs()) - 1))
                        }
                        Some(n) => {
                            let back = usize::from(n.unsigned_abs());
                            dates.extend(all.len().checked_sub(back).and_then(|i| all.get(i)));
                        }
                    }
                }
                dates.sort();
                dates.dedup();
                dates
            }
            Freq::Yearly => Date::new(
                start
                    .year()
                    .saturating_add(i16::try_from(step).unwrap_or(i16::MAX)),
                start.month(),
                start.day(),
            )
            .ok()
            .into_iter()
            .collect(),
        };
        out.retain(|d| *d >= start);
        out
    }

    /// Occurrence start dates from `start`, in order, honouring COUNT/UNTIL,
    /// stopping once `keep_going(date)` returns false.
    pub fn dates(&self, start: Date, mut keep_going: impl FnMut(Date) -> bool) -> Vec<Date> {
        let mut out = Vec::new();
        let mut emitted: u32 = 0;
        for k in 0..MAX_PERIODS {
            for d in self.period_dates(start, k as i64) {
                if self.until.is_some_and(|u| d > u) || self.count.is_some_and(|c| emitted >= c) {
                    return out;
                }
                if !keep_going(d) {
                    return out;
                }
                emitted += 1;
                out.push(d);
            }
        }
        out
    }

    /// Occurrence start instants (UTC ms) of a timed series whose first
    /// occurrence starts at `dtstart` local time in `tz`, overlapping
    /// `[from, to)` given the occurrence `duration_ms`.
    pub fn occurrences(
        &self,
        dtstart: DateTime,
        tz: &TimeZone,
        duration_ms: i64,
        from: i64,
        to: i64,
    ) -> Vec<i64> {
        let time = dtstart.time();
        let mut out = Vec::new();
        let to_ms = |d: Date| -> Option<i64> {
            tz.to_zoned(d.to_datetime(time))
                .ok()
                .map(|z| z.timestamp().as_millisecond())
        };
        self.dates(dtstart.date(), |d| to_ms(d).is_some_and(|ms| ms < to))
            .into_iter()
            .filter_map(to_ms)
            .filter(|ms| ms + duration_ms > from)
            .for_each(|ms| out.push(ms));
        out
    }

    /// Short human description, e.g. "Every 2 weeks on Mon, Wed".
    pub fn describe(&self) -> String {
        let unit = match self.freq {
            Freq::Daily => "day",
            Freq::Weekly => "week",
            Freq::Monthly => "month",
            Freq::Yearly => "year",
        };
        let mut s = if self.interval == 1 {
            format!("Every {unit}")
        } else {
            format!("Every {} {unit}s", self.interval)
        };
        if !self.by_day.is_empty() {
            let names: Vec<String> = self
                .by_day
                .iter()
                .map(|(ord, d)| {
                    let name = match d {
                        Weekday::Monday => "Mon",
                        Weekday::Tuesday => "Tue",
                        Weekday::Wednesday => "Wed",
                        Weekday::Thursday => "Thu",
                        Weekday::Friday => "Fri",
                        Weekday::Saturday => "Sat",
                        Weekday::Sunday => "Sun",
                    };
                    match ord {
                        Some(-1) => format!("last {name}"),
                        Some(n) => format!("{} {name}", ordinal(*n)),
                        None => name.to_owned(),
                    }
                })
                .collect();
            s.push_str(&format!(" on {}", names.join(", ")));
        }
        if !self.by_month_day.is_empty() {
            let days: Vec<String> = self.by_month_day.iter().map(|d| ordinal(*d)).collect();
            s.push_str(&format!(" on the {}", days.join(", ")));
        }
        if let Some(c) = self.count {
            s.push_str(&format!(", {c} times"));
        }
        if let Some(u) = self.until {
            s.push_str(&format!(", until {u}"));
        }
        s
    }
}

fn ordinal(n: i8) -> String {
    match n {
        -1 => "last".into(),
        n if n < 0 => format!("{} from last", ordinal(-n)),
        1 | 21 | 31 => format!("{n}st"),
        2 | 22 => format!("{n}nd"),
        3 | 23 => format!("{n}rd"),
        n => format!("{n}th"),
    }
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let freq = match self.freq {
            Freq::Daily => "DAILY",
            Freq::Weekly => "WEEKLY",
            Freq::Monthly => "MONTHLY",
            Freq::Yearly => "YEARLY",
        };
        write!(f, "FREQ={freq}")?;
        if self.interval != 1 {
            write!(f, ";INTERVAL={}", self.interval)?;
        }
        if !self.by_day.is_empty() {
            let days: Vec<String> = self
                .by_day
                .iter()
                .map(|(o, d)| {
                    format!(
                        "{}{}",
                        o.map(|n| n.to_string()).unwrap_or_default(),
                        day_code(*d)
                    )
                })
                .collect();
            write!(f, ";BYDAY={}", days.join(","))?;
        }
        if !self.by_month_day.is_empty() {
            let days: Vec<String> = self.by_month_day.iter().map(i8::to_string).collect();
            write!(f, ";BYMONTHDAY={}", days.join(","))?;
        }
        if let Some(c) = self.count {
            write!(f, ";COUNT={c}")?;
        }
        if let Some(u) = self.until {
            write!(f, ";UNTIL={}", u.strftime("%Y%m%d"))?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use jiff::civil::date;
    use proptest::prelude::*;

    fn dates(rule: &str, start: Date, n: usize) -> Vec<String> {
        let r = Rule::parse(rule).unwrap();
        let mut seen = 0;
        r.dates(start, |_| {
            seen += 1;
            seen <= n
        })
        .into_iter()
        .map(|d| d.to_string())
        .collect()
    }

    #[test]
    fn weekly_on_days() {
        // 2026-10-15 is a Thursday.
        assert_eq!(
            dates("FREQ=WEEKLY;BYDAY=MO,TH", date(2026, 10, 15), 4),
            ["2026-10-15", "2026-10-19", "2026-10-22", "2026-10-26"]
        );
        assert_eq!(
            dates("FREQ=WEEKLY;INTERVAL=2", date(2026, 10, 15), 3),
            ["2026-10-15", "2026-10-29", "2026-11-12"]
        );
    }

    #[test]
    fn monthly_ordinals_and_month_days() {
        assert_eq!(
            dates("FREQ=MONTHLY;BYDAY=2TU", date(2026, 10, 1), 3),
            ["2026-10-13", "2026-11-10", "2026-12-08"]
        );
        assert_eq!(
            dates("FREQ=MONTHLY;BYDAY=-1FR", date(2026, 10, 1), 2),
            ["2026-10-30", "2026-11-27"]
        );
        // The 31st skips short months.
        assert_eq!(
            dates("FREQ=MONTHLY", date(2026, 10, 31), 3),
            ["2026-10-31", "2026-12-31", "2027-01-31"]
        );
        assert_eq!(
            dates("FREQ=MONTHLY;BYMONTHDAY=-1", date(2026, 1, 15), 2),
            ["2026-01-31", "2026-02-28"]
        );
    }

    #[test]
    fn count_until_and_yearly() {
        assert_eq!(dates("FREQ=DAILY;COUNT=3", date(2026, 10, 15), 99).len(), 3);
        assert_eq!(
            dates("FREQ=DAILY;UNTIL=20261017", date(2026, 10, 15), 99),
            ["2026-10-15", "2026-10-16", "2026-10-17"]
        );
        assert_eq!(
            dates("FREQ=YEARLY", date(2028, 2, 29), 2),
            ["2028-02-29", "2032-02-29"]
        );
    }

    #[test]
    fn keeps_local_time_across_dst() {
        // London switches to GMT on 2026-10-25; a 09:00 meeting stays 09:00 local.
        let tz = TimeZone::get("Europe/London").unwrap();
        let r = Rule::parse("FREQ=WEEKLY").unwrap();
        let start = date(2026, 10, 19).at(9, 0, 0, 0);
        let hour = 3_600_000;
        let from = tz.to_zoned(start).unwrap().timestamp().as_millisecond();
        let occ = r.occurrences(start, &tz, hour, from, from + 15 * 86_400_000);
        assert_eq!(occ.len(), 3);
        for ms in occ {
            let local = jiff::Timestamp::from_millisecond(ms)
                .unwrap()
                .to_zoned(tz.clone());
            assert_eq!((local.hour(), local.minute()), (9, 0));
        }
    }

    #[test]
    fn round_trips_and_describes() {
        for s in [
            "FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE",
            "FREQ=MONTHLY;BYDAY=-1FR;COUNT=6",
            "FREQ=DAILY;UNTIL=20261231",
        ] {
            assert_eq!(Rule::parse(s).unwrap().to_string(), s);
        }
        assert_eq!(
            Rule::parse("FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE")
                .unwrap()
                .describe(),
            "Every 2 weeks on Mon, Wed"
        );
        assert_eq!(
            Rule::parse("FREQ=MONTHLY;BYDAY=2TU").unwrap().describe(),
            "Every month on 2nd Tue"
        );
        assert!(Rule::parse("FREQ=HOURLY").is_err());
        assert!(Rule::parse("BYDAY=MO").is_err());
        assert!(Rule::parse("FREQ=WEEKLY;BYDAY=XX").is_err());
    }

    proptest! {
        /// Occurrences are strictly increasing, start on/after dtstart, and
        /// respect COUNT.
        #[test]
        fn occurrences_are_ordered(
            freq in prop::sample::select(vec!["DAILY", "WEEKLY", "MONTHLY", "YEARLY"]),
            interval in 1u32..5,
            count in 1u32..40,
            day in 1i8..29,
            days in prop::collection::vec(prop::sample::select(vec!["MO","TU","WE","TH","FR","SA","SU"]), 0..3),
        ) {
            let mut rule = format!("FREQ={freq};INTERVAL={interval};COUNT={count}");
            if freq == "WEEKLY" && !days.is_empty() {
                rule.push_str(&format!(";BYDAY={}", days.join(",")));
            }
            let start = date(2026, 3, day);
            let out = Rule::parse(&rule).unwrap().dates(start, |_| true);
            prop_assert!(out.len() <= count as usize);
            prop_assert!(out.iter().all(|d| *d >= start));
            prop_assert!(out.windows(2).all(|w| w[0] < w[1]));
        }
    }
}
