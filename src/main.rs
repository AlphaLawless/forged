#![allow(dead_code)] // Functions are public API; not all called from main yet during development

mod ai;
mod clipboard;
mod commands;
mod config;
mod git;
mod prompt;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "forged", about = "AI-powered git commit message generator", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Number of messages to generate (1-5)
    #[arg(short, long)]
    generate: Option<u8>,

    /// Files to exclude from AI analysis (repeatable)
    #[arg(short = 'x', long = "exclude")]
    exclude: Vec<String>,

    /// Automatically stage tracked file changes
    #[arg(short, long, default_value_t = false)]
    all: bool,

    /// Commit message format: plain, conventional, gitmoji, subject+body
    #[arg(short = 't', long = "type")]
    commit_type: Option<String>,

    /// Skip confirmation
    #[arg(short = 'y', long, default_value_t = false)]
    yes: bool,

    /// Copy to clipboard instead of committing
    #[arg(short, long, default_value_t = false)]
    clipboard: bool,

    /// Bypass pre-commit hooks
    #[arg(short = 'n', long = "no-verify", default_value_t = false)]
    no_verify: bool,

    /// Custom prompt to guide the LLM
    #[arg(short, long)]
    prompt: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Run the interactive setup wizard
    Setup,
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Set a config value
    Set { key: String, value: String },
    /// Get a config value
    Get { key: String },
    /// List all config values
    List,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Some(Commands::Config { action }) => match action {
            ConfigAction::Set { key, value } => commands::config::run_set(&key, &value),
            ConfigAction::Get { key } => commands::config::run_get(&key),
            ConfigAction::List => commands::config::run_list(),
        },
        Some(Commands::Setup) => {
            let existing = config::Config::load().ok();
            commands::setup::run(existing).map(|_| ())
        }
        None => {
            commands::commit::run(commands::commit::CommitOpts {
                generate: cli.generate,
                exclude_files: cli.exclude,
                stage_all: cli.all,
                commit_type: cli.commit_type,
                skip_confirm: cli.yes,
                clipboard: cli.clipboard,
                no_verify: cli.no_verify,
                custom_prompt: cli.prompt,
                extra_args: vec![],
            })
            .await
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;
    use super::Cli;

    #[test]
    fn test_cli_parses_generate_flag() {
        let cli = Cli::parse_from(["forged", "-g", "3"]);
        assert_eq!(cli.generate, Some(3));
    }

    #[test]
    fn test_cli_parses_exclude_multiple_times() {
        let cli = Cli::parse_from(["forged", "-x", "a.txt", "-x", "b.txt"]);
        assert_eq!(cli.exclude, vec!["a.txt", "b.txt"]);
    }

    #[test]
    fn test_cli_default_values() {
        let cli = Cli::parse_from(["forged"]);
        assert!(cli.generate.is_none());
        assert!(!cli.all);
        assert!(!cli.yes);
        assert!(!cli.clipboard);
        assert!(!cli.no_verify);
    }

    #[test]
    fn test_cli_config_subcommand_set() {
        let cli = Cli::parse_from(["forged", "config", "set", "provider", "claude"]);
        assert!(cli.command.is_some());
    }
}
