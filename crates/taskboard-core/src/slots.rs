//! Interval arithmetic for the slot finder (pure, property-tested).
//! Intervals are half-open `[start, end)` in UTC milliseconds.

pub type Span = (i64, i64);

/// Sorts and merges overlapping or touching intervals; drops empty ones.
pub fn normalize(mut spans: Vec<Span>) -> Vec<Span> {
    spans.retain(|(a, b)| b > a);
    spans.sort_unstable();
    let mut out: Vec<Span> = Vec::with_capacity(spans.len());
    for (a, b) in spans {
        match out.last_mut() {
            Some(last) if a <= last.1 => last.1 = last.1.max(b),
            _ => out.push((a, b)),
        }
    }
    out
}

/// `available` minus `busy`.
pub fn subtract(available: &[Span], busy: &[Span]) -> Vec<Span> {
    let busy = normalize(busy.to_vec());
    let mut out = Vec::new();
    for &(start, end) in &normalize(available.to_vec()) {
        let mut cursor = start;
        for &(b0, b1) in busy.iter().filter(|(b0, b1)| *b1 > start && *b0 < end) {
            if b0 > cursor {
                out.push((cursor, b0.min(end)));
            }
            cursor = cursor.max(b1);
            if cursor >= end {
                break;
            }
        }
        if cursor < end {
            out.push((cursor, end));
        }
    }
    out
}

/// Intersection of two interval sets.
pub fn intersect(a: &[Span], b: &[Span]) -> Vec<Span> {
    let (a, b) = (normalize(a.to_vec()), normalize(b.to_vec()));
    let mut out = Vec::new();
    for &(a0, a1) in &a {
        for &(b0, b1) in &b {
            let (s, e) = (a0.max(b0), a1.min(b1));
            if e > s {
                out.push((s, e));
            }
        }
    }
    normalize(out)
}

/// Rounds `t` up to the next multiple of `step` measured from `origin`.
pub fn round_up(t: i64, origin: i64, step: i64) -> i64 {
    if step <= 0 {
        return t;
    }
    let off = (t - origin).rem_euclid(step);
    if off == 0 { t } else { t + (step - off) }
}

/// Up to two suggested slots of `duration` per free window: one at the start
/// (aligned to `step` from `origin`), one ending at the window's end when the
/// window is long enough, so a long free block isn't split in the middle.
pub fn suggest(free: &[Span], duration: i64, step: i64, origin: i64, not_before: i64) -> Vec<Span> {
    let mut out = Vec::new();
    for &(w0, w1) in free {
        let first = round_up(w0.max(not_before), origin, step);
        if first + duration > w1 {
            continue;
        }
        out.push((first, first + duration));
        let last_start = w1 - duration;
        let aligned_last = last_start - (last_start - origin).rem_euclid(step.max(1));
        if aligned_last >= first + duration {
            out.push((aligned_last, aligned_last + duration));
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn examples() {
        assert_eq!(
            normalize(vec![(5, 7), (1, 3), (2, 4), (7, 8)]),
            vec![(1, 4), (5, 8)]
        );
        assert_eq!(
            subtract(&[(0, 100)], &[(10, 20), (50, 60)]),
            vec![(0, 10), (20, 50), (60, 100)]
        );
        assert_eq!(subtract(&[(0, 100)], &[(-5, 200)]), vec![]);
        assert_eq!(
            intersect(&[(0, 50), (60, 100)], &[(40, 70)]),
            vec![(40, 50), (60, 70)]
        );
        assert_eq!(round_up(7, 0, 15), 15);
        assert_eq!(round_up(30, 0, 15), 30);
        assert_eq!(suggest(&[(0, 120)], 30, 15, 0, 0), vec![(0, 30), (90, 120)]);
        assert_eq!(
            suggest(&[(5, 40)], 30, 15, 0, 0),
            vec![],
            "aligned start doesn't fit"
        );
        assert_eq!(suggest(&[(0, 100)], 30, 15, 0, 50), vec![(60, 90)]);
    }

    fn spans() -> impl Strategy<Value = Vec<Span>> {
        prop::collection::vec(
            (0i64..1000, 1i64..200).prop_map(|(a, len)| (a, a + len)),
            0..12,
        )
    }

    fn covers(set: &[Span], t: i64) -> bool {
        set.iter().any(|&(a, b)| a <= t && t < b)
    }

    proptest! {
        #[test]
        fn free_time_is_available_and_never_busy(available in spans(), busy in spans()) {
            let free = subtract(&available, &busy);
            prop_assert!(free.windows(2).all(|w| w[0].1 < w[1].0 || w[0].1 <= w[1].0));
            prop_assert!(free.iter().all(|(a, b)| a < b));
            for t in (0..1200).step_by(7) {
                let expect = covers(&available, t) && !covers(&busy, t);
                prop_assert_eq!(covers(&free, t), expect, "t = {}", t);
            }
        }

        #[test]
        fn suggestions_fit_inside_free_windows(free in spans(), duration in 5i64..120, step in 1i64..60, not_before in 0i64..500) {
            let free = normalize(free);
            for (s, e) in suggest(&free, duration, step, 0, not_before) {
                prop_assert_eq!(e - s, duration);
                prop_assert!(s >= not_before);
                prop_assert!(free.iter().any(|&(a, b)| a <= s && e <= b));
            }
        }
    }
}
