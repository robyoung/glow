use chrono::{offset::Utc, DateTime};

pub(crate) fn format_time_since(now: DateTime<Utc>, stamp: DateTime<Utc>) -> String {
    let duration = now.signed_duration_since(stamp);
    
    let mut parts = Vec::new();
    let num_minutes = duration.num_minutes();
    if num_minutes < 60 {
        let word = if num_minutes == 1 { "minute" } else { "minutes" };
        if num_minutes > 0 {
            parts.push(format!("{} {}", num_minutes, word));
        }

        let num_seconds = duration.num_seconds() - num_minutes * 60;
        if parts.is_empty() || num_seconds > 0 {
            let word = if num_seconds == 1 { "second" } else { "seconds" };

            parts.push(format!("{} {}", num_seconds, word));
        }
    } else if duration.num_days() > 0 {
        parts.push(if duration.num_days() == 1 {
            String::from("more than a day")
        } else {
            format!("more than {} days", duration.num_days())
        });
    } else if duration.num_hours() > 0 {
        parts.push(if duration.num_hours() == 1 {
            String::from("more than an hour")
        } else {
            format!("more than {} hours", duration.num_hours())
        });
    }

    parts.as_slice().join(", ")
}

#[cfg(test)]
mod tests {
    use chrono::Duration;
    use super::*;

    #[test]
    fn time_since_stamp_is_correctly_formatted() {

        let cases = [
            (12, "12 seconds"),
            (1212, "20 minutes, 12 seconds"),
            (12121, "more than 3 hours"),
            (121212, "more than a day"),
            (1212121, "more than 14 days"),
        ];
        for (seconds, formatted) in cases.iter() {
            let now = Utc::now();
            let then = now.checked_sub_signed(Duration::seconds(*seconds)).unwrap();

            assert_eq!(
                format_time_since(now, then),
                *formatted,
            );
        }
    }
}
