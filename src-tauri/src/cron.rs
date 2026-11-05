//! Crontab schedules for project processes: the five classic fields (minute, hour, day of
//! month, month, day of week) with `*`, numbers, lists, ranges and steps. Names (MON, JAN)
//! and the `@daily` shortcuts aren't supported; the UI offers presets instead.

use time::OffsetDateTime;

/// One field's allowed values, as a bit per value.
#[derive(Debug, Clone, Copy)]
struct Field {
    bits: u64,
    /// Written as `*` (matters for the day-of-month / day-of-week rule).
    any: bool,
}

const LIMITS: [(&str, u8, u8); 5] = [("minute", 0, 59), ("hour", 0, 23), ("day of the month", 1, 31), ("month", 1, 12), ("day of the week", 0, 7)];

fn field(text: &str, name: &str, lo: u8, hi: u8) -> Result<Field, String> {
    let bad = || format!("The {name} \"{text}\" isn't valid: use *, a number from {lo} to {hi}, a list (1,15), a range (1-5) or a step (*/10).");
    let mut bits = 0u64;
    for part in text.split(',') {
        let (range, step) = match part.split_once('/') {
            Some((r, s)) => (r, s.parse::<u8>().ok().filter(|s| *s > 0).ok_or_else(bad)?),
            None => (part, 1),
        };
        let (from, to) = if range == "*" {
            (lo, hi)
        } else if let Some((a, b)) = range.split_once('-') {
            (a.parse().map_err(|_| bad())?, b.parse().map_err(|_| bad())?)
        } else {
            let n: u8 = range.parse().map_err(|_| bad())?;
            // "5/15" means from 5 to the end, every 15.
            (n, if step > 1 { hi } else { n })
        };
        if from < lo || to > hi || from > to {
            return Err(bad());
        }
        let mut v = from;
        while v <= to {
            bits |= 1 << v;
            v = match v.checked_add(step) {
                Some(n) => n,
                None => break,
            };
        }
    }
    Ok(Field { bits, any: text == "*" })
}

/// A parsed schedule.
#[derive(Debug, Clone, Copy)]
pub struct Schedule([Field; 5]);

impl Schedule {
    pub fn parse(expr: &str) -> Result<Self, String> {
        let parts: Vec<&str> = expr.split_whitespace().collect();
        if parts.len() != 5 {
            return Err(format!("\"{expr}\" needs five fields: minute hour day month weekday, like 0 3 * * * for 3:00 every night."));
        }
        let mut fields = [Field { bits: 0, any: true }; 5];
        for (i, (name, lo, hi)) in LIMITS.iter().enumerate() {
            fields[i] = field(parts[i], name, *lo, *hi)?;
        }
        // Sunday is 0 and 7.
        if fields[4].bits & (1 << 7) != 0 {
            fields[4].bits |= 1;
        }
        Ok(Self(fields))
    }

    /// Whether the schedule fires in the minute `at` falls in.
    pub fn matches(&self, at: OffsetDateTime) -> bool {
        let has = |f: Field, v: u8| f.bits & (1 << v) != 0;
        let [minute, hour, day, month, weekday] = self.0;
        // Like cron: with both day fields restricted, either one matching is enough.
        let day_ok = match (day.any, weekday.any) {
            (true, true) => true,
            (false, true) => has(day, at.day()),
            (true, false) => has(weekday, at.weekday().number_days_from_sunday()),
            (false, false) => has(day, at.day()) || has(weekday, at.weekday().number_days_from_sunday()),
        };
        has(minute, at.minute()) && has(hour, at.hour()) && has(month, at.month() as u8) && day_ok
    }
}

impl Schedule {
    /// The first minute after `after` the schedule fires, within a year.
    pub fn next(&self, after: OffsetDateTime) -> Option<OffsetDateTime> {
        let start = after.replace_second(0).ok()?.replace_nanosecond(0).ok()? + time::Duration::MINUTE;
        (0..366 * 24 * 60).map(|m| start + time::Duration::minutes(m)).find(|t| self.matches(*t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn parses_and_matches() {
        let at = datetime!(2026-11-05 03:00 UTC); // a Thursday
        assert!(Schedule::parse("* * * * *").unwrap().matches(at));
        assert!(Schedule::parse("0 3 * * *").unwrap().matches(at));
        assert!(!Schedule::parse("0 4 * * *").unwrap().matches(at));
        assert!(Schedule::parse("*/15 * * * *").unwrap().matches(at));
        assert!(!Schedule::parse("*/15 * * * *").unwrap().matches(datetime!(2026-11-05 03:07 UTC)));
        assert!(Schedule::parse("0 3 * * 1-5").unwrap().matches(at));
        assert!(!Schedule::parse("0 3 * * 0,6").unwrap().matches(at));
        assert!(Schedule::parse("0 0 * * 7").unwrap().matches(datetime!(2026-11-08 00:00 UTC))); // Sunday
        // Day of month or day of week, as in cron.
        assert!(Schedule::parse("0 3 1 * 4").unwrap().matches(at));
        assert!(Schedule::parse("5/20 * * * *").unwrap().matches(datetime!(2026-11-05 03:45 UTC)));
    }

    #[test]
    fn finds_the_next_run() {
        let s = Schedule::parse("0 3 * * *").unwrap();
        assert_eq!(s.next(datetime!(2026-11-05 02:59:30 UTC)), Some(datetime!(2026-11-05 03:00 UTC)));
        assert_eq!(s.next(datetime!(2026-11-05 03:00 UTC)), Some(datetime!(2026-11-06 03:00 UTC)));
        assert_eq!(Schedule::parse("0 0 30 2 *").unwrap().next(datetime!(2026-11-05 00:00 UTC)), None);
    }

    #[test]
    fn rejects_bad_schedules() {
        for bad in ["", "* * * *", "60 * * * *", "* 24 * * *", "* * 0 * *", "*/0 * * * *", "5-1 * * * *", "a * * * *", "* * * * 8"] {
            assert!(Schedule::parse(bad).is_err(), "{bad} should be refused");
        }
    }
}
