use anyhow::{Context, Result, bail};
use colored::Colorize;

use crate::ai;
use crate::ai::FailoverReport;
use crate::ai::provider::{
    GenerateOpts, generate_description_with_failover, generate_messages_with_failover,
};
use crate::clipboard;
use crate::config::CommitType;
use crate::config::Config;
use crate::git;
use crate::prompt;

pub struct CommitOpts {
    pub generate: Option<u8>,
    pub exclude_files: Vec<String>,
    pub stage_all: bool,
    pub commit_type: Option<String>,
    pub skip_confirm: bool,
    pub clipboard: bool,
    pub no_verify: bool,
    pub custom_prompt: Option<String>,
    pub hook_file: Option<String>,
    pub extra_args: Vec<String>,
}

pub async fn run(opts: CommitOpts) -> Result<()> {
    let headless = std::env::var("GIT_PARAMS").is_ok() || !atty::is(atty::Stream::Stdout);

    if !headless {
        println!("{}", " forged ".on_cyan().black());
    }

    // First-run setup
    let mut config = Config::load()?;
    if super::setup::needs_setup(&config) {
        if atty::is(atty::Stream::Stdin) {
            config = super::setup::run(Some(config))?;
        } else {
            bail!(
                "No configuration found. Run `forged` in an interactive terminal to set up, or manually configure with `forged config set`."
            );
        }
    }

    git::assert_git_repo()?;

    if opts.stage_all {
        std::process::Command::new("git")
            .args(["add", "--update"])
            .status()?;
    }

    // Better DX: if nothing staged, check for unstaged changes and offer to stage
    let staged = match git::staged_diff(&opts.exclude_files)? {
        Some(s) => s,
        None => {
            if headless {
                bail!(
                    "No staged changes found. Stage your changes manually, or use the --all flag."
                );
            }
            if !offer_stage_changes()? {
                return Ok(());
            }
            // Re-read staged diff after staging
            match git::staged_diff(&opts.exclude_files)? {
                Some(s) => s,
                None => bail!("No staged changes found after staging."),
            }
        }
    };

    if !headless {
        let file_count = staged.files.len();
        let label = if file_count == 1 { "file" } else { "files" };
        println!("{} Detected {} staged {}", "::".dimmed(), file_count, label);
    }

    let providers = ai::build_providers(&config)?;

    let mut session =
        SessionConfig::from_config(&config, opts.commit_type.as_deref(), opts.generate)?;

    let diff = git::truncate_diff(&staged.diff);

    let custom_prompt = opts.custom_prompt.as_deref();

    if !headless {
        println!(
            "{} Analyzing changes in {} file{}...",
            "::".dimmed(),
            staged.files.len(),
            if staged.files.len() == 1 { "" } else { "s" }
        );
    }

    let params = build_generation_params(&session, custom_prompt);

    let (messages, report) = generate_full_messages(
        &providers,
        &params.system,
        params.desc_system.as_deref(),
        &diff,
        &params.gen_opts,
    )
    .await?;

    if !headless {
        print_failover_report(&report);
    }

    // Hook mode: write message to file and exit
    if let Some(ref hook_file) = opts.hook_file {
        std::fs::write(hook_file, &messages[0])
            .with_context(|| format!("Failed to write hook file: {hook_file}"))?;
        return Ok(());
    }

    // Headless mode: output and exit
    if headless {
        println!("{}", messages[0]);
        return Ok(());
    }

    // Skip confirm: commit directly
    if opts.skip_confirm {
        let message = &messages[0];
        if opts.clipboard {
            return copy_to_clipboard(message);
        }
        return do_commit(message, opts.no_verify, &opts.extra_args);
    }

    // Interactive loop: select → action menu (commit / edit / regenerate / cancel)
    let mut current_messages = messages;
    loop {
        let message = pick_message(&current_messages)?;
        let Some(mut message) = message else {
            println!("Commit cancelled.");
            return Ok(());
        };

        match action_menu(&message, opts.clipboard)? {
            Action::Commit => {
                if opts.clipboard {
                    return copy_to_clipboard(&message);
                }
                return do_commit(&message, opts.no_verify, &opts.extra_args);
            }
            Action::Edit => {
                let edited = inquire::Editor::new("Edit commit message:")
                    .with_predefined_text(&message)
                    .prompt()?;
                message = edited.trim().to_string();
                if message.is_empty() {
                    println!("{}", "Empty message — cancelled.".dimmed());
                    return Ok(());
                }
                // Show edited message and loop back to action menu
                println!("\n{}\n", message.bold());
                match action_menu(&message, opts.clipboard)? {
                    Action::Commit => {
                        if opts.clipboard {
                            return copy_to_clipboard(&message);
                        }
                        return do_commit(&message, opts.no_verify, &opts.extra_args);
                    }
                    Action::Cancel => {
                        println!("Commit cancelled.");
                        return Ok(());
                    }
                    Action::Edit | Action::Regenerate | Action::Settings => {
                        // Let the outer loop handle regenerate, settings, or another edit
                        continue;
                    }
                }
            }
            Action::Regenerate => {
                let params = build_generation_params(&session, custom_prompt);
                println!("{} Regenerating...", "::".dimmed());
                let (msgs, report) = generate_full_messages(
                    &providers,
                    &params.system,
                    params.desc_system.as_deref(),
                    &diff,
                    &params.gen_opts,
                )
                .await?;
                print_failover_report(&report);
                current_messages = msgs;
                continue;
            }
            Action::Settings => {
                if settings_menu(&mut session)? {
                    let params = build_generation_params(&session, custom_prompt);
                    println!("{} Regenerating with new settings...", "::".dimmed());
                    let (msgs, report) = generate_full_messages(
                        &providers,
                        &params.system,
                        params.desc_system.as_deref(),
                        &diff,
                        &params.gen_opts,
                    )
                    .await?;
                    print_failover_report(&report);
                    current_messages = msgs;
                }
                continue;
            }
            Action::Cancel => {
                println!("Commit cancelled.");
                return Ok(());
            }
        }
    }
}

// --- Session config buffer ---

/// Ephemeral settings that can be changed during the interactive session.
/// Initialized from Config + CLI overrides, never persisted.
#[derive(Debug, Clone)]
struct SessionConfig {
    locale: String,
    commit_type: CommitType,
    max_length: u32,
    generate: u8,
}

impl SessionConfig {
    fn from_config(
        config: &Config,
        commit_type_override: Option<&str>,
        generate_override: Option<u8>,
    ) -> Result<Self> {
        let commit_type = if let Some(t) = commit_type_override {
            CommitType::from_str_loose(t)?
        } else {
            config.commit_type.clone()
        };
        Ok(Self {
            locale: config.locale.clone(),
            commit_type,
            max_length: config.max_length,
            generate: generate_override.unwrap_or(config.generate),
        })
    }
}

struct GenerationParams {
    system: String,
    desc_system: Option<String>,
    gen_opts: GenerateOpts,
}

fn build_generation_params(
    session: &SessionConfig,
    custom_prompt: Option<&str>,
) -> GenerationParams {
    let system = prompt::build_system_prompt(
        &session.locale,
        session.max_length,
        &session.commit_type,
        custom_prompt,
    );

    let desc_system = if session.commit_type == CommitType::SubjectBody {
        Some(prompt::build_description_prompt(
            &session.locale,
            session.max_length,
            custom_prompt,
        ))
    } else {
        None
    };

    // model and timeout are placeholders — overridden per-provider by failover
    let gen_opts = GenerateOpts {
        model: String::new(),
        temperature: 0.4,
        max_tokens: 2000,
        completions: session.generate,
        timeout_secs: 0,
    };

    GenerationParams {
        system,
        desc_system,
        gen_opts,
    }
}

fn print_failover_report(report: &FailoverReport) {
    if report.failures.is_empty() {
        println!(
            "{} Generated with {}",
            "::".dimmed(),
            report.used_model.dimmed()
        );
    } else {
        let failure_summary: String = report
            .failures
            .iter()
            .map(|f| format!("{} failed: {}", f.provider, f.reason))
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "{} Generated with {} {}",
            "::".dimmed(),
            report.used_model,
            format!("(fallback: {failure_summary})").dimmed()
        );
    }
}

// --- Subject+Body helpers ---

/// Generate commit messages with failover. When `desc_system` is Some (subject+body mode),
/// generates subjects first, then a body for each using the provider that succeeded.
async fn generate_full_messages(
    providers: &[ai::ProviderWithOpts],
    system: &str,
    desc_system: Option<&str>,
    diff: &str,
    opts: &GenerateOpts,
) -> Result<(Vec<String>, FailoverReport)> {
    let (subjects, report) =
        generate_messages_with_failover(providers, system, diff, opts).await?;

    if subjects.is_empty() {
        bail!("No commit messages were generated. Try again.");
    }

    let Some(desc_sys) = desc_system else {
        return Ok((subjects, report));
    };

    // 2-step: generate body for each subject, using the provider that succeeded
    let working = providers
        .iter()
        .find(|p| p.provider.name() == report.used_provider)
        .unwrap_or(&providers[0]);

    let desc_opts = GenerateOpts {
        model: working.model.clone(),
        timeout_secs: working.timeout,
        ..opts.clone()
    };

    let mut full_messages = Vec::with_capacity(subjects.len());
    for subject in &subjects {
        let (body, _) = generate_description_with_failover(
            std::slice::from_ref(working),
            desc_sys,
            subject,
            diff,
            &desc_opts,
        )
        .await?;
        full_messages.push(combine_subject_body(subject, &body));
    }
    Ok((full_messages, report))
}

fn combine_subject_body(subject: &str, body: &str) -> String {
    if body.is_empty() {
        return subject.to_string();
    }
    format!("{subject}\n\n{body}")
}

// --- Action menu ---

#[derive(Debug, Clone, PartialEq)]
enum Action {
    Commit,
    Edit,
    Regenerate,
    Settings,
    Cancel,
}

const ACTION_COMMIT: &str = "Commit this message";
const ACTION_EDIT: &str = "Edit message";
const ACTION_REGENERATE: &str = "Regenerate";
const ACTION_SETTINGS: &str = "Settings";
const ACTION_CANCEL: &str = "Cancel";

const ACTION_COPY: &str = "Copy to clipboard";

fn action_menu(message: &str, clipboard_mode: bool) -> Result<Action> {
    println!("\n{}\n", message.bold());

    let mut options = vec![];
    if clipboard_mode {
        options.push(ACTION_COPY);
    } else {
        options.push(ACTION_COMMIT);
    }
    options.extend([
        ACTION_EDIT,
        ACTION_REGENERATE,
        ACTION_SETTINGS,
        ACTION_CANCEL,
    ]);

    let choice = inquire::Select::new("What do you want to do?", options)
        .with_page_size(10)
        .prompt()?;

    match choice {
        ACTION_COMMIT | ACTION_COPY => Ok(Action::Commit),
        ACTION_EDIT => Ok(Action::Edit),
        ACTION_REGENERATE => Ok(Action::Regenerate),
        ACTION_SETTINGS => Ok(Action::Settings),
        ACTION_CANCEL => Ok(Action::Cancel),
        _ => Ok(Action::Cancel),
    }
}

// --- Settings menu ---

const COMMIT_TYPE_OPTIONS: &[(&str, &str)] = &[
    (
        "conventional",
        "conventional  — feat: / fix: / refactor: ...",
    ),
    ("plain", "plain         — free-form message"),
    ("gitmoji", "gitmoji       — :emoji: message"),
    (
        "subject+body",
        "subject+body  — title + detailed description",
    ),
];

const SETTING_LOCALE: &str = "locale";
const SETTING_TYPE: &str = "type";
const SETTING_MAX_LENGTH: &str = "max_length";
const SETTING_GENERATE: &str = "generate";
const SETTING_BACK: &str = "← Back";

/// Show session settings and let the user edit them interactively.
/// Returns `true` if any setting was changed.
fn settings_menu(session: &mut SessionConfig) -> Result<bool> {
    let mut changed = false;

    loop {
        println!();
        println!("  {}", "Session settings:".dimmed());
        println!("    locale     = {}", session.locale);
        println!("    type       = {}", session.commit_type.as_str());
        println!("    max_length = {}", session.max_length);
        println!("    generate   = {}", session.generate);
        println!();

        let options = vec![
            SETTING_LOCALE,
            SETTING_TYPE,
            SETTING_MAX_LENGTH,
            SETTING_GENERATE,
            SETTING_BACK,
        ];
        let choice = inquire::Select::new("Change setting:", options)
            .with_page_size(10)
            .prompt()?;

        match choice {
            SETTING_LOCALE => {
                let value = inquire::Text::new("Locale:")
                    .with_default(&session.locale)
                    .with_help_message("e.g. en, pt-br, ja, es")
                    .prompt()?;
                if value != session.locale {
                    session.locale = value;
                    changed = true;
                    println!("  {} locale → {}", "✔".green(), session.locale);
                }
            }
            SETTING_TYPE => {
                let labels: Vec<&str> = COMMIT_TYPE_OPTIONS.iter().map(|(_, l)| *l).collect();
                let current_idx = COMMIT_TYPE_OPTIONS
                    .iter()
                    .position(|(k, _)| *k == session.commit_type.as_str())
                    .unwrap_or(0);
                let selected = inquire::Select::new("Commit type:", labels)
                    .with_starting_cursor(current_idx)
                    .with_page_size(10)
                    .prompt()?;
                let key = COMMIT_TYPE_OPTIONS
                    .iter()
                    .find(|(_, l)| *l == selected)
                    .map(|(k, _)| *k)
                    .unwrap_or("conventional");
                let new_type = CommitType::from_str_loose(key)?;
                if new_type != session.commit_type {
                    session.commit_type = new_type;
                    changed = true;
                    println!("  {} type → {}", "✔".green(), session.commit_type.as_str());
                }
            }
            SETTING_MAX_LENGTH => {
                let value = inquire::Text::new("Max length:")
                    .with_default(&session.max_length.to_string())
                    .prompt()?;
                match value.trim().parse::<u32>() {
                    Ok(n) if n >= 20 => {
                        if n != session.max_length {
                            session.max_length = n;
                            changed = true;
                            println!("  {} max_length → {}", "✔".green(), n);
                        }
                    }
                    _ => {
                        println!("  {} Must be a number ≥ 20", "⚠".yellow());
                    }
                }
            }
            SETTING_GENERATE => {
                let value = inquire::Text::new("Generate count (1-5):")
                    .with_default(&session.generate.to_string())
                    .prompt()?;
                match value.trim().parse::<u8>() {
                    Ok(n) if (1..=5).contains(&n) => {
                        if n != session.generate {
                            session.generate = n;
                            changed = true;
                            println!("  {} generate → {}", "✔".green(), n);
                        }
                    }
                    _ => {
                        println!("  {} Must be between 1 and 5", "⚠".yellow());
                    }
                }
            }
            _ => break,
        }
    }

    Ok(changed)
}

/// Pick a message from multiple options, or return the single one directly.
fn pick_message(messages: &[String]) -> Result<Option<String>> {
    if messages.len() == 1 {
        return Ok(Some(messages[0].clone()));
    }

    let selection = inquire::Select::new("Pick a commit message:", messages.to_vec())
        .with_page_size(10)
        .prompt_skippable()?;
    Ok(selection)
}

// --- Helpers ---

fn do_commit(message: &str, no_verify: bool, extra_args: &[String]) -> Result<()> {
    match git::commit(message, no_verify, extra_args)? {
        git::CommitResult::Success => {
            println!("{} Successfully committed!", "✔".green());
            Ok(())
        }
        git::CommitResult::HookFailed => {
            println!(
                "{} Commit failed — pre-commit hook rejected the commit.",
                "✘".red()
            );
            println!(
                "  {} Use {} to bypass hooks.",
                "tip:".dimmed(),
                "--no-verify".bold()
            );
            std::process::exit(1);
        }
    }
}

fn copy_to_clipboard(message: &str) -> Result<()> {
    if clipboard::copy(message) {
        println!("{} Message copied to clipboard!", "✔".green());
    } else {
        println!("{} Could not copy to clipboard. Message:", "⚠".yellow());
        println!("\n{}\n", message);
    }
    Ok(())
}

/// Show unstaged changes and let the user pick which files to stage.
/// Returns true if at least one file was staged.
fn offer_stage_changes() -> Result<bool> {
    let changes = git::unstaged_changes().unwrap_or_default();

    if changes.is_empty() {
        println!("{} No changes found in this repository.", "⚠".yellow());
        println!();
        println!("  Nothing to commit — working tree is clean.");
        return Ok(false);
    }

    println!("{} No staged changes found.\n", "⚠".yellow());

    // Build labels: "[M] src/main.rs"
    let labels: Vec<String> = changes
        .iter()
        .map(|f| {
            let tag = match f.status.as_str() {
                "modified" => "[M]",
                "new file" => "[N]",
                "deleted" => "[D]",
                _ => "[?]",
            };
            format!("{} {}", tag, f.path)
        })
        .collect();

    let selected = inquire::MultiSelect::new("Stage files:", labels.clone())
        .with_all_selected_by_default()
        .with_page_size(20)
        .prompt()?;

    if selected.is_empty() {
        println!();
        println!("  {} No files selected.", "tip:".dimmed());
        return Ok(false);
    }

    // Map selected labels back to file paths
    let selected_paths: Vec<String> = selected
        .iter()
        .filter_map(|label| {
            // Find the matching change by label
            labels.iter().zip(changes.iter()).find_map(|(l, c)| {
                if l == label {
                    Some(c.path.clone())
                } else {
                    None
                }
            })
        })
        .collect();

    if selected_paths.len() == changes.len() {
        git::stage_all()?;
    } else {
        git::stage_files(&selected_paths)?;
    }

    println!(
        "{} Staged {} of {} file(s)",
        "✔".green(),
        selected_paths.len(),
        changes.len()
    );

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_menu_options_are_complete() {
        // Verify all action constants are distinct
        let options = [
            ACTION_COMMIT,
            ACTION_EDIT,
            ACTION_REGENERATE,
            ACTION_SETTINGS,
            ACTION_CANCEL,
            ACTION_COPY,
        ];
        let unique: std::collections::HashSet<_> = options.iter().collect();
        assert_eq!(unique.len(), options.len(), "Action labels must be unique");
    }

    #[test]
    fn test_pick_message_single_returns_directly() {
        let messages = vec!["feat: add login".to_string()];
        let result = pick_message(&messages).unwrap();
        assert_eq!(result, Some("feat: add login".to_string()));
    }

    #[test]
    fn test_combine_subject_body_joins_with_blank_line() {
        let result = combine_subject_body("feat: add auth", "- Add OAuth2\n- Add token refresh");
        assert_eq!(
            result,
            "feat: add auth\n\n- Add OAuth2\n- Add token refresh"
        );
    }

    #[test]
    fn test_combine_subject_body_empty_body_returns_subject_only() {
        let result = combine_subject_body("feat: add auth", "");
        assert_eq!(result, "feat: add auth");
    }

    #[test]
    fn test_session_config_from_config_defaults() {
        let config = Config {
            locale: "pt-br".into(),
            commit_type: CommitType::Conventional,
            max_length: 72,
            generate: 2,
            ..Config::default()
        };
        let session = SessionConfig::from_config(&config, None, None).unwrap();
        assert_eq!(session.locale, "pt-br");
        assert_eq!(session.commit_type, CommitType::Conventional);
        assert_eq!(session.max_length, 72);
        assert_eq!(session.generate, 2);
    }

    #[test]
    fn test_session_config_from_config_with_overrides() {
        let config = Config {
            locale: "en".into(),
            commit_type: CommitType::Plain,
            max_length: 72,
            generate: 1,
            ..Config::default()
        };
        let session = SessionConfig::from_config(&config, Some("subject+body"), Some(3)).unwrap();
        assert_eq!(session.commit_type, CommitType::SubjectBody);
        assert_eq!(session.generate, 3);
        // Non-overridden fields stay the same
        assert_eq!(session.locale, "en");
        assert_eq!(session.max_length, 72);
    }

    #[test]
    fn test_build_generation_params_plain() {
        let session = SessionConfig {
            locale: "en".into(),
            commit_type: CommitType::Conventional,
            max_length: 72,
            generate: 1,
        };
        let params = build_generation_params(&session, None);
        assert!(params.system.contains("en"));
        assert!(params.desc_system.is_none());
        assert_eq!(params.gen_opts.completions, 1);
    }

    #[test]
    fn test_build_generation_params_subject_body() {
        let session = SessionConfig {
            locale: "ja".into(),
            commit_type: CommitType::SubjectBody,
            max_length: 90,
            generate: 2,
        };
        let params = build_generation_params(&session, None);
        assert!(params.system.contains("ja"));
        assert!(params.desc_system.is_some());
        assert!(params.desc_system.unwrap().contains("ja"));
        assert_eq!(params.gen_opts.completions, 2);
        assert_eq!(params.gen_opts.max_tokens, 2000);
    }
}
