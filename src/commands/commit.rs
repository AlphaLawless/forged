use anyhow::{Result, bail};
use colored::Colorize;

use crate::ai;
use crate::ai::provider::{GenerateOpts, generate_messages};
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
    let headless = std::env::var("GIT_PARAMS").is_ok() || atty_check();

    if !headless {
        println!("{}", " forged ".on_cyan().black());
    }

    git::assert_git_repo()?;

    if opts.stage_all {
        std::process::Command::new("git")
            .args(["add", "--update"])
            .status()?;
    }

    let staged = match git::staged_diff(&opts.exclude_files)? {
        Some(s) => s,
        None => bail!("No staged changes found. Stage your changes manually, or use the --all flag."),
    };

    if !headless {
        let file_count = staged.files.len();
        let label = if file_count == 1 { "file" } else { "files" };
        println!("{} Detected {} staged {}", "::".dimmed(), file_count, label);
    }

    let mut config = Config::load()?;

    // First-run setup: if no provider/key configured, launch the wizard
    if !headless && super::setup::needs_setup(&config) {
        config = super::setup::run(Some(config))?;
    } else if headless && super::setup::needs_setup(&config) {
        bail!("No configuration found. Run `forged` in an interactive terminal to set up, or manually configure with `forged config set`.");
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

    let gen_opts = GenerateOpts {
        model,
        temperature: 0.4,
        max_tokens: 2000,
        completions: generate,
        timeout_secs: config.timeout,
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

    let message = select_message(&messages, opts.skip_confirm)?;
    let Some(message) = message else {
        println!("Commit cancelled.");
        return Ok(());
    };

    if opts.clipboard {
        println!("Message: {message}");
        println!("{}", "(clipboard support coming soon)".dimmed());
        return Ok(());
    }

    git::commit(&message, opts.no_verify, &opts.extra_args)?;
    println!("{} Successfully committed!", "✔".green());

    Ok(())
}

fn select_message(messages: &[String], skip_confirm: bool) -> Result<Option<String>> {
    if skip_confirm {
        return Ok(Some(messages[0].clone()));
    }

    if messages.len() == 1 {
        println!("\n{}\n", messages[0].bold());
        let confirm = inquire::Confirm::new("Use this commit message?")
            .with_default(true)
            .prompt()?;
        return Ok(if confirm { Some(messages[0].clone()) } else { None });
    }

    let selection = inquire::Select::new("Pick a commit message:", messages.to_vec()).prompt()?;
    Ok(Some(selection))
}

/// Check if we're in a non-interactive context.
fn atty_check() -> bool {
    !atty::is(atty::Stream::Stdout)
}
