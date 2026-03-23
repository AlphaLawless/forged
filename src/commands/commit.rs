use anyhow::{Result, bail};
use colored::Colorize;

use crate::ai;
use crate::ai::provider::{GenerateOpts, generate_messages};
use crate::clipboard;
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
            bail!("No configuration found. Run `forged` in an interactive terminal to set up, or manually configure with `forged config set`.");
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
                bail!("No staged changes found. Stage your changes manually, or use the --all flag.");
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

    let provider = ai::build_provider(&config)?;

    let commit_type = if let Some(ref t) = opts.commit_type {
        crate::config::CommitType::from_str_loose(t)?
    } else {
        config.commit_type.clone()
    };

    let generate = opts.generate.unwrap_or(config.generate);
    let model = if config.model.is_empty() {
        provider.default_model().to_string()
    } else {
        config.model.clone()
    };

    let system = prompt::build_system_prompt(
        &config.locale,
        config.max_length,
        &commit_type,
        opts.custom_prompt.as_deref(),
    );

    let diff = git::truncate_diff(&staged.diff);

    let timeout = if config.timeout > 0 {
        config.timeout
    } else {
        provider.default_timeout()
    };

    let gen_opts = GenerateOpts {
        model,
        temperature: 0.4,
        max_tokens: 2000,
        completions: generate,
        timeout_secs: timeout,
    };

    if !headless {
        println!(
            "{} Analyzing changes in {} file{}...",
            "::".dimmed(),
            staged.files.len(),
            if staged.files.len() == 1 { "" } else { "s" }
        );
    }

    let messages = generate_messages(provider.as_ref(), &system, &diff, &gen_opts).await?;

    if messages.is_empty() {
        bail!("No commit messages were generated. Try again.");
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
                    Action::Edit | Action::Regenerate => {
                        // Let the outer loop handle regenerate or another edit
                        continue;
                    }
                }
            }
            Action::Regenerate => {
                println!("{} Regenerating...", "::".dimmed());
                current_messages =
                    generate_messages(provider.as_ref(), &system, &diff, &gen_opts).await?;
                if current_messages.is_empty() {
                    bail!("No commit messages were generated. Try again.");
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

// --- Action menu ---

#[derive(Debug, Clone, PartialEq)]
enum Action {
    Commit,
    Edit,
    Regenerate,
    Cancel,
}

const ACTION_COMMIT: &str = "Commit this message";
const ACTION_EDIT: &str = "Edit message";
const ACTION_REGENERATE: &str = "Regenerate";
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
    options.extend([ACTION_EDIT, ACTION_REGENERATE, ACTION_CANCEL]);

    let choice = inquire::Select::new("What do you want to do?", options)
        .with_page_size(10)
        .prompt()?;

    match choice {
        ACTION_COMMIT | ACTION_COPY => Ok(Action::Commit),
        ACTION_EDIT => Ok(Action::Edit),
        ACTION_REGENERATE => Ok(Action::Regenerate),
        ACTION_CANCEL => Ok(Action::Cancel),
        _ => Ok(Action::Cancel),
    }
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
            println!("{} Commit failed — pre-commit hook rejected the commit.", "✘".red());
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

/// Show unstaged changes and offer to stage them. Returns true if staging happened.
fn offer_stage_changes() -> Result<bool> {
    let changes = git::unstaged_changes().unwrap_or_default();

    if changes.is_empty() {
        println!("{} No changes found in this repository.", "⚠".yellow());
        println!();
        println!("  Nothing to commit — working tree is clean.");
        return Ok(false);
    }

    println!("{} No staged changes found, but you have unstaged changes:\n", "⚠".yellow());
    for file in &changes {
        let status_colored = match file.status.as_str() {
            "modified" => format!("{:>12}", file.status).yellow().to_string(),
            "new file" => format!("{:>12}", file.status).green().to_string(),
            "deleted" => format!("{:>12}", file.status).red().to_string(),
            _ => format!("{:>12}", file.status),
        };
        println!("  {}: {}", status_colored, file.path);
    }
    println!();

    let stage = inquire::Confirm::new("Stage all changes and continue?")
        .with_default(true)
        .prompt()?;

    if !stage {
        println!();
        println!("  {} Stage files manually:", "tip:".dimmed());
        println!("    git add <file>       Stage specific files");
        println!("    git add -A           Stage everything");
        println!("    forged --all         Auto-stage tracked files");
        return Ok(false);
    }

    git::stage_all()?;
    println!("{} Staged {} file(s)", "✔".green(), changes.len());

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_menu_options_are_complete() {
        // Verify all action constants are distinct
        let options = [ACTION_COMMIT, ACTION_EDIT, ACTION_REGENERATE, ACTION_CANCEL, ACTION_COPY];
        let unique: std::collections::HashSet<_> = options.iter().collect();
        assert_eq!(unique.len(), options.len(), "Action labels must be unique");
    }

    #[test]
    fn test_pick_message_single_returns_directly() {
        let messages = vec!["feat: add login".to_string()];
        let result = pick_message(&messages).unwrap();
        assert_eq!(result, Some("feat: add login".to_string()));
    }
}
