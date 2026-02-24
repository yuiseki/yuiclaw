use std::path::PathBuf;

/// Load `~/.config/yuiclaw/.env` if it exists.
///
/// Rules:
/// - Lines starting with `#` or blank lines are skipped.
/// - Each non-empty line must be in `KEY=VALUE` form.
/// - Leading/trailing whitespace around the key and value is trimmed.
/// - Single- or double-quoted values have their quotes stripped.
/// - Existing environment variables are **not** overridden (file provides defaults).
pub fn load_config_dotenv() {
    let env_path = match config_env_path() {
        Some(p) => p,
        None => return,
    };

    if !env_path.exists() {
        return;
    }

    let contents = match std::fs::read_to_string(&env_path) {
        Ok(s) => s,
        Err(_) => return,
    };

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        let value = strip_quotes(value.trim());
        // Only set if the variable is not already present in the environment.
        if std::env::var(key).is_err() {
            // SAFETY: single-threaded at this point (early in main, before tokio runtime).
            unsafe {
                std::env::set_var(key, value);
            }
        }
    }
}

/// Returns the path `~/.config/yuiclaw/.env`.
fn config_env_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("yuiclaw").join(".env"))
}

/// Strip a single layer of surrounding single or double quotes from a value.
fn strip_quotes(s: &str) -> &str {
    if (s.starts_with('"') && s.ends_with('"'))
        || (s.starts_with('\'') && s.ends_with('\''))
    {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use tempfile::NamedTempFile;
    use std::io::Write;

    fn parse_dotenv_str(contents: &str) -> Vec<(String, String)> {
        // Helper that parses the same logic against a string, returning key-value pairs.
        let mut result = Vec::new();
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                if !key.is_empty() {
                    result.push((key.to_string(), strip_quotes(value.trim()).to_string()));
                }
            }
        }
        result
    }

    #[test]
    fn parses_simple_key_value_pairs() {
        let pairs = parse_dotenv_str("FOO=bar\nBAZ=qux\n");
        assert_eq!(pairs, vec![
            ("FOO".to_string(), "bar".to_string()),
            ("BAZ".to_string(), "qux".to_string()),
        ]);
    }

    #[test]
    fn skips_comments_and_blank_lines() {
        let pairs = parse_dotenv_str("# comment\n\nKEY=value\n");
        assert_eq!(pairs, vec![("KEY".to_string(), "value".to_string())]);
    }

    #[test]
    fn strips_double_quotes() {
        let pairs = parse_dotenv_str(r#"TOKEN="abc123""#);
        assert_eq!(pairs, vec![("TOKEN".to_string(), "abc123".to_string())]);
    }

    #[test]
    fn strips_single_quotes() {
        let pairs = parse_dotenv_str("TOKEN='abc123'");
        assert_eq!(pairs, vec![("TOKEN".to_string(), "abc123".to_string())]);
    }

    #[test]
    fn trims_whitespace_around_key_and_value() {
        let pairs = parse_dotenv_str("  KEY  =  value  ");
        assert_eq!(pairs, vec![("KEY".to_string(), "value".to_string())]);
    }

    #[test]
    fn skips_lines_without_equals() {
        let pairs = parse_dotenv_str("NOEQUALS\nKEY=val\n");
        assert_eq!(pairs, vec![("KEY".to_string(), "val".to_string())]);
    }

    #[test]
    fn does_not_override_existing_env_var() {
        let key = "YUICLAW_TEST_NO_OVERRIDE_12345";
        unsafe { env::set_var(key, "original") };

        // Simulate writing a .env file and loading it
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "{}=overridden", key).unwrap();

        // Manually replicate load logic using the file path
        let contents = std::fs::read_to_string(f.path()).unwrap();
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') { continue; }
            if let Some((k, v)) = line.split_once('=') {
                let k = k.trim();
                if !k.is_empty() && env::var(k).is_err() {
                    unsafe { env::set_var(k, v.trim()); }
                }
            }
        }

        assert_eq!(env::var(key).unwrap(), "original");
        unsafe { env::remove_var(key) };
    }

    #[test]
    fn sets_env_var_when_not_already_set() {
        let key = "YUICLAW_TEST_SET_FROM_FILE_12345";
        unsafe { env::remove_var(key) };

        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "{}=from_file", key).unwrap();

        let contents = std::fs::read_to_string(f.path()).unwrap();
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') { continue; }
            if let Some((k, v)) = line.split_once('=') {
                let k = k.trim();
                if !k.is_empty() && env::var(k).is_err() {
                    unsafe { env::set_var(k, v.trim()); }
                }
            }
        }

        assert_eq!(env::var(key).unwrap(), "from_file");
        unsafe { env::remove_var(key) };
    }

    #[test]
    fn strip_quotes_double() {
        assert_eq!(strip_quotes(r#""hello""#), "hello");
    }

    #[test]
    fn strip_quotes_single() {
        assert_eq!(strip_quotes("'hello'"), "hello");
    }

    #[test]
    fn strip_quotes_no_quotes() {
        assert_eq!(strip_quotes("hello"), "hello");
    }

    #[test]
    fn strip_quotes_mismatched_not_stripped() {
        assert_eq!(strip_quotes(r#""hello'"#), r#""hello'"#);
    }
}
