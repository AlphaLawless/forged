use crate::config::CommitType;

const CONVENTIONAL_TYPES: &str = r#"{
  "feat": "A new feature",
  "fix": "A bug fix",
  "docs": "Documentation only changes",
  "style": "Changes that do not affect the meaning of the code (white-space, formatting, etc)",
  "refactor": "A code change that improves structure without changing functionality",
  "perf": "A code change that improves performance",
  "test": "Adding missing tests or correcting existing tests",
  "build": "Changes that affect the build system or external dependencies",
  "ci": "Changes to CI configuration files and scripts",
  "chore": "Other changes that don't modify src or test files",
  "revert": "Reverts a previous commit"
}"#;

const GITMOJI_TYPES: &str = r#"{
  "🎨": "Improve structure / format of the code",
  "⚡": "Improve performance",
  "🔥": "Remove code or files",
  "🐛": "Fix a bug",
  "🚑": "Critical hotfix",
  "✨": "Introduce new features",
  "📝": "Add or update documentation",
  "🚀": "Deploy stuff",
  "💄": "Add or update the UI and style files",
  "✅": "Add, update, or pass tests",
  "🔒": "Fix security or privacy issues",
  "🚨": "Fix compiler / linter warnings",
  "🚧": "Work in progress",
  "💚": "Fix CI Build",
  "⬇️": "Downgrade dependencies",
  "⬆️": "Upgrade dependencies",
  "👷": "Add or update CI build system",
  "♻️": "Refactor code",
  "➕": "Add a dependency",
  "➖": "Remove a dependency",
  "🔧": "Add or update configuration files",
  "🌐": "Internationalization and localization",
  "✏️": "Fix typos",
  "⏪": "Revert changes",
  "📦": "Add or update compiled files or packages",
  "🚚": "Move or rename resources",
  "💥": "Introduce breaking changes",
  "♿": "Improve accessibility",
  "💡": "Add or update comments in source code",
  "🗃": "Perform database related changes",
  "🔊": "Add or update logs",
  "🏗": "Make architectural changes",
  "🤡": "Mock things",
  "🏷": "Add or update types",
  "🩹": "Simple fix for a non-critical issue",
  "⚰": "Remove dead code",
  "👔": "Add or update business logic"
}"#;

fn commit_type_instruction(commit_type: &CommitType) -> &'static str {
    match commit_type {
        CommitType::Plain => "",
        CommitType::Conventional => {
            "Choose a type from the type-to-description JSON below that best describes the git diff. IMPORTANT: The type MUST be lowercase (e.g., \"feat\", not \"Feat\" or \"FEAT\"):"
        }
        CommitType::Gitmoji => {
            "Choose an emoji from the emoji-to-description JSON below that best describes the git diff:"
        }
        CommitType::SubjectBody => {
            "Output only the subject line; the body is generated separately."
        }
    }
}

fn commit_type_data(commit_type: &CommitType) -> &'static str {
    match commit_type {
        CommitType::Conventional => CONVENTIONAL_TYPES,
        CommitType::Gitmoji => GITMOJI_TYPES,
        _ => "",
    }
}

fn commit_type_format(commit_type: &CommitType) -> &'static str {
    match commit_type {
        CommitType::Plain => "The output response must be in format:\n<commit message>",
        CommitType::Conventional => {
            "The output response must be in format:\n<type>[optional (<scope>)]: <commit message>\nThe commit message subject must start with a lowercase letter"
        }
        CommitType::Gitmoji => "The output response must be in format:\n:emoji: <commit message>",
        CommitType::SubjectBody => {
            "The output response must be in format:\n<commit message subject>"
        }
    }
}

pub fn build_system_prompt(
    locale: &str,
    max_length: u32,
    commit_type: &CommitType,
    custom_prompt: Option<&str>,
) -> String {
    let parts: Vec<&str> = [
        Some("Generate a concise git commit message title in present tense that precisely describes the key changes in the following code diff. Focus on what was changed, not just file names. Provide only the title, no description or body."),
        Some("Be specific: include concrete details (package names, versions, functionality) rather than generic statements."),
        custom_prompt,
        Some(commit_type_instruction(commit_type)),
        {
            let data = commit_type_data(commit_type);
            if data.is_empty() { None } else { Some(data) }
        },
        Some(commit_type_format(commit_type)),
    ]
    .into_iter()
    .flatten()
    .filter(|s| !s.is_empty())
    .collect();

    format!(
        "{}\nMessage language: {locale}\nCommit message must be a maximum of {max_length} characters.\nExclude anything unnecessary. Your entire response will be passed directly into git commit.\nIMPORTANT: Respond with ONLY the commit message text. No explanations, no quotes, no formatting. Must not exceed {max_length} characters.",
        parts.join("\n")
    )
}

pub fn build_description_prompt(
    locale: &str,
    max_length: u32,
    custom_prompt: Option<&str>,
) -> String {
    let mut parts = vec![
        "You are generating the short body (description) of a git commit message. You are given the commit title and the code diff.",
        "Output must be brief: use 3-6 bullet points (one short line each), or 2-4 short sentences. No long paragraphs. Focus on what changed and why, in present tense.",
        "Do not repeat the title. No meta-commentary. Respond with ONLY the commit body.",
    ];

    if let Some(p) = custom_prompt
        && !p.is_empty()
    {
        parts.push(p);
    }

    format!(
        "{}\nGit convention: each line at most {max_length} characters.\nMessage language: {locale}",
        parts.join("\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_contains_locale() {
        let prompt = build_system_prompt("pt-br", 72, &CommitType::Plain, None);
        assert!(prompt.contains("pt-br"));
    }

    #[test]
    fn test_prompt_contains_max_length() {
        let prompt = build_system_prompt("en", 50, &CommitType::Plain, None);
        assert!(prompt.contains("50 characters"));
    }

    #[test]
    fn test_conventional_prompt_contains_feat_and_fix() {
        let prompt = build_system_prompt("en", 72, &CommitType::Conventional, None);
        assert!(prompt.contains("\"feat\""));
        assert!(prompt.contains("\"fix\""));
        assert!(prompt.contains("lowercase"));
    }

    #[test]
    fn test_gitmoji_prompt_contains_emoji_descriptions() {
        let prompt = build_system_prompt("en", 72, &CommitType::Gitmoji, None);
        assert!(prompt.contains("🐛"));
        assert!(prompt.contains("✨"));
        assert!(prompt.contains(":emoji:"));
    }

    #[test]
    fn test_custom_prompt_appended_to_base() {
        let prompt = build_system_prompt(
            "en",
            72,
            &CommitType::Plain,
            Some("Always mention the ticket number"),
        );
        assert!(prompt.contains("Always mention the ticket number"));
        assert!(prompt.contains("concise git commit"));
    }

    #[test]
    fn test_subject_body_prompt_says_subject_only() {
        let prompt = build_system_prompt("en", 72, &CommitType::SubjectBody, None);
        assert!(prompt.contains("subject line"));
        assert!(prompt.contains("body is generated separately"));
    }

    #[test]
    fn test_prompt_filters_empty_sections() {
        let prompt = build_system_prompt("en", 72, &CommitType::Plain, None);
        // Plain type has empty instruction and empty data, should not have blank lines from those
        assert!(!prompt.contains("\n\n\n"));
    }

    #[test]
    fn test_description_prompt_contains_locale() {
        let prompt = build_description_prompt("ja", 72, None);
        assert!(prompt.contains("ja"));
    }

    #[test]
    fn test_description_prompt_with_custom() {
        let prompt = build_description_prompt("en", 72, Some("Focus on security changes"));
        assert!(prompt.contains("Focus on security changes"));
    }
}
