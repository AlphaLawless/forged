use regex::Regex;
use std::sync::LazyLock;

static THINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<think>.*?</think>").unwrap());
static LEADING_TAG_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^<[^>]*>\s*").unwrap());

/// Sanitize a single-line commit title.
/// Strips reasoning blocks, takes first line, removes trailing dot,
/// surrounding quotes, and leading HTML-like tags.
pub fn sanitize_title(msg: &str) -> String {
    let cleaned = THINK_RE.replace_all(msg, "");
    let trimmed = cleaned.trim();

    // Take first line only
    let first_line = trimmed.lines().next().unwrap_or("").trim();

    // Remove trailing dot after a word char
    let without_dot = if first_line.ends_with('.') && first_line.len() > 1 {
        let chars: Vec<char> = first_line.chars().collect();
        let second_last = chars.get(chars.len() - 2);
        if second_last.is_some_and(|c| c.is_alphanumeric()) {
            &first_line[..first_line.len() - 1]
        } else {
            first_line
        }
    } else {
        first_line
    };

    // Remove surrounding quotes
    let without_quotes = strip_surrounding_quotes(without_dot);

    // Remove leading HTML-like tag
    let result = LEADING_TAG_RE.replace(without_quotes, "");

    result.trim().to_string()
}

/// Sanitize a multi-line description. Strips reasoning blocks,
/// surrounding quotes, and leading tags, but keeps multiple lines.
pub fn sanitize_description(msg: &str) -> String {
    let cleaned = THINK_RE.replace_all(msg, "");
    let trimmed = cleaned.trim();
    let without_quotes = strip_surrounding_quotes(trimmed);
    let result = LEADING_TAG_RE.replace(without_quotes, "");
    result.trim().to_string()
}

fn strip_surrounding_quotes(s: &str) -> &str {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"')
            || (first == b'\'' && last == b'\'')
            || (first == b'`' && last == b'`')
        {
            return &s[1..s.len() - 1];
        }
    }
    s
}

/// Remove exact duplicates while preserving order.
pub fn deduplicate(msgs: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    msgs.into_iter()
        .filter(|m| seen.insert(m.clone()))
        .collect()
}

/// Wrap a single line at max_len by breaking on spaces.
/// Lines starting with "- " or "* " get continuation lines indented with 2 spaces.
pub fn wrap_line(line: &str, max_len: usize) -> String {
    if line.len() <= max_len {
        return line.to_string();
    }

    let is_bullet = line.starts_with("- ") || line.starts_with("* ");
    let indent = if is_bullet { "  " } else { "" };
    let continuation_max = max_len - indent.len();

    let mut parts: Vec<String> = Vec::new();
    let mut rest = line.to_string();
    let mut is_first = true;

    while rest.len() > if is_first { max_len } else { continuation_max } {
        let max_this = if is_first { max_len } else { continuation_max };
        let chunk = &rest[..max_this.min(rest.len())];
        let split_at = match chunk.rfind(' ') {
            Some(pos) if pos > 0 => pos + 1,
            _ => max_this,
        };
        let segment = rest[..split_at].trim_end();
        if is_first {
            parts.push(segment.to_string());
        } else {
            parts.push(format!("{indent}{segment}"));
        }
        rest = rest[split_at..].trim_start().to_string();
        is_first = false;
    }
    if !rest.is_empty() {
        if is_first {
            parts.push(rest);
        } else {
            parts.push(format!("{indent}{rest}"));
        }
    }

    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_removes_think_block_single_line() {
        let msg = "<think>reasoning here</think>feat: add login";
        assert_eq!(sanitize_title(msg), "feat: add login");
    }

    #[test]
    fn test_sanitize_removes_think_block_multiline() {
        let msg = "<think>\nstep 1\nstep 2\n</think>\nfeat: add login";
        assert_eq!(sanitize_title(msg), "feat: add login");
    }

    #[test]
    fn test_sanitize_removes_think_block_multiple_occurrences() {
        let msg = "<think>a</think>fix: bug<think>b</think>";
        assert_eq!(sanitize_title(msg), "fix: bug");
    }

    #[test]
    fn test_sanitize_takes_only_first_line() {
        let msg = "first line\nsecond line\nthird line";
        assert_eq!(sanitize_title(msg), "first line");
    }

    #[test]
    fn test_sanitize_removes_trailing_dot() {
        assert_eq!(sanitize_title("feat: add login."), "feat: add login");
    }

    #[test]
    fn test_sanitize_preserves_trailing_dot_after_non_word() {
        // e.g. "v1.0..." should keep the dot if preceded by dot
        assert_eq!(sanitize_title("fix: something..."), "fix: something...");
    }

    #[test]
    fn test_sanitize_removes_surrounding_double_quotes() {
        assert_eq!(sanitize_title("\"feat: add login\""), "feat: add login");
    }

    #[test]
    fn test_sanitize_removes_surrounding_single_quotes() {
        assert_eq!(sanitize_title("'feat: add login'"), "feat: add login");
    }

    #[test]
    fn test_sanitize_removes_leading_html_tag() {
        assert_eq!(
            sanitize_title("<output> feat: add login"),
            "feat: add login"
        );
    }

    #[test]
    fn test_sanitize_preserves_normal_message() {
        assert_eq!(sanitize_title("feat: add login"), "feat: add login");
    }

    #[test]
    fn test_deduplicate_removes_exact_duplicates() {
        let msgs = vec!["a".into(), "b".into(), "a".into(), "c".into()];
        assert_eq!(deduplicate(msgs), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_deduplicate_preserves_order() {
        let msgs = vec!["c".into(), "a".into(), "b".into()];
        assert_eq!(deduplicate(msgs), vec!["c", "a", "b"]);
    }

    #[test]
    fn test_wrap_line_short_line_unchanged() {
        assert_eq!(wrap_line("short line", 72), "short line");
    }

    #[test]
    fn test_wrap_line_breaks_on_space() {
        let line = "this is a very long line that should be wrapped at the word boundary";
        let wrapped = wrap_line(line, 40);
        for part in wrapped.lines() {
            assert!(part.len() <= 40, "line too long: {part}");
        }
        // Verify content is preserved (ignoring whitespace differences)
        let rejoined: String = wrapped
            .lines()
            .map(|l| l.trim())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(rejoined, line);
    }

    #[test]
    fn test_wrap_line_bullet_continuation_indented() {
        let line = "- this is a bullet point that is quite long and needs to be wrapped to fit";
        let wrapped = wrap_line(line, 40);
        let lines: Vec<&str> = wrapped.lines().collect();
        assert!(lines.len() > 1);
        // Continuation lines should start with "  "
        for continuation in &lines[1..] {
            assert!(
                continuation.starts_with("  "),
                "expected indent: {continuation}"
            );
        }
    }

    #[test]
    fn test_wrap_line_no_space_forces_hard_break() {
        let line = "abcdefghijklmnopqrstuvwxyz0123456789";
        let wrapped = wrap_line(line, 10);
        assert!(wrapped.contains('\n'));
    }

    #[test]
    fn test_sanitize_description_keeps_multiple_lines() {
        let msg = "<think>reasoning</think>\n- line one\n- line two";
        let result = sanitize_description(msg);
        assert!(result.contains("line one"));
        assert!(result.contains("line two"));
    }
}
