//! Fractional indexing: string sort keys that allow inserting between any two
//! neighbours by writing one row.
//!
//! Port of David Greenspan's algorithm ("Implementing Fractional Indexing").
//! A key is an integer part (head char encodes its length, so appends stay
//! short: 62 appends per extra char at first, then 3,844, then 238,328) followed
//! by an optional fraction that never ends in '0'.

use crate::error::{Error, Result};

const DIGITS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const BASE: usize = 62;
const SMALLEST_INTEGER: &str = "A00000000000000000000000000";

fn digit_index(c: u8) -> Result<usize> {
    DIGITS
        .iter()
        .position(|&d| d == c)
        .ok_or_else(|| invalid(format!("bad digit {:?}", c as char)))
}

fn digit(i: usize) -> char {
    DIGITS.get(i).map_or('0', |&d| d as char)
}

fn invalid(msg: String) -> Error {
    Error::InvalidInput(format!("order key: {msg}"))
}

fn integer_length(head: u8) -> Result<usize> {
    match head {
        b'a'..=b'z' => Ok(usize::from(head - b'a') + 2),
        b'A'..=b'Z' => Ok(usize::from(b'Z' - head) + 2),
        _ => Err(invalid(format!("bad head {:?}", head as char))),
    }
}

fn split(key: &str) -> Result<(&str, &str)> {
    // Keys are ASCII; a byte split inside a multi-byte char would panic.
    if !key.is_ascii() {
        return Err(invalid(format!("bad key {key:?}")));
    }
    let head = *key
        .as_bytes()
        .first()
        .ok_or_else(|| invalid("empty".into()))?;
    let len = integer_length(head)?;
    if len > key.len() {
        return Err(invalid(format!("truncated key {key:?}")));
    }
    Ok(key.split_at(len))
}

/// Checks that `key` is a well-formed order key.
pub fn validate(key: &str) -> Result<()> {
    if key == SMALLEST_INTEGER {
        return Err(invalid("smallest integer is reserved".into()));
    }
    let (int, frac) = split(key)?;
    for &c in int.as_bytes().iter().skip(1).chain(frac.as_bytes()) {
        digit_index(c)?;
    }
    if frac.ends_with('0') {
        return Err(invalid(format!("fraction ends with 0: {key:?}")));
    }
    Ok(())
}

/// Midpoint between fractions `a` < `b` (`b = None` means +infinity).
fn midpoint(a: &str, b: Option<&str>) -> Result<String> {
    let (ab, bb) = (a.as_bytes(), b.map(str::as_bytes));
    if let Some(bb) = bb {
        let mut n = 0;
        while n < bb.len() && ab.get(n).copied().unwrap_or(b'0') == bb[n] {
            n += 1;
        }
        if n > 0 {
            let rest_a = a.get(n.min(a.len())..).unwrap_or("");
            let rest_b = b.and_then(|b| b.get(n..)).unwrap_or("");
            let prefix = b.and_then(|b| b.get(..n)).unwrap_or("");
            return Ok(format!("{prefix}{}", midpoint(rest_a, Some(rest_b))?));
        }
    }
    let digit_a = match ab.first() {
        Some(&c) => digit_index(c)?,
        None => 0,
    };
    let digit_b = match bb.and_then(|b| b.first()) {
        Some(&c) => digit_index(c)?,
        None => BASE,
    };
    if digit_b - digit_a > 1 {
        return Ok(digit((digit_a + digit_b).div_ceil(2)).to_string());
    }
    match b {
        Some(b) if b.len() > 1 => Ok(b.get(..1).unwrap_or("").to_owned()),
        _ => Ok(format!(
            "{}{}",
            digit(digit_a),
            midpoint(a.get(1.min(a.len())..).unwrap_or(""), None)?
        )),
    }
}

fn increment_integer(x: &str) -> Result<Option<String>> {
    let bytes = x.as_bytes();
    let head = bytes[0];
    let mut digs: Vec<u8> = bytes[1..].to_vec();
    let mut carry = true;
    for d in digs.iter_mut().rev() {
        if !carry {
            break;
        }
        let i = digit_index(*d)? + 1;
        if i == BASE {
            *d = b'0';
        } else {
            *d = DIGITS[i];
            carry = false;
        }
    }
    if carry {
        if head == b'Z' {
            return Ok(Some("a0".into()));
        }
        if head == b'z' {
            return Ok(None);
        }
        let new_head = head + 1;
        if new_head > b'a' {
            digs.push(b'0');
        } else {
            digs.pop();
        }
        digs.insert(0, new_head);
    } else {
        digs.insert(0, head);
    }
    Ok(Some(String::from_utf8_lossy(&digs).into_owned()))
}

fn decrement_integer(x: &str) -> Result<Option<String>> {
    let bytes = x.as_bytes();
    let head = bytes[0];
    let mut digs: Vec<u8> = bytes[1..].to_vec();
    let mut borrow = true;
    for d in digs.iter_mut().rev() {
        if !borrow {
            break;
        }
        let i = digit_index(*d)?;
        if i == 0 {
            *d = b'z';
        } else {
            *d = DIGITS[i - 1];
            borrow = false;
        }
    }
    if borrow {
        if head == b'a' {
            return Ok(Some("Zz".into()));
        }
        if head == b'A' {
            return Ok(None);
        }
        let new_head = head - 1;
        if new_head < b'Z' {
            digs.push(b'z');
        } else {
            digs.pop();
        }
        digs.insert(0, new_head);
    } else {
        digs.insert(0, head);
    }
    Ok(Some(String::from_utf8_lossy(&digs).into_owned()))
}

/// A key strictly between `a` and `b` (`None` = open end).
pub fn key_between(a: Option<&str>, b: Option<&str>) -> Result<String> {
    if let Some(a) = a {
        validate(a)?;
    }
    if let Some(b) = b {
        validate(b)?;
    }
    if let (Some(a), Some(b)) = (a, b)
        && a >= b
    {
        return Err(invalid(format!("{a:?} >= {b:?}")));
    }
    match (a, b) {
        (None, None) => Ok("a0".into()),
        (None, Some(b)) => {
            let (ib, fb) = split(b)?;
            if ib == SMALLEST_INTEGER {
                return Ok(format!("{ib}{}", midpoint("", Some(fb))?));
            }
            if !fb.is_empty() {
                return Ok(ib.to_owned());
            }
            decrement_integer(ib)?.ok_or_else(|| invalid("cannot go lower".into()))
        }
        (Some(a), None) => {
            let (ia, fa) = split(a)?;
            match increment_integer(ia)? {
                Some(i) => Ok(i),
                None => Ok(format!("{ia}{}", midpoint(fa, None)?)),
            }
        }
        (Some(a), Some(b)) => {
            let (ia, fa) = split(a)?;
            let (ib, fb) = split(b)?;
            if ia == ib {
                return Ok(format!("{ia}{}", midpoint(fa, Some(fb))?));
            }
            let i = increment_integer(ia)?.ok_or_else(|| invalid("cannot go higher".into()))?;
            if i.as_str() < b {
                Ok(i)
            } else {
                Ok(format!("{ia}{}", midpoint(fa, None)?))
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn known_values() {
        assert_eq!(key_between(None, None).unwrap(), "a0");
        assert_eq!(key_between(Some("a0"), None).unwrap(), "a1");
        assert_eq!(key_between(None, Some("a0")).unwrap(), "Zz");
        assert_eq!(key_between(Some("a0"), Some("a1")).unwrap(), "a0V");
        assert_eq!(key_between(Some("az"), None).unwrap(), "b00");
        assert!(key_between(Some("a1"), Some("a0")).is_err());
        assert!(key_between(Some("a0"), Some("a0")).is_err());
        assert!(validate("a10").is_err());
    }

    #[test]
    fn appends_stay_short() {
        let mut key = key_between(None, None).unwrap();
        for _ in 0..100_000 {
            key = key_between(Some(&key), None).unwrap();
        }
        assert!(key.len() <= 4, "key grew to {key:?}");
    }

    #[test]
    fn prepends_stay_short() {
        let mut key = key_between(None, None).unwrap();
        for _ in 0..100_000 {
            key = key_between(None, Some(&key)).unwrap();
        }
        assert!(key.len() <= 4, "key grew to {key:?}");
    }

    proptest! {
        /// Random insertions keep the list strictly sorted with valid keys.
        #[test]
        fn random_inserts_keep_order(positions in prop::collection::vec(0usize..1000, 1..300)) {
            let mut keys: Vec<String> = Vec::new();
            for p in positions {
                let at = p % (keys.len() + 1);
                let before = at.checked_sub(1).map(|i| keys[i].as_str());
                let after = keys.get(at).map(String::as_str);
                let key = key_between(before, after).unwrap();
                validate(&key).unwrap();
                if let Some(b) = before { prop_assert!(b < key.as_str()); }
                if let Some(a) = after { prop_assert!(key.as_str() < a); }
                keys.insert(at, key);
            }
            prop_assert!(keys.windows(2).all(|w| w[0] < w[1]));
        }
    }
}
