# forged — Plano de Implementação

Port do aicommits (TypeScript) para Rust. Nome do projeto: **forged** (abreviação de forgecommit).
Providers alvo: **Claude** (Anthropic API nativa). Gemini fica para fase posterior.
Filosofia: **feature → teste imediato → próxima feature**. Nenhum módulo avança sem cobertura de testes.

---

## Princípios de Teste

- Todo módulo core tem testes unitários no próprio arquivo (`#[cfg(test)]`)
- Chamadas HTTP são sempre abstraídas via trait → mockáveis sem internet
- Testes de integração em `tests/` cobrem o fluxo completo com stubs
- Se um teste quebrar após mudança, a mudança está errada — não o teste
- Nomes de teste descrevem comportamento: `test_sanitize_removes_think_tags`, não `test1`

---

## Fases e Tarefas

### Fase 0 — Scaffolding
- [ ] `cargo new forged --name forged`
- [ ] Estrutura de módulos criada (arquivos vazios com `todo!()`)
- [ ] `Cargo.toml` com todas as dependências
- [ ] CI mínimo: `cargo test` + `cargo clippy` + `cargo fmt --check`

**Dependências:**
```toml
clap = { version = "4", features = ["derive"] }
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", features = ["json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_ini = "0.2"
dirs = "5"
inquire = "0.7"
colored = "2"
anyhow = "1"
async-trait = "0.1"
regex = "1"

[dev-dependencies]
mockito = "1"          # mock HTTP server local para testes de provider
tempfile = "3"         # diretórios temporários para testes de config/git
```

---

### Fase 1 — Config (`src/config.rs`)

**Implementar:**
- Struct `Config` com campos: `provider`, `api_key`, `model`, `locale`, `commit_type`, `max_length`, `generate`, `timeout`
- `Config::load() -> Result<Config>` — lê `~/.aicommits` (INI)
- `Config::save(&self) -> Result<()>` — escreve de volta
- `Config::set(key, value) -> Result<()>` — valida e atualiza um campo
- Valores default quando campo ausente

**Testes unitários (`#[cfg(test)]` em `config.rs`):**
```
test_config_defaults_when_file_missing
test_config_load_from_ini_string
test_config_invalid_provider_returns_error
test_config_invalid_max_length_below_20
test_config_generate_above_5_returns_error
test_config_locale_must_be_letters_only
test_config_set_unknown_key_returns_error
test_config_save_and_reload_roundtrip      ← usa tempfile
```

---

### Fase 2 — Git (`src/git.rs`)

**Implementar:**
- `assert_git_repo() -> Result<PathBuf>` — `git rev-parse --show-toplevel`
- `staged_diff(exclude: &[&str]) -> Result<Option<StagedDiff>>`
  - `StagedDiff { files: Vec<String>, diff: String }`
  - Exclui lock files quando há arquivos não-lock staged
- `staged_diff_for_files(files: &[&str], exclude: &[&str]) -> Result<StagedDiff>`
- Constante `LOCK_FILE_PATTERNS` — `*.lock`, `package-lock.json`, `pnpm-lock.yaml`

**Testes unitários:**
```
test_lock_file_detection_matches_patterns
test_lock_file_not_excluded_when_only_locks_staged
test_staged_diff_returns_none_when_nothing_staged   ← git repo temporário com tempfile
test_staged_diff_excludes_lock_files_when_mixed
test_diff_truncation_at_30000_chars
```

> Nota: testes de git criam um repo temporário com `tempfile::TempDir` + `git init` + `git add`.

---

### Fase 3 — Prompt (`src/prompt.rs`)

**Implementar:**
- Enum `CommitType { Plain, Conventional, Gitmoji, SubjectBody }`
- `build_system_prompt(locale, max_len, commit_type, custom_prompt) -> String`
- `build_description_prompt(locale, max_len, custom_prompt) -> String`
- Textos dos tipos (conventional com tipos JSON, gitmoji com emojis)

**Testes unitários:**
```
test_prompt_contains_locale
test_prompt_contains_max_length
test_conventional_prompt_contains_feat_and_fix
test_gitmoji_prompt_contains_emoji_descriptions
test_custom_prompt_appended_to_base
test_subject_body_prompt_says_subject_only
test_prompt_filters_empty_sections      ← nenhum None/vazio vaza no join
```

---

### Fase 4 — Sanitização (`src/ai/sanitize.rs`)

**Implementar:**
- `sanitize_title(msg: &str) -> String`
  - Remove blocos `<think>...</think>` (regex multiline)
  - Pega só a primeira linha
  - Remove trailing `.`
  - Remove aspas envolventes
  - Remove tags HTML leading (`<tag>`)
- `sanitize_description(msg: &str) -> String`
  - Remove `<think>` mas mantém múltiplas linhas
- `deduplicate(msgs: Vec<String>) -> Vec<String>`
- `wrap_line(line: &str, max_len: usize) -> String` — quebra linhas longas com indentação para bullets

**Testes unitários — este módulo merece cobertura densa:**
```
test_sanitize_removes_think_block_single_line
test_sanitize_removes_think_block_multiline
test_sanitize_removes_think_block_multiple_occurrences
test_sanitize_takes_only_first_line
test_sanitize_removes_trailing_dot
test_sanitize_removes_surrounding_double_quotes
test_sanitize_removes_surrounding_single_quotes
test_sanitize_removes_leading_html_tag
test_sanitize_preserves_normal_message
test_deduplicate_removes_exact_duplicates
test_deduplicate_preserves_order
test_wrap_line_short_line_unchanged
test_wrap_line_breaks_on_space
test_wrap_line_bullet_continuation_indented
test_wrap_line_no_space_forces_hard_break
```

---

### Fase 5 — Trait do Provider (`src/ai/provider.rs`)

**Implementar:**
```rust
pub struct GenerateOpts {
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub completions: u8,
    pub timeout_secs: u64,
}

#[async_trait]
pub trait AiProvider: Send + Sync {
    fn name(&self) -> &str;
    fn default_model(&self) -> &str;
    async fn complete(&self, system: &str, user: &str, opts: &GenerateOpts) -> Result<String>;
}

// função de alto nível: chama complete() N vezes em paralelo, deduplica, sanitiza
pub async fn generate_messages(
    provider: &dyn AiProvider,
    system: &str,
    user: &str,
    opts: &GenerateOpts,
) -> Result<Vec<String>>
```

**Testes unitários:**
```
test_generate_messages_deduplicates_identical_responses
test_generate_messages_returns_all_unique
test_generate_messages_sanitizes_each_response
```
> Usa um `MockProvider` que implementa `AiProvider` retornando strings fixas — zero HTTP.

---

### Fase 6 — Provider Claude (`src/ai/providers/claude.rs`)

**Implementar:**
- `ClaudeProvider { api_key: String, client: reqwest::Client }`
- Implementa `AiProvider::complete()`
- Endpoint: `POST https://api.anthropic.com/v1/messages`
- Headers: `x-api-key`, `anthropic-version: 2023-06-01`
- Body: `{ model, max_tokens, system, messages: [{role:"user", content}], temperature }`
- Extrai `content[0].text` da resposta
- Mapeia erros HTTP: 401 → "invalid API key", 429 → "rate limited", timeout → mensagem clara

**Testes com mockito (servidor HTTP local):**
```
test_claude_sends_correct_headers
test_claude_sends_system_and_user_message
test_claude_parses_response_text_correctly
test_claude_401_returns_invalid_key_error
test_claude_429_returns_rate_limit_error
test_claude_timeout_returns_timeout_error
test_claude_malformed_response_returns_parse_error
```

---

### Fase 7 — Registry de Providers (`src/ai/mod.rs`)

**Implementar:**
- `build_provider(config: &Config) -> Result<Box<dyn AiProvider>>`
  - Lê `config.provider` e instancia o correto
  - Erro claro se provider desconhecido ou api_key vazia

**Testes unitários:**
```
test_build_claude_provider_from_config
test_build_unknown_provider_returns_error
test_build_provider_without_api_key_returns_error
```

---

### Fase 9 — Comando Commit (`src/commands/commit.rs`)

**Implementar o fluxo completo:**
```
1. git::assert_repo()
2. git::staged_diff(exclude) → StagedDiff
3. config::load()
4. ai::build_provider(&config)
5. prompt::build_system_prompt(...)
6. truncar diff se > 30_000 chars
7. ai::generate_messages(provider, system, diff, opts)
8. se headless: println! e exit
9. se interativo: inquire::select ou confirm
10. git commit -m "..."
```

**Testes de integração (`tests/commit_integration.rs`):**
```
test_commit_flow_headless_prints_message   ← mock provider + git repo temp
test_commit_flow_skips_confirm_with_yes_flag
test_commit_aborts_when_nothing_staged
test_commit_truncates_large_diff
test_commit_chunks_large_number_of_files   ← >50 arquivos
```

---

### Fase 10 — Comando Config (`src/commands/config.rs`)

**Implementar:**
- `config get <key>` — imprime valor atual
- `config set <key> <value>` — valida e persiste
- `config list` — lista todos os pares

**Testes:**
```
test_config_set_persists_to_file
test_config_get_reads_persisted_value
test_config_set_invalid_key_prints_error
test_config_set_provider_updates_correctly
```

---

### Fase 11 — CLI (`src/main.rs`)

**Implementar com clap derive:**
```
forged [flags]                       # comando padrão: gerar commit
forged config set <key> <value>
forged config get <key>
forged config list

Flags:
  -g, --generate <N>    Quantas mensagens gerar (1–5)
  -a, --all             Stage todos os tracked files
  -t, --type <TYPE>     plain|conventional|gitmoji|subject+body
  -y, --yes             Pula confirmação
  -c, --clipboard       Copia ao invés de commitar
  -n, --no-verify       Bypassa pre-commit hooks
  -p, --prompt <TEXT>   Prompt customizado
  -x, --exclude <FILE>  Excluir arquivo do diff (repetível)
  -v, --version         Versão
```

**Testes:**
```
test_cli_parses_generate_flag
test_cli_parses_exclude_multiple_times
test_cli_version_flag
test_cli_config_subcommand_set
```

---

## Estrutura de Arquivos Final

```
forged/
├── Cargo.toml
├── src/
│   ├── main.rs
│   ├── config.rs
│   ├── git.rs
│   ├── prompt.rs
│   └── ai/
│       ├── mod.rs          # build_provider(), generate_messages()
│       ├── provider.rs     # trait AiProvider, GenerateOpts
│       ├── sanitize.rs     # sanitize_title, sanitize_description, wrap_line
│       └── providers/
│           ├── mod.rs
│           └── claude.rs
│   └── commands/
│       ├── commit.rs
│       └── config.rs
└── tests/
    └── commit_integration.rs
```

---

## Contagem Estimada de Testes

| Módulo | Testes |
|---|---|
| config.rs | 8 |
| git.rs | 5 |
| prompt.rs | 7 |
| ai/sanitize.rs | 15 |
| ai/provider.rs | 3 |
| ai/providers/claude.rs | 7 |
| ai/mod.rs | 3 |
| commands/commit.rs | 5 |
| commands/config.rs | 4 |
| main.rs (CLI) | 4 |
| **Total** | **~57 testes** |

---

## Ordem de Implementação (sem dependências circulares)

```
Fase 0 (scaffold)
  → Fase 1 (config)      # zero deps externas
  → Fase 3 (prompt)      # zero deps externas
  → Fase 4 (sanitize)    # zero deps externas
  → Fase 2 (git)         # só std::process
  → Fase 5 (trait)       # depende de sanitize
  → Fase 6 (claude)      # depende de trait + reqwest
  → Fase 7 (registry)    # depende de config + claude
  → Fase 8 (commit cmd)  # depende de tudo acima
  → Fase 9 (config cmd)  # depende de config
  → Fase 10 (main/CLI)   # depende de tudo
```

---

## Regras de Qualidade

1. **Nenhum `unwrap()` fora de testes** — todo erro propagado com `?` ou mapeado com `anyhow::Context`
2. **Nenhum `clone()` desnecessário** — passar referências onde possível
3. **HTTP só nos providers** — `reqwest::Client` nunca vaza para fora de `ai/providers/`
4. **Testes não dependem de internet** — `mockito` para todos os HTTP, `tempfile` para filesystem
5. **`cargo clippy -- -D warnings` passa** — zero warnings no CI
