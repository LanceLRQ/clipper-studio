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

    // ==================== format_duration_ms ====================

    #[test]
    fn test_format_zero() {
        assert_eq!(format_duration_ms(0), "00:00.000");
    }

    #[test]
    fn test_format_milliseconds_only() {
        assert_eq!(format_duration_ms(500), "00:00.500");
        assert_eq!(format_duration_ms(1), "00:00.001");
        assert_eq!(format_duration_ms(999), "00:00.999");
    }

    #[test]
    fn test_format_seconds() {
        assert_eq!(format_duration_ms(1_000), "00:01.000");
        assert_eq!(format_duration_ms(1_500), "00:01.500");
        assert_eq!(format_duration_ms(59_999), "00:59.999");
    }

    #[test]
    fn test_format_minutes() {
        assert_eq!(format_duration_ms(60_000), "01:00.000");
        assert_eq!(format_duration_ms(90_000), "01:30.000");
    }

    #[test]
    fn test_format_hours() {
        assert_eq!(format_duration_ms(3_600_000), "01:00:00.000");
        assert_eq!(format_duration_ms(3_661_500), "01:01:01.500");
        assert_eq!(format_duration_ms(86_399_999), "23:59:59.999");
    }

    #[test]
    fn test_format_large_values() {
        assert_eq!(format_duration_ms(360_000_000), "100:00:00.000");
        assert_eq!(format_duration_ms(86_400_000), "24:00:00.000");
    }

    #[test]
    fn test_format_negative_values() {
        // Negative values are not expected in normal usage,
        // but verify the function does not panic
        // -1: total_seconds=-1/1000=0, ms=-1%1000=-1
        assert_eq!(format_duration_ms(-1), "00:00.-01");
        // -1500: total_seconds=-1, minutes=0, seconds=-1, ms=-500
        assert_eq!(format_duration_ms(-1500), "00:-1.-500");
    }

    // ==================== parse_duration_ms ====================

    #[test]
    fn test_parse_mmss_mmm() {
        assert_eq!(parse_duration_ms("00:00.000"), Some(0));
        assert_eq!(parse_duration_ms("00:01.500"), Some(1500));
        assert_eq!(parse_duration_ms("01:30.000"), Some(90_000));
        assert_eq!(parse_duration_ms("59:59.999"), Some(3_599_999));
    }

    #[test]
    fn test_parse_hhmmss_mmm() {
        assert_eq!(parse_duration_ms("01:01:01.500"), Some(3_661_500));
        assert_eq!(parse_duration_ms("01:00:00.000"), Some(3_600_000));
        assert_eq!(parse_duration_ms("23:59:59.999"), Some(86_399_999));
    }

    #[test]
    fn test_parse_without_millis() {
        // MM:SS without .mmm
        assert_eq!(parse_duration_ms("01:30"), Some(90_000));
        assert_eq!(parse_duration_ms("00:05"), Some(5000));
        // HH:MM:SS without .mmm
        assert_eq!(parse_duration_ms("01:00:00"), Some(3_600_000));
        assert_eq!(parse_duration_ms("00:01:30"), Some(90_000));
    }

    #[test]
    fn test_parse_short_millis_pads_right() {
        // "5" -> "500"
        assert_eq!(parse_duration_ms("00:01.5"), Some(1500));
        // "50" -> "500"
        assert_eq!(parse_duration_ms("00:01.50"), Some(1500));
        // "05" -> "050"
        assert_eq!(parse_duration_ms("00:01.05"), Some(1050));
    }

    #[test]
    fn test_parse_invalid_inputs() {
        assert_eq!(parse_duration_ms("invalid"), None);
        assert_eq!(parse_duration_ms(""), None);
        assert_eq!(parse_duration_ms("1:2:3:4"), None); // too many parts
        assert_eq!(parse_duration_ms("ab:cd.ef"), None);
    }

    #[test]
    fn test_parse_edge_cases() {
        // Single colon with no seconds
        assert_eq!(parse_duration_ms("00:"), None);
        // Empty parts
        assert_eq!(parse_duration_ms(":"), None);
    }

    // ==================== roundtrip ====================

    #[test]
    fn test_roundtrip_zero() {
        let ms = 0i64;
        let formatted = format_duration_ms(ms);
        let parsed = parse_duration_ms(&formatted).unwrap();
        assert_eq!(ms, parsed);
    }

    #[test]
    fn test_roundtrip_large() {
        let ms = 5_432_100i64;
        let formatted = format_duration_ms(ms);
        let parsed = parse_duration_ms(&formatted).unwrap();
        assert_eq!(ms, parsed);
    }

    #[test]
    fn test_roundtrip_various() {
        let test_values = [
            1i64,
            999,
            1_000,
            59_999,
            60_000,
            90_000,
            3_599_999,
            3_600_000,
            3_661_500,
            86_399_999,
            86_400_000,
            360_000_000,
        ];
        for ms in test_values {
            let formatted = format_duration_ms(ms);
            let parsed = parse_duration_ms(&formatted).unwrap();
            assert_eq!(
                ms, parsed,
                "roundtrip failed for {} (formatted as {})",
                ms, formatted
            );
        }
    }
}
