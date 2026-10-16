//! Quick Add: natural-language task entry (SPEC §5), shared by app and CLI.
//!
//! `Write report tomorrow 3pm #work !high @Acme ~2h` →
//! title "Write report", due tomorrow 15:00, tag work, priority high,
//! project matching "Acme", estimate 120 min.
//!
//! The first date and first time win; later ones stay in the title. Words in
//! double quotes are always kept literally (`"Monday" standup notes`).

use jiff::ToSpan;
use jiff::civil::{Date, Time, Weekday};
use serde::Serialize;
use ts_rs::TS;

use crate::model::Priority;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TokenKind {
    Tag,
    Priority,
    Project,
    Date,
    Time,
    TimeRange,
    Estimate,
}

/// A recognized span of the input (byte offsets), for live highlighting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct Token {
    pub kind: TokenKind,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ParsedQuickAdd {
    pub title: String,
    pub tags: Vec<String>,
    pub priority: Option<Priority>,
    /// Raw `@…` text; resolved to a project by the caller.
    pub project_hint: Option<String>,
    pub due_date: Option<String>,
    pub due_time: Option<String>,
    /// Set when a range like `3-4pm` was typed (a meeting, from M4).
    pub end_time: Option<String>,
    pub estimate_minutes: Option<i64>,
    pub tokens: Vec<Token>,
}

#[derive(Debug, Clone)]
struct Word<'a> {
    text: &'a str,
    start: usize,
    end: usize,
    quoted: bool,
}

/// Splits on whitespace, keeping `"quoted phrases"` as single literal words.
fn words(input: &str) -> Vec<Word<'_>> {
    let mut out = Vec::new();
    let mut chars = input.char_indices().peekable();
    while let Some(&(i, c)) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        if c == '"' {
            chars.next();
            let start = i;
            let mut end = input.len();
            for (j, d) in chars.by_ref() {
                if d == '"' {
                    end = j + 1;
                    break;
                }
            }
            out.push(Word {
                text: &input[start..end],
                start,
                end,
                quoted: true,
            });
            continue;
        }
        let start = i;
        let mut end = input.len();
        // `@"Multi word project"` is one word up to the closing quote.
        let quoted_project = input[i..].starts_with("@\"");
        let mut in_quotes = false;
        while let Some(&(j, d)) = chars.peek() {
            if d == '"' && quoted_project {
                in_quotes = !in_quotes;
            } else if d.is_whitespace() && !in_quotes {
                end = j;
                break;
            }
            chars.next();
        }
        out.push(Word {
            text: &input[start..end],
            start,
            end,
            quoted: false,
        });
    }
    out
}

fn weekday(s: &str) -> Option<Weekday> {
    Some(match s {
        "mon" | "monday" => Weekday::Monday,
        "tue" | "tues" | "tuesday" => Weekday::Tuesday,
        "wed" | "weds" | "wednesday" => Weekday::Wednesday,
        "thu" | "thur" | "thurs" | "thursday" => Weekday::Thursday,
        "fri" | "friday" => Weekday::Friday,
        "sat" | "saturday" => Weekday::Saturday,
        "sun" | "sunday" => Weekday::Sunday,
        _ => return None,
    })
}

fn month(s: &str) -> Option<i8> {
    let months = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let s = s.trim_end_matches('.');
    if s.len() < 3 {
        return None;
    }
    let prefix = s.get(..3)?;
    let full = [
        "january",
        "february",
        "march",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
    ];
    months.iter().position(|m| *m == prefix).and_then(|i| {
        let ok = s.len() == 3 || full.get(i).is_some_and(|f| f.starts_with(s)) || s == "sept";
        ok.then(|| i8::try_from(i + 1).ok()).flatten()
    })
}

/// Next date on or after `today` falling on `day`.
fn upcoming(today: Date, day: Weekday) -> Date {
    let ahead = (i64::from(day.to_monday_zero_offset())
        - i64::from(today.weekday().to_monday_zero_offset()))
    .rem_euclid(7);
    today.checked_add(ahead.days()).unwrap_or(today)
}

/// Day + month without a year: this year, or next year if already past.
fn day_month(today: Date, m: i8, d: i8) -> Option<Date> {
    let this_year = Date::new(today.year(), m, d).ok()?;
    if this_year >= today {
        Some(this_year)
    } else {
        Date::new(today.year() + 1, m, d).ok()
    }
}

/// Tries to read a date starting at word `i`. Returns (date, words consumed).
fn parse_date(ws: &[Word<'_>], i: usize, today: Date) -> Option<(Date, usize)> {
    let w = |k: usize| {
        ws.get(i + k)
            .filter(|w| !w.quoted)
            .map(|w| w.text.to_lowercase())
    };
    let first = w(0)?;
    let first = first.trim_end_matches(',');

    match first {
        "today" | "tod" => return Some((today, 1)),
        "tomorrow" | "tmrw" | "tmr" => return Some((today.tomorrow().ok()?, 1)),
        "yesterday" | "yday" => return Some((today.yesterday().ok()?, 1)),
        _ => {}
    }
    if let Some(day) = weekday(first) {
        return Some((upcoming(today, day), 1));
    }
    if first == "next" {
        let second = w(1)?;
        // "next X" = X in the following Monday–Sunday week.
        let next_monday = upcoming(today.tomorrow().ok()?, Weekday::Monday);
        if let Some(day) = weekday(&second) {
            let offset = i64::from(day.to_monday_zero_offset());
            return Some((next_monday.checked_add(offset.days()).ok()?, 2));
        }
        if second == "week" {
            return Some((next_monday, 2));
        }
        return None;
    }
    if first == "in" {
        let n: i64 = w(1)?.parse().ok().filter(|n| (1..=999).contains(n))?;
        let unit = w(2)?;
        let date = match unit.trim_end_matches('s') {
            "day" | "d" => today.checked_add(n.days()).ok()?,
            "week" | "wk" | "w" => today.checked_add(n.weeks()).ok()?,
            "month" | "mo" => today.checked_add(n.months()).ok()?,
            _ => return None,
        };
        return Some((date, 3));
    }
    if let Ok(date) = first.parse::<Date>() {
        return Some((date, 1));
    }
    // "oct 20" / "october 20"
    if let Some(m) = month(first)
        && let Some(d) = w(1).and_then(|s| s.trim_end_matches(',').parse::<i8>().ok())
    {
        return day_month(today, m, d).map(|date| (date, 2));
    }
    // "20 oct" — unless a day follows the month ("round 2 oct 22" means Oct 22).
    if let Ok(d) = first.parse::<i8>()
        && let Some(m) = w(1).and_then(|s| month(&s))
        && !w(2).is_some_and(|s| s.trim_end_matches(',').parse::<i8>().is_ok())
    {
        return day_month(today, m, d).map(|date| (date, 2));
    }
    None
}

/// Parses `3pm`, `3:30pm`, `15:00`, `noon`, `midnight`.
fn parse_clock(s: &str) -> Option<Time> {
    let s = s.to_lowercase();
    match s.as_str() {
        "noon" => return Time::new(12, 0, 0, 0).ok(),
        "midnight" => return Time::new(0, 0, 0, 0).ok(),
        _ => {}
    }
    let (body, meridiem) = if let Some(b) = s.strip_suffix("am") {
        (b, Some(false))
    } else if let Some(b) = s.strip_suffix("pm") {
        (b, Some(true))
    } else {
        (s.as_str(), None)
    };
    let (h, m) = match body.split_once(':') {
        Some((h, m)) => (h.parse::<i8>().ok()?, m.parse::<i8>().ok()?),
        // A bare number is only a time with am/pm ("3" alone is not).
        None if meridiem.is_some() => (body.parse::<i8>().ok()?, 0),
        None => return None,
    };
    let h = match meridiem {
        Some(pm) => {
            if !(1..=12).contains(&h) {
                return None;
            }
            (h % 12) + if pm { 12 } else { 0 }
        }
        None => h,
    };
    Time::new(h, m, 0, 0).ok()
}

/// `3-4pm`, `3pm-4pm`, `15:00-16:30`, `9:30-11am`. Am/pm on the end applies
/// to the start too, unless that would put the start after the end.
fn parse_range(s: &str) -> Option<(Time, Time)> {
    let (a, b) = s.split_once(['-', '–'])?;
    let end = parse_clock(b)?;
    let start = parse_clock(a).or_else(|| {
        let suffix = if b.to_lowercase().ends_with("pm") {
            "pm"
        } else if b.to_lowercase().ends_with("am") {
            "am"
        } else {
            ""
        };
        if suffix.is_empty() {
            return None;
        }
        let with = parse_clock(&format!("{a}{suffix}"))?;
        if with < end {
            Some(with)
        } else {
            // "11-1pm" → 11am-1pm
            parse_clock(&format!("{a}am"))
        }
    })?;
    (start < end).then_some((start, end))
}

/// `~30m`, `~2h`, `~1h30m`, `~1.5h`.
fn parse_estimate(s: &str) -> Option<i64> {
    let body = s.strip_prefix('~')?.to_lowercase();
    if let Some(h) = body.strip_suffix('h')
        && let Ok(hours) = h.parse::<f64>()
    {
        let minutes = (hours * 60.0).round();
        return (minutes > 0.0 && minutes < 100_000.0).then_some(minutes as i64);
    }
    let (hours, rest) = match body.split_once('h') {
        Some((h, rest)) => (h.parse::<i64>().ok()?, rest),
        None => (0, body.as_str()),
    };
    let minutes = match rest.strip_suffix('m').or(rest.strip_suffix("min")) {
        Some(m) if !m.is_empty() => m.parse::<i64>().ok()?,
        _ if rest.is_empty() => 0,
        _ => return None,
    };
    let total = hours * 60 + minutes;
    (total > 0).then_some(total)
}

fn priority(s: &str) -> Option<Priority> {
    Some(match s.to_lowercase().as_str() {
        "!low" | "!1" => Priority::Low,
        "!med" | "!medium" | "!2" => Priority::Med,
        "!high" | "!3" | "!!" => Priority::High,
        "!urgent" | "!4" | "!!!" => Priority::Urgent,
        _ => return None,
    })
}

fn fmt_time(t: Time) -> String {
    format!("{:02}:{:02}", t.hour(), t.minute())
}

pub fn parse(input: &str, today: Date) -> ParsedQuickAdd {
    let ws = words(input);
    let mut out = ParsedQuickAdd::default();
    let mut used = vec![false; ws.len()];
    let mark = |out: &mut ParsedQuickAdd, used: &mut [bool], kind, from: usize, n: usize| {
        for u in used.iter_mut().skip(from).take(n) {
            *u = true;
        }
        if let (Some(a), Some(b)) = (ws.get(from), ws.get(from + n - 1)) {
            out.tokens.push(Token {
                kind,
                start: a.start,
                end: b.end,
            });
        }
    };

    let mut i = 0;
    while i < ws.len() {
        let word = &ws[i];
        if word.quoted {
            i += 1;
            continue;
        }
        let text = word.text;

        if let Some(tag) = text
            .strip_prefix('#')
            .filter(|t| !t.is_empty() && !t.starts_with('#'))
        {
            out.tags.push(tag.to_owned());
            mark(&mut out, &mut used, TokenKind::Tag, i, 1);
        } else if out.priority.is_none()
            && let Some(p) = priority(text)
        {
            out.priority = Some(p);
            mark(&mut out, &mut used, TokenKind::Priority, i, 1);
        } else if out.project_hint.is_none() && text.starts_with('@') {
            // `@Acme`, `@Website-redesign`, or `@"Website redesign"`.
            let (hint, n) = match ws.get(i + 1) {
                Some(next) if text == "@" && next.quoted => {
                    (next.text.trim_matches('"').to_owned(), 2)
                }
                _ if text.starts_with("@\"") => (text[1..].trim_matches('"').to_owned(), 1),
                _ => (text.trim_start_matches('@').replace(['-', '_'], " "), 1),
            };
            if !hint.trim().is_empty() {
                out.project_hint = Some(hint.trim().to_owned());
                mark(&mut out, &mut used, TokenKind::Project, i, n);
            }
        } else if out.estimate_minutes.is_none()
            && let Some(m) = parse_estimate(text)
        {
            out.estimate_minutes = Some(m);
            mark(&mut out, &mut used, TokenKind::Estimate, i, 1);
        } else if out.due_time.is_none()
            && let Some((start, end)) = parse_range(text)
        {
            out.due_time = Some(fmt_time(start));
            out.end_time = Some(fmt_time(end));
            mark(&mut out, &mut used, TokenKind::TimeRange, i, 1);
        } else if out.due_time.is_none()
            && text.eq_ignore_ascii_case("at")
            && let Some(t) = ws
                .get(i + 1)
                .filter(|w| !w.quoted)
                .and_then(|w| parse_clock(w.text))
        {
            out.due_time = Some(fmt_time(t));
            mark(&mut out, &mut used, TokenKind::Time, i, 2);
            i += 2;
            continue;
        } else if out.due_time.is_none()
            && let Some(t) = parse_clock(text)
        {
            out.due_time = Some(fmt_time(t));
            mark(&mut out, &mut used, TokenKind::Time, i, 1);
        } else if out.due_date.is_none()
            && let Some((date, n)) = parse_date(&ws, i, today)
        {
            out.due_date = Some(date.to_string());
            mark(&mut out, &mut used, TokenKind::Date, i, n);
            i += n;
            continue;
        }
        i += 1;
    }

    // A time without a date means today.
    if out.due_time.is_some() && out.due_date.is_none() {
        out.due_date = Some(today.to_string());
    }
    out.title = ws
        .iter()
        .zip(&used)
        .filter(|(_, used)| !**used)
        .map(|(w, _)| {
            if w.quoted {
                w.text.trim_matches('"')
            } else {
                w.text
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    out.tokens.sort_by_key(|t| t.start);
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use jiff::civil::date;

    // Thursday.
    const TODAY: Date = date(2026, 10, 15);

    fn p(s: &str) -> ParsedQuickAdd {
        parse(s, TODAY)
    }

    #[test]
    fn the_spec_example() {
        let r = p("Write report tomorrow 3pm #work !high");
        assert_eq!(r.title, "Write report");
        assert_eq!(r.due_date.as_deref(), Some("2026-10-16"));
        assert_eq!(r.due_time.as_deref(), Some("15:00"));
        assert_eq!(r.tags, ["work"]);
        assert_eq!(r.priority, Some(Priority::High));
        assert_eq!(r.tokens.len(), 4);
    }

    #[test]
    fn project_estimate_and_quoted_project() {
        let r = p("Fix login @Mobile-app ~1h30m");
        assert_eq!(r.project_hint.as_deref(), Some("Mobile app"));
        assert_eq!(r.estimate_minutes, Some(90));
        assert_eq!(r.title, "Fix login");
        let r = p("Plan sprint @ \"Website redesign\" ~1.5h");
        assert_eq!(r.project_hint.as_deref(), Some("Website redesign"));
        assert_eq!(r.estimate_minutes, Some(90));
    }

    #[test]
    fn weekdays_and_relative_dates() {
        assert_eq!(
            p("x thursday").due_date.as_deref(),
            Some("2026-10-15"),
            "today counts"
        );
        assert_eq!(p("x fri").due_date.as_deref(), Some("2026-10-16"));
        assert_eq!(p("x mon").due_date.as_deref(), Some("2026-10-19"));
        assert_eq!(p("x next fri").due_date.as_deref(), Some("2026-10-23"));
        assert_eq!(p("x next mon").due_date.as_deref(), Some("2026-10-19"));
        assert_eq!(p("x next week").due_date.as_deref(), Some("2026-10-19"));
        assert_eq!(p("x in 3 days").due_date.as_deref(), Some("2026-10-18"));
        assert_eq!(p("x in 2 weeks").due_date.as_deref(), Some("2026-10-29"));
        assert_eq!(p("x in 1 month").due_date.as_deref(), Some("2026-11-15"));
    }

    #[test]
    fn calendar_dates() {
        assert_eq!(p("x oct 20").due_date.as_deref(), Some("2026-10-20"));
        assert_eq!(p("x 20 october").due_date.as_deref(), Some("2026-10-20"));
        assert_eq!(
            p("x jan 5").due_date.as_deref(),
            Some("2027-01-05"),
            "past → next year"
        );
        assert_eq!(p("x 2026-12-31").due_date.as_deref(), Some("2026-12-31"));
        assert_eq!(p("x feb 30").due_date, None);
        let r = p("Logo round 2 oct 22");
        assert_eq!(
            (r.title.as_str(), r.due_date.as_deref()),
            ("Logo round 2", Some("2026-10-22"))
        );
    }

    #[test]
    fn times_and_ranges() {
        assert_eq!(p("x at 9am").due_time.as_deref(), Some("09:00"));
        assert_eq!(p("x 12am").due_time.as_deref(), Some("00:00"));
        assert_eq!(p("x noon").due_time.as_deref(), Some("12:00"));
        assert_eq!(p("x 15:30").due_time.as_deref(), Some("15:30"));
        let r = p("Acme sync tomorrow 3-4pm");
        assert_eq!(
            (r.due_time.as_deref(), r.end_time.as_deref()),
            (Some("15:00"), Some("16:00"))
        );
        assert_eq!(r.title, "Acme sync");
        let r = p("x 11-1pm");
        assert_eq!(
            (r.due_time.as_deref(), r.end_time.as_deref()),
            (Some("11:00"), Some("13:00"))
        );
        assert_eq!(p("x 9:30-11am").due_time.as_deref(), Some("09:30"));
        assert_eq!(
            p("call at 3pm").due_date.as_deref(),
            Some("2026-10-15"),
            "time alone → today"
        );
    }

    #[test]
    fn plain_words_and_numbers_stay_in_the_title() {
        let r = p("Buy 3 apples for 10 people");
        assert_eq!(r.title, "Buy 3 apples for 10 people");
        assert!(r.due_date.is_none() && r.due_time.is_none());
        let r = p("Read chapter 12 in the book");
        assert_eq!(r.title, "Read chapter 12 in the book");
    }

    #[test]
    fn quotes_keep_words_literal_and_only_first_date_counts() {
        let r = p("\"Monday\" standup notes friday");
        assert_eq!(r.title, "Monday standup notes");
        assert_eq!(r.due_date.as_deref(), Some("2026-10-16"));
        let r = p("move monday meeting to tuesday");
        assert_eq!(r.due_date.as_deref(), Some("2026-10-19"));
        assert_eq!(r.title, "move meeting to tuesday");
    }

    #[test]
    fn token_offsets_point_at_the_input() {
        let s = "Ship it #release !!";
        let r = p(s);
        let spans: Vec<&str> = r.tokens.iter().map(|t| &s[t.start..t.end]).collect();
        assert_eq!(spans, ["#release", "!!"]);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod fuzz {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Any input parses without panicking, and every token span is a valid
        /// slice of the input.
        #[test]
        fn never_panics(s in r"\PC{0,60}") {
            let r = parse(&s, jiff::civil::date(2026, 10, 15));
            for t in &r.tokens {
                prop_assert!(t.start <= t.end && t.end <= s.len());
                prop_assert!(s.get(t.start..t.end).is_some());
            }
        }

        #[test]
        fn never_panics_on_keyword_soup(
            words in prop::collection::vec(prop::sample::select(vec![
                "next", "in", "at", "3", "12", "-", "3-", "-4pm", "pm", "~", "~h", "~0m", "@", "\"",
                "#", "!", "oct", "31", "feb", "29", "mon", "week", "days", "noon", "25:61",
            ]), 0..12)
        ) {
            let s = words.join(" ");
            let r = parse(&s, jiff::civil::date(2026, 10, 15));
            for t in &r.tokens {
                prop_assert!(s.get(t.start..t.end).is_some());
            }
        }
    }
}
