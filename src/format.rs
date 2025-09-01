use std::time::Duration;

use chrono::Local;

const SECOND: u64 = 1;
const MINUTE: u64 = 60 * SECOND;
const HOUR: u64 = 60 * MINUTE;
const DAY: u64 = 24 * HOUR;
const YEAR: u64 = 365 * DAY;

pub fn format_time(time: u64) -> String {
    if time == 0 {
        return String::from("never");
    }
    let now = now();
    let (duration, style) = if now > time {
        (now.saturating_sub(time), "ago")
    } else {
        (time.saturating_sub(now), "left")
    };

    let unit: &str;
    let value: u64;
    if duration < MINUTE {
        unit = "s";
        if duration < 30 {
            return String::from("now");
        }
        value = duration;
    } else if duration < HOUR {
        unit = "m";
        value = duration / MINUTE;
    } else if duration < DAY {
        unit = "h";
        value = duration / HOUR;
    } else if duration < YEAR {
        unit = "d";
        value = duration / DAY;
    } else {
        unit = "y";
        value = duration / YEAR;
    }

    format!("{value}{unit} {style}")
}

/// Show elapsed time.
pub fn format_elapsed(d: Duration) -> String {
    let elapsed_time = d.as_secs_f64();

    if elapsed_time >= 3600.0 {
        let hours = elapsed_time / 3600.0;
        format!("{hours:.2}h")
    } else if elapsed_time >= 60.0 {
        let minutes = elapsed_time / 60.0;
        format!("{minutes:.2}min")
    } else if elapsed_time >= 1.0 {
        format!("{elapsed_time:.2}s")
    } else {
        let milliseconds = elapsed_time * 1000.0;
        format!("{milliseconds:.2}ms")
    }
}

pub fn now() -> u64 {
    Local::now().timestamp() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_time() {
        let current = now();
        assert_eq!(format_time(current - 5), "now");
        assert_eq!(format_time(current - 30), "30s ago");
        assert_eq!(format_time(current - 90), "1m ago");
        assert_eq!(format_time(current - 3600), "1h ago");
        assert_eq!(format_time(current - 86400), "1d ago");
        assert_eq!(format_time(current - 31536000), "1y ago");
        assert_eq!(format_time(current + 5), "now");
        assert_eq!(format_time(current + 30), "30s left");
        assert_eq!(format_time(current + 90), "1m left");
        assert_eq!(format_time(current + 3600), "1h left");
        assert_eq!(format_time(current + 86400), "1d left");
        assert_eq!(format_time(current + 31536000), "1y left");
        assert_eq!(format_time(0), "never");
    }

    #[test]
    fn test_format_elapsed() {
        assert_eq!(format_elapsed(Duration::from_millis(500)), "500.00ms");
        assert_eq!(format_elapsed(Duration::from_secs(5)), "5.00s");
        assert_eq!(format_elapsed(Duration::from_secs(65)), "1.08min");
        assert_eq!(format_elapsed(Duration::from_secs(3665)), "1.02h");
    }
}
