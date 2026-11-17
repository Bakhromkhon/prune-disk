/// Parse a human-friendly size such as `100MB`, `1.5GiB` or `4096`.
pub fn parse_size(input: &str) -> Result<u64, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("size is empty".to_string());
    }

    let lower = input.to_ascii_lowercase();
    let (number, multiplier) = if let Some(value) = lower.strip_suffix("tib") {
        (value, 1024_u64.pow(4))
    } else if let Some(value) = lower.strip_suffix("tb") {
        (value, 1_000_000_000_000)
    } else if let Some(value) = lower.strip_suffix("gib") {
        (value, 1024_u64.pow(3))
    } else if let Some(value) = lower.strip_suffix("gb") {
        (value, 1_000_000_000)
    } else if let Some(value) = lower.strip_suffix("mib") {
        (value, 1024_u64.pow(2))
    } else if let Some(value) = lower.strip_suffix("mb") {
        (value, 1_000_000)
    } else if let Some(value) = lower.strip_suffix("kib") {
        (value, 1024)
    } else if let Some(value) = lower.strip_suffix("kb") {
        (value, 1_000)
    } else if let Some(value) = lower.strip_suffix('b') {
        (value, 1)
    } else {
        (lower.as_str(), 1)
    };

    let number = number.trim();
    let value: f64 = number
        .parse()
        .map_err(|_| format!("invalid size: {input}"))?;
    if !value.is_finite() || value < 0.0 {
        return Err(format!("invalid size: {input}"));
    }

    Ok((value * multiplier as f64).round() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_sizes() {
        assert_eq!(parse_size("0").unwrap(), 0);
        assert_eq!(parse_size("4096").unwrap(), 4096);
        assert_eq!(parse_size("1KB").unwrap(), 1_000);
        assert_eq!(parse_size("1KiB").unwrap(), 1_024);
        assert_eq!(parse_size("1.5MB").unwrap(), 1_500_000);
        assert_eq!(parse_size("2GB").unwrap(), 2_000_000_000);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_size("").is_err());
        assert!(parse_size("many").is_err());
        assert!(parse_size("-1MB").is_err());
    }
}
