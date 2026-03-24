# forged

AI-powered git commit message generator. Written in Rust.

Analyzes your staged changes and generates meaningful commit messages using Claude or Gemini. Supports conventional commits, gitmoji, subject+body format, and more.

## Install

You need an API key from [Anthropic](https://console.anthropic.com/) or [Google AI Studio](https://aistudio.google.com/apikey).

### One-liner (Linux/macOS)

```sh
curl -fsSL https://raw.githubusercontent.com/AlphaLawless/forged/main/install.sh | sh
```

### From GitHub Releases

Download the latest binary for your platform from [Releases](https://github.com/AlphaLawless/forged/releases/latest).

| Platform | Asset |
|----------|-------|
| Linux x86_64 | `forged-linux-x86_64.tar.gz` |
| Linux aarch64 | `forged-linux-aarch64.tar.gz` |
| macOS x86_64 | `forged-darwin-x86_64.tar.gz` |
| macOS Apple Silicon | `forged-darwin-aarch64.tar.gz` |
| Windows x86_64 | `forged-windows-x86_64.zip` |

### From source

Requires [Rust](https://www.rust-lang.org/tools/install) 1.85+.

```sh
git clone https://github.com/AlphaLawless/forged.git
cd forged
cargo install --path .
```

### Verify

```sh
forged --version
```

## Quick start

```sh
# First run — interactive setup
forged

# Or configure manually
forged config set provider gemini
forged config set api_key YOUR_KEY
forged config set type conventional
```

## Usage

```sh
# Generate a commit message from staged changes
forged

# Stage all tracked changes and commit
forged -a

# Skip confirmation (auto-commit first result)
forged -y

# Generate 3 options to pick from
forged -g 3

# Use a specific commit format
forged -t conventional
forged -t gitmoji
forged -t subject+body

# Copy to clipboard instead of committing
forged -c

# Exclude files from analysis
forged -x package-lock.json -x yarn.lock

# Custom prompt to guide the AI
forged -p "Always mention the ticket number"

# Bypass pre-commit hooks
forged -n
```

### Config

```sh
forged config set <key> <value>
forged config get <key>
forged config list
```

Available keys: `provider`, `api_key`, `model`, `locale`, `type`, `max_length`, `generate`, `timeout`

### Git hook

Auto-generate messages when you run `git commit`:

```sh
# Install hook in current repo
forged hook install

# Now just use git normally
git add .
git commit    # editor opens with AI-generated message

# Remove hook
forged hook uninstall
```

The hook writes the message as a draft — you still review it in your editor before confirming.

### Interactive menu

After generating a message, you get:

```
feat: add OAuth2 login flow

? What do you want to do?
> Commit this message
  Edit message
  Regenerate
  Settings
  Cancel
```

**Settings** lets you change locale, commit type, max length, and generation count for the current session without modifying your config file.

## Providers

| Provider | Models |
|----------|--------|
| Claude (Anthropic) | claude-sonnet-4-6, claude-haiku-4-5, claude-opus-4-6 |
| Gemini (Google) | gemini-2.5-flash, gemini-2.5-pro, gemini-2.0-flash |

## Commit formats

| Format | Example |
|--------|---------|
| `plain` | `add user authentication` |
| `conventional` | `feat: add user authentication` |
| `gitmoji` | `✨ add user authentication` |
| `subject+body` | Subject line + detailed bullet points (2-step AI generation) |

## Config file

Stored at `~/.forged`:

```ini
provider=gemini
api_key=AIza...
model=gemini-2.5-flash
locale=en
type=conventional
max_length=72
generate=1
timeout=0
```

`timeout=0` uses the provider's default (30s for Claude, 60s for Gemini).

## Roadmap

### Done

- [x] Claude and Gemini providers
- [x] Conventional commits, gitmoji, plain, subject+body formats
- [x] Interactive commit menu (commit, edit, regenerate, cancel)
- [x] File picker when nothing is staged
- [x] Clipboard support
- [x] Session settings buffer (change config per-session without persisting)
- [x] Git hook integration (`prepare-commit-msg`)
- [x] Subject+body 2-step generation
- [x] Lock file auto-exclusion
- [x] Diff truncation for large changes
- [x] CI pipeline (check, fmt, clippy, test)
- [x] GitHub Releases with cross-platform binaries
- [x] One-liner install script

### Planned

- [ ] Multi-provider with failover (configure multiple providers, auto-fallback on rate limit/timeout)
- [ ] Local LLM support (Ollama, llama.cpp, LM Studio — works offline, no API key needed)
- [ ] OpenRouter and ChatGPT providers
- [ ] Large diff chunking (50+ files)

## License

[MIT](./LICENSE)
