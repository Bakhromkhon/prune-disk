use std::path::Path;

/// Escape untrusted text at the human/terminal rendering boundary.
/// Keep JSON and the underlying model untouched.
pub fn escape_terminal(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            ch if ch.is_control()
                || matches!(ch, '\u{061c}' | '\u{200e}' | '\u{200f}'
                    | '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}') =>
            {
                use std::fmt::Write;
                let _ = write!(output, "\\u{{{:x}}}", ch as u32);
            }
            ch => output.push(ch),
        }
    }
    output
}

/// Render a path with the user's home directory shortened to `~`.
pub fn display_path(path: &Path) -> String {
    if let Some(home) = dirs::home_dir()
        && let Ok(rest) = path.strip_prefix(&home)
    {
        if rest.as_os_str().is_empty() {
            return "~".to_string();
        }
        return escape_terminal(&format!("~/{}", rest.display()));
    }
    escape_terminal(&path.display().to_string())
}

/// Truncate a string to at most `max` characters, adding `...` when needed.
pub fn truncate(input: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if input.chars().count() <= max {
        return input.to_string();
    }
    if max <= 3 {
        return input.chars().take(max).collect();
    }
    let mut output: String = input.chars().take(max - 3).collect();
    output.push_str("...");
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncates_long_strings() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 8), "hello...");
        assert_eq!(truncate("hello", 3), "hel");
    }

    #[test]
    fn escapes_terminal_controls_and_bidi_but_preserves_unicode() {
        assert_eq!(
            escape_terminal("é文\x1b[31m\n\r\t\0\u{7f}\u{85}\u{202e}\u{2066}"),
            "é文\\u{1b}[31m\\n\\r\\t\\u{0}\\u{7f}\\u{85}\\u{202e}\\u{2066}"
        );
        for ch in (0..=0x9f)
            .filter_map(char::from_u32)
            .filter(|ch| ch.is_control())
        {
            assert!(
                !escape_terminal(&ch.to_string())
                    .chars()
                    .any(char::is_control)
            );
        }
    }

    #[test]
    fn display_path_escapes_untrusted_names() {
        assert_eq!(
            display_path(Path::new("/cache/a\n\x1b")),
            "/cache/a\\n\\u{1b}"
        );
    }

    #[test]
    fn literal_escapes_are_distinct_from_controls() {
        assert_ne!(escape_terminal("a\nb"), escape_terminal(r"a\nb"));
        assert_ne!(escape_terminal("\u{202e}"), escape_terminal(r"\u{202e}"));
    }
}
