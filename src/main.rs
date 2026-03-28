use clap::{Parser, Subcommand};
use forged::{commands, config};

#[derive(Parser)]
#[command(
    name = "forged",
    about = "AI-powered git commit message generator",
    version
)]
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

    /// Write AI message to file (used by git hook, not for direct use)
    #[arg(long, hide = true)]
    hook: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Run the interactive setup wizard
    Setup {
        #[command(subcommand)]
        scope: Option<SetupScope>,
    },
    /// Upgrade forged to the latest version
    Upgrade,
    /// Manage git hook integration
    Hook {
        #[command(subcommand)]
        action: HookAction,
    },
}

#[derive(Subcommand)]
enum HookAction {
    /// Install the prepare-commit-msg hook
    Install {
        /// Overwrite existing hook even if not created by forged
        #[arg(long, default_value_t = false)]
        force: bool,
    },
    /// Remove the prepare-commit-msg hook
    Uninstall {
        /// Remove hook even if not created by forged
        #[arg(long, default_value_t = false)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum SetupScope {
    /// Manage local (per-repo) configuration profiles
    Local {
        /// Remove the local config for this repo
        #[arg(long, default_value_t = false)]
        remove: bool,

        /// Use an existing profile (interactive picker if name omitted)
        #[arg(long = "use", value_name = "PROFILE", num_args = 0..=1, default_missing_value = "")]
        use_profile: Option<String>,

        /// List available local profiles
        #[arg(long, default_value_t = false)]
        list: bool,
    },
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
        Some(Commands::Setup { scope }) => match scope {
            None => {
                let existing = config::Config::load_global().ok();
                commands::setup::run(existing).map(|_| ())
            }
            Some(SetupScope::Local {
                remove,
                use_profile,
                list,
            }) => {
                if remove {
                    commands::setup::remove_local()
                } else if list {
                    commands::setup::list_profiles()
                } else if let Some(name) = use_profile {
                    let name = if name.is_empty() { None } else { Some(name) };
                    commands::setup::use_profile(name.as_deref())
                } else {
                    commands::setup::run_local().map(|_| ())
                }
            }
        },
        Some(Commands::Upgrade) => commands::upgrade::run().await,
        Some(Commands::Hook { action }) => match action {
            HookAction::Install { force } => commands::hook::install(force),
            HookAction::Uninstall { force } => commands::hook::uninstall(force),
        },
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
                hook_file: cli.hook,
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
    use super::Cli;
    use clap::Parser;

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
