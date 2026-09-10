use jiff::Timestamp;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::Time;

pub fn format_duration_short(secs: u64) -> String {
    if secs >= 86400 {
        format!("{}d", secs / 86400)
    } else if secs >= 3600 {
        format!("{}h", secs / 3600)
    } else if secs >= 60 {
        format!("{}m", secs / 60)
    } else {
        format!("{secs}s")
    }
}

pub fn resource_age_at(now: Timestamp, timestamp: Option<&Time>) -> String {
    match timestamp {
        Some(time) => {
            let duration = now.duration_since(time.0);
            format_duration_short(duration.as_secs().max(0) as u64)
        }
        None => "?".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::SignedDuration;

    #[test]
    fn resource_age_at_uses_the_supplied_clock() {
        let now = Timestamp::UNIX_EPOCH + SignedDuration::from_secs(7200);
        let created = Time(Timestamp::UNIX_EPOCH + SignedDuration::from_secs(3600));
        assert_eq!(resource_age_at(now, Some(&created)), "1h");
        assert_eq!(resource_age_at(now, None), "?");
    }

    #[test]
    fn a_resource_created_in_the_future_reads_as_zero() {
        let now = Timestamp::UNIX_EPOCH;
        let created = Time(Timestamp::UNIX_EPOCH + SignedDuration::from_secs(30));
        assert_eq!(resource_age_at(now, Some(&created)), "0s");
    }

    const NOW: Timestamp = Timestamp::constant(1_757_000_000, 0);

    fn time_ago(duration: SignedDuration) -> Time {
        Time(NOW - duration)
    }

    #[test]
    fn age_none_returns_question_mark() {
        assert_eq!(resource_age_at(NOW, None), "?");
    }

    #[test]
    fn age_seconds() {
        let t = time_ago(SignedDuration::from_secs(45));
        assert_eq!(resource_age_at(NOW, Some(&t)), "45s");
    }

    #[test]
    fn age_minutes() {
        let t = time_ago(SignedDuration::from_mins(7));
        assert_eq!(resource_age_at(NOW, Some(&t)), "7m");
    }

    #[test]
    fn age_hours() {
        let t = time_ago(SignedDuration::from_hours(3));
        assert_eq!(resource_age_at(NOW, Some(&t)), "3h");
    }

    #[test]
    fn age_days() {
        let t = time_ago(SignedDuration::from_hours(5 * 24));
        assert_eq!(resource_age_at(NOW, Some(&t)), "5d");
    }

    #[test]
    fn age_zero_seconds() {
        let t = time_ago(SignedDuration::from_secs(0));
        assert_eq!(resource_age_at(NOW, Some(&t)), "0s");
    }

    #[test]
    fn age_boundary_59_minutes() {
        let t = time_ago(SignedDuration::from_mins(59));
        assert_eq!(resource_age_at(NOW, Some(&t)), "59m");
    }

    #[test]
    fn age_boundary_60_minutes_shows_hours() {
        let t = time_ago(SignedDuration::from_mins(60));
        assert_eq!(resource_age_at(NOW, Some(&t)), "1h");
    }

    #[test]
    fn age_boundary_24_hours_shows_days() {
        let t = time_ago(SignedDuration::from_hours(24));
        assert_eq!(resource_age_at(NOW, Some(&t)), "1d");
    }
}
