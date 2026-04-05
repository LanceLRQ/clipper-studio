/// Format milliseconds to HH:MM:SS.mmm string
pub fn format_duration_ms(ms: i64) -> String {
    let total_seconds = ms / 1000;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    let millis = ms % 1000;

    if hours > 0 {
        format!("{:02}:{:02}:{:02}.{:03}", hours, minutes, seconds, millis)
    } else {
        format!("{:02}:{:02}.{:03}", minutes, seconds, millis)
    }
}

/// Parse HH:MM:SS.mmm or MM:SS.mmm string to milliseconds
pub fn parse_duration_ms(s: &str) -> Option<i64> {
    let parts: Vec<&str> = s.split(':').collect();
    match parts.len() {
        2 => {
            // MM:SS.mmm
            let minutes: i64 = parts[0].parse().ok()?;
            let sec_parts: Vec<&str> = parts[1].split('.').collect();
            let seconds: i64 = sec_parts[0].parse().ok()?;
            let millis: i64 = if sec_parts.len() > 1 {
                let ms_str = sec_parts[1];
                let padded = format!("{:0<3}", &ms_str[..ms_str.len().min(3)]);
                padded.parse().ok()?
            } else {
                0
            };
            Some(minutes * 60_000 + seconds * 1000 + millis)
        }
        3 => {
            // HH:MM:SS.mmm
            let hours: i64 = parts[0].parse().ok()?;
            let minutes: i64 = parts[1].parse().ok()?;
            let sec_parts: Vec<&str> = parts[2].split('.').collect();
            let seconds: i64 = sec_parts[0].parse().ok()?;
            let millis: i64 = if sec_parts.len() > 1 {
                let ms_str = sec_parts[1];
                let padded = format!("{:0<3}", &ms_str[..ms_str.len().min(3)]);
                padded.parse().ok()?
            } else {
                0
            };
            Some(hours * 3_600_000 + minutes * 60_000 + seconds * 1000 + millis)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration_ms() {
        assert_eq!(format_duration_ms(0), "00:00.000");
        assert_eq!(format_duration_ms(1500), "00:01.500");
        assert_eq!(format_duration_ms(90_000), "01:30.000");
        assert_eq!(format_duration_ms(3_661_500), "01:01:01.500");
    }

    #[test]
    fn test_parse_duration_ms() {
        assert_eq!(parse_duration_ms("00:01.500"), Some(1500));
        assert_eq!(parse_duration_ms("01:30.000"), Some(90_000));
        assert_eq!(parse_duration_ms("01:01:01.500"), Some(3_661_500));
        assert_eq!(parse_duration_ms("invalid"), None);
    }

    #[test]
    fn test_roundtrip() {
        let ms = 5_432_100;
        let formatted = format_duration_ms(ms);
        let parsed = parse_duration_ms(&formatted).unwrap();
        assert_eq!(ms, parsed);
    }
}
