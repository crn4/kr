use chrono::Utc;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::Time;

pub fn get_resource_age(timestamp: Option<&Time>) -> String {
    match timestamp {
        Some(time) => {
            let now = Utc::now();
            let duration = now.signed_duration_since(time.0);

            if duration.num_days() > 0 {
                format!("{}d", duration.num_days())
            } else if duration.num_hours() > 0 {
                format!("{}h", duration.num_hours())
            } else if duration.num_minutes() > 0 {
                format!("{}m", duration.num_minutes())
            } else {
                format!("{}s", duration.num_seconds())
            }
        }
        None => "?".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn time_ago(duration: Duration) -> Time {
        Time(Utc::now() - duration)
    }

    #[test]
    fn age_none_returns_question_mark() {
        assert_eq!(get_resource_age(None), "?");
    }

    #[test]
    fn age_seconds() {
        let t = time_ago(Duration::seconds(45));
        assert_eq!(get_resource_age(Some(&t)), "45s");
    }

    #[test]
    fn age_minutes() {
        let t = time_ago(Duration::minutes(7));
        assert_eq!(get_resource_age(Some(&t)), "7m");
    }

    #[test]
    fn age_hours() {
        let t = time_ago(Duration::hours(3));
        assert_eq!(get_resource_age(Some(&t)), "3h");
    }

    #[test]
    fn age_days() {
        let t = time_ago(Duration::days(5));
        assert_eq!(get_resource_age(Some(&t)), "5d");
    }

    #[test]
    fn age_zero_seconds() {
        let t = time_ago(Duration::seconds(0));
        assert_eq!(get_resource_age(Some(&t)), "0s");
    }

    #[test]
    fn age_boundary_59_minutes() {
        let t = time_ago(Duration::minutes(59));
        assert_eq!(get_resource_age(Some(&t)), "59m");
    }

    #[test]
    fn age_boundary_60_minutes_shows_hours() {
        let t = time_ago(Duration::minutes(60));
        assert_eq!(get_resource_age(Some(&t)), "1h");
    }

    #[test]
    fn age_boundary_24_hours_shows_days() {
        let t = time_ago(Duration::hours(24));
        assert_eq!(get_resource_age(Some(&t)), "1d");
    }
}
