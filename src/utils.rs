use jiff::Timestamp;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::Time;

pub fn get_resource_age(timestamp: Option<&Time>) -> String {
    match timestamp {
        Some(time) => {
            let now = Timestamp::now();
            let duration = now.duration_since(time.0);
            let secs = duration.as_secs();

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
        None => "?".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::SignedDuration;

    fn time_ago(duration: SignedDuration) -> Time {
        Time(Timestamp::now() - duration)
    }

    #[test]
    fn age_none_returns_question_mark() {
        assert_eq!(get_resource_age(None), "?");
    }

    #[test]
    fn age_seconds() {
        let t = time_ago(SignedDuration::from_secs(45));
        assert_eq!(get_resource_age(Some(&t)), "45s");
    }

    #[test]
    fn age_minutes() {
        let t = time_ago(SignedDuration::from_mins(7));
        assert_eq!(get_resource_age(Some(&t)), "7m");
    }

    #[test]
    fn age_hours() {
        let t = time_ago(SignedDuration::from_hours(3));
        assert_eq!(get_resource_age(Some(&t)), "3h");
    }

    #[test]
    fn age_days() {
        let t = time_ago(SignedDuration::from_hours(5 * 24));
        assert_eq!(get_resource_age(Some(&t)), "5d");
    }

    #[test]
    fn age_zero_seconds() {
        let t = time_ago(SignedDuration::from_secs(0));
        assert_eq!(get_resource_age(Some(&t)), "0s");
    }

    #[test]
    fn age_boundary_59_minutes() {
        let t = time_ago(SignedDuration::from_mins(59));
        assert_eq!(get_resource_age(Some(&t)), "59m");
    }

    #[test]
    fn age_boundary_60_minutes_shows_hours() {
        let t = time_ago(SignedDuration::from_mins(60));
        assert_eq!(get_resource_age(Some(&t)), "1h");
    }

    #[test]
    fn age_boundary_24_hours_shows_days() {
        let t = time_ago(SignedDuration::from_hours(24));
        assert_eq!(get_resource_age(Some(&t)), "1d");
    }
}
