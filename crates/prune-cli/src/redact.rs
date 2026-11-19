use serde_json::Value;
use std::path::{Path, PathBuf};

/// Replace the current home path in every string value in a JSON tree.
pub fn redact_home_in_json(value: &mut Value, home: &Path) {
    if !home.is_absolute() {
        return;
    }
    let canonical_home = canonical_home(home);
    redact_json(value, home, canonical_home.as_deref());
}

fn redact_json(value: &mut Value, home: &Path, canonical_home: Option<&Path>) {
    match value {
        Value::String(text) => {
            let mut redacted = redact_home_in_string_for_path(text, home);
            if let Some(canonical_home) = canonical_home {
                redacted = redact_home_in_string_for_path(&redacted, canonical_home);
            }
            *text = redacted;
        }
        Value::Array(values) => {
            for value in values {
                redact_json(value, home, canonical_home);
            }
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                redact_json(value, home, canonical_home);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

/// Replace home-directory prefixes in arbitrary display text.
pub fn redact_home_in_string(text: &str, home: &Path) -> String {
    if !home.is_absolute() {
        return text.to_owned();
    }

    let mut redacted = redact_home_in_string_for_path(text, home);
    if let Some(canonical_home) = canonical_home(home) {
        redacted = redact_home_in_string_for_path(&redacted, &canonical_home);
    }
    redacted
}

fn canonical_home(home: &Path) -> Option<PathBuf> {
    home.canonicalize()
        .ok()
        .filter(|canonical_home| canonical_home.as_path() != home)
}

fn redact_home_in_string_for_path(text: &str, home: &Path) -> String {
    let home = home.to_string_lossy().into_owned();
    let home_variants = home_prefix_variants(&home);
    if home_variants.is_empty() {
        return text.to_owned();
    }

    let mut redacted = String::with_capacity(text.len());
    let mut offset = 0;

    while offset < text.len() {
        let matched = home_variants
            .iter()
            .filter_map(|candidate_home| match_home_prefix(&text[offset..], candidate_home))
            .max_by_key(|(matched_len, _)| *matched_len);
        if let Some((matched_len, trailing_separator)) = matched {
            redacted.push('~');
            let suffix = &text[offset + matched_len..];
            if let Some(separator) = trailing_separator
                && suffix
                    .chars()
                    .next()
                    .is_some_and(|character| !is_path_separator(character))
            {
                // Preserve the component separator if the home path itself
                // ended in one (notably when HOME is a filesystem root).
                redacted.push(separator);
            }
            offset += matched_len;
            continue;
        }

        let character = text[offset..]
            .chars()
            .next()
            .expect("offset is within the string");
        redacted.push(character);
        offset += character.len_utf8();
    }

    redacted
}

#[cfg(windows)]
fn home_prefix_variants(home: &str) -> Vec<String> {
    const VERBATIM: &str = "\\\\?\\";
    const VERBATIM_UNC: &str = "\\\\?\\UNC\\";

    let mut variants = vec![home.to_owned()];
    if home
        .get(..VERBATIM_UNC.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(VERBATIM_UNC))
    {
        variants.push(format!("\\\\{}", &home[VERBATIM_UNC.len()..]));
    } else if home
        .get(..VERBATIM.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(VERBATIM))
    {
        variants.push(home[VERBATIM.len()..].to_owned());
    }

    let bytes = home.as_bytes();
    if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && is_path_separator(home[2..].chars().next().unwrap_or_default())
    {
        variants.push(format!("{VERBATIM}{home}"));
    } else if home.starts_with("\\\\") && !home.starts_with(VERBATIM) {
        variants.push(format!("{VERBATIM_UNC}{}", &home[2..]));
    }

    variants.sort();
    variants.dedup();
    variants
}

#[cfg(not(windows))]
fn home_prefix_variants(home: &str) -> Vec<String> {
    vec![home.to_owned()]
}

fn match_home_prefix(candidate: &str, home: &str) -> Option<(usize, Option<char>)> {
    if home.is_empty() {
        return None;
    }

    let mut candidate_chars = candidate.char_indices();
    let mut matched_len = 0;
    let mut last_character = None;

    for home_character in home.chars() {
        let (offset, candidate_character) = candidate_chars.next()?;
        if !same_path_character(candidate_character, home_character) {
            return None;
        }

        matched_len = offset + candidate_character.len_utf8();
        last_character = Some(candidate_character);
    }

    let home_ends_with_separator = home.chars().last().is_some_and(is_path_separator);
    let suffix = &candidate[matched_len..];
    if !home_ends_with_separator
        && !suffix.is_empty()
        && !suffix.chars().next().is_some_and(is_path_separator)
        && !is_text_boundary(suffix)
    {
        return None;
    }

    Some((
        matched_len,
        home_ends_with_separator.then_some(last_character.expect("home is not empty")),
    ))
}

fn is_text_boundary(suffix: &str) -> bool {
    // Report text may punctuate a complete path; dots inside a filename and
    // other path-component characters are deliberately not boundaries.
    let mut characters = suffix.chars();
    let Some(character) = characters.next() else {
        return true;
    };

    if character.is_whitespace() || matches!(character, '"' | '\'' | '`' | ')' | ']' | '}') {
        return true;
    }

    if matches!(character, ',' | ';' | ':' | '!' | '?' | '.') {
        return characters.next().is_none_or(|next| {
            next.is_whitespace() || matches!(next, '"' | '\'' | '`' | ')' | ']' | '}')
        });
    }

    false
}

#[cfg(windows)]
fn is_path_separator(character: char) -> bool {
    matches!(character, '/' | '\\')
}

#[cfg(not(windows))]
fn is_path_separator(character: char) -> bool {
    character == '/'
}

#[cfg(any(windows, target_os = "macos"))]
fn same_path_character(left: char, right: char) -> bool {
    #[cfg(windows)]
    if is_path_separator(left) && is_path_separator(right) {
        return true;
    }

    if left.is_ascii() && right.is_ascii() {
        left.eq_ignore_ascii_case(&right)
    } else {
        left.to_lowercase().collect::<String>() == right.to_lowercase().collect::<String>()
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn same_path_character(left: char, right: char) -> bool {
    left == right
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[cfg(unix)]
    #[test]
    fn recursively_redacts_string_leaves_and_respects_component_boundaries() {
        let home = Path::new("/home/example");
        let mut value = json!({
            "errors": [{
                "path": "/home/example",
                "message": "failed to read /home/example/cache and /home/example: denied"
            }],
            "findings": [{
                "path": "/home/example/cache",
                "description": "under /home/example and /home/example2 and /home/example.txt"
            }],
            "case_mismatch": "/HOME/example/cache",
            "rules": [{
                "name": "rule in /home/example/cache"
            }]
        });

        redact_home_in_json(&mut value, home);

        assert_eq!(value["errors"][0]["path"], "~");
        assert_eq!(
            value["errors"][0]["message"],
            "failed to read ~/cache and ~: denied"
        );
        assert_eq!(value["findings"][0]["path"], "~/cache");
        assert_eq!(
            value["findings"][0]["description"],
            "under ~ and /home/example2 and /home/example.txt"
        );
        #[cfg(target_os = "macos")]
        assert_eq!(value["case_mismatch"], "~/cache");
        #[cfg(not(target_os = "macos"))]
        assert_eq!(value["case_mismatch"], "/HOME/example/cache");
        assert_eq!(value["rules"][0]["name"], "rule in ~/cache");
    }

    #[cfg(unix)]
    #[test]
    fn case_variant_prefix_matches_on_macos_but_preserves_component_boundaries() {
        let fixture = tempfile::tempdir().expect("temporary directory");
        let home = fixture.path().join("CaseRoot");
        let home_text = home.to_string_lossy();
        let case_variant = home_text
            .chars()
            .map(|character| {
                if character.is_ascii_lowercase() {
                    character.to_ascii_uppercase()
                } else if character.is_ascii_uppercase() {
                    character.to_ascii_lowercase()
                } else {
                    character
                }
            })
            .collect::<String>();
        assert!(
            case_variant.as_str() != home_text.as_ref(),
            "temporary path should contain ASCII letters"
        );

        let case_variant_child = format!("{case_variant}/cache");
        let sibling_path = format!("{case_variant}2/cache");
        let redacted = redact_home_in_string(&case_variant_child, &home);
        let sibling = redact_home_in_string(&sibling_path, &home);

        #[cfg(target_os = "macos")]
        assert!(
            redacted == "~/cache",
            "macOS path prefixes should ignore case"
        );
        #[cfg(not(target_os = "macos"))]
        assert!(
            redacted == case_variant_child,
            "case-sensitive Unix paths should preserve different casing"
        );
        assert!(
            sibling == sibling_path,
            "a longer component is not the home path"
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_paths_accept_both_separator_forms_and_case_variants() {
        let home = Path::new(r"C:\Users\Éxample");
        let mut value = json!({
            "native": r"C:\Users\Éxample\cache",
            "slash": "c:/users/éxample/cache",
            "unicode_case": "C:/USERS/ÉXAMPLE/cache",
            "near_match": r"C:\Users\Éxamples\cache"
        });

        redact_home_in_json(&mut value, home);

        assert_eq!(value["native"], r"~\cache");
        assert_eq!(value["slash"], "~/cache");
        assert_eq!(value["unicode_case"], "~/cache");
        assert_eq!(value["near_match"], r"C:\Users\Éxamples\cache");
    }

    #[cfg(windows)]
    #[test]
    fn windows_verbatim_drive_and_unc_spellings_are_redacted() {
        let drive_home = Path::new(r"C:\Users\Alice");
        assert_eq!(
            redact_home_in_string(r"\\?\C:\Users\Alice\cache\item", drive_home),
            r"~\cache\item"
        );

        let unc_home = Path::new(r"\\server\share\Alice");
        assert_eq!(
            redact_home_in_string(r"\\?\UNC\server\share\Alice\cache\item", unc_home),
            r"~\cache\item"
        );
    }
}
