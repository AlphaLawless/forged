# forged — Plano de Implementação

Port do aicommits (TypeScript) para Rust. Nome do projeto: **forged** (abreviação de forgecommit).
Providers: **Claude** (Anthropic API nativa), **Gemini** (OpenAI-compat endpoint).
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

### Fase 0 — Scaffolding [DONE]
- [x] `cargo init --name forged`
- [x] Estrutura de módulos criada
- [x] `Cargo.toml` com todas as dependências

**Dependências atuais:**
```toml
clap = { version = "4", features = ["derive"] }
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", features = ["json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
dirs = "6"
inquire = "0.7"
colored = "3"
anyhow = "1"
async-trait = "0.1"
regex = "1"
futures = "0.3"
atty = "0.2"

[dev-dependencies]
mockito = "1"
tempfile = "3"
```

---

### Fase 1 — Config (`src/config.rs`) [DONE]
- [x] Struct `Config` com campos: `provider`, `api_key`, `model`, `locale`, `commit_type`, `max_length`, `generate`, `timeout`
- [x] `Config::load()` / `Config::load_from()` — lê `~/.forged` (INI parser manual)
- [x] `Config::save()` / `Config::save_to()` — escreve de volta
- [x] `Config::set(key, value)` — valida e atualiza um campo
- [x] Valores default quando campo ausente
- [x] `timeout` default = 0 (usa o default do provider)

**Testes (8):**
```
test_config_defaults_when_file_missing
test_config_load_from_ini_string
test_config_invalid_max_length_below_20
test_config_generate_above_5_returns_error
test_config_generate_zero_returns_error
test_config_invalid_commit_type
test_config_set_unknown_key_returns_error
test_config_save_and_reload_roundtrip
```

---

### Fase 2 — Git (`src/git.rs`) [DONE]
- [x] `assert_git_repo()` — `git rev-parse --show-toplevel`
- [x] `staged_diff(exclude)` com `staged_diff_impl(dir, exclude)` para testabilidade via `git -C`
- [x] `truncate_diff()` — corta em `MAX_DIFF_LENGTH` (30.000 chars)
- [x] `commit()` — `git commit -m` com split subject/body
- [x] Lock file detection e exclusão condicional

**Testes (6):**
```
test_lock_file_detection_matches_patterns
test_staged_diff_returns_none_when_nothing_staged
test_truncate_diff_short
test_truncate_diff_at_limit
test_staged_diff_with_files
test_lock_file_not_excluded_when_only_locks_staged
```

---

### Fase 3 — Prompt (`src/prompt.rs`) [DONE]
- [x] Enum `CommitType` em `config.rs` (Plain, Conventional, Gitmoji, SubjectBody)
- [x] `build_system_prompt(locale, max_len, commit_type, custom_prompt)`
- [x] `build_description_prompt(locale, max_len, custom_prompt)`

**Testes (9):**
```
test_prompt_contains_locale
test_prompt_contains_max_length
test_conventional_prompt_contains_feat_and_fix
test_gitmoji_prompt_contains_emoji_descriptions
test_custom_prompt_appended_to_base
test_subject_body_prompt_says_subject_only
test_prompt_filters_empty_sections
test_description_prompt_contains_locale
test_description_prompt_with_custom
```

---

### Fase 4 — Sanitização (`src/ai/sanitize.rs`) [DONE]
- [x] `sanitize_title()` — strip `<think>`, first line, trailing dot, quotes, leading tags
- [x] `sanitize_description()` — strip `<think>` mas mantém multiline
- [x] `deduplicate()` — remove duplicatas preservando ordem
- [x] `wrap_line()` — quebra linhas com indentação para bullets

**Testes (16):**
```
test_sanitize_removes_think_block_single_line
test_sanitize_removes_think_block_multiline
test_sanitize_removes_think_block_multiple_occurrences
test_sanitize_takes_only_first_line
test_sanitize_removes_trailing_dot
test_sanitize_preserves_trailing_dot_after_non_word
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
test_sanitize_description_keeps_multiple_lines
```

---

### Fase 5 — Trait do Provider (`src/ai/provider.rs`) [DONE]
- [x] `GenerateOpts` struct
- [x] `AiProvider` trait com `name()`, `default_model()`, `default_timeout()`, `complete()`
- [x] `generate_messages()` — chama N completions em paralelo, sanitiza, deduplica

**Testes (3):**
```
test_generate_messages_deduplicates_identical_responses
test_generate_messages_returns_all_unique
test_generate_messages_sanitizes_each_response
```

---

### Fase 6a — Provider Claude (`src/ai/providers/claude.rs`) [DONE]
- [x] `ClaudeProvider` com Anthropic API nativa
- [x] Endpoint: `POST {base_url}/v1/messages`
- [x] Headers: `x-api-key`, `anthropic-version: 2023-06-01`
- [x] `default_timeout()` = 30s
- [x] Error mapping: 401, 429, timeout, connect, malformed

**Testes com mockito (7):**
```
test_claude_sends_correct_headers
test_claude_sends_system_and_user_message
test_claude_parses_response_text_correctly
test_claude_401_returns_invalid_key_error
test_claude_429_returns_rate_limit_error
test_claude_malformed_response_returns_parse_error
```

---

### Fase 6b — Provider Gemini (`src/ai/providers/gemini.rs`) [DONE]
- [x] `GeminiProvider` via OpenAI-compatible endpoint
- [x] Endpoint: `POST {base_url}/chat/completions`
- [x] Headers: `Authorization: Bearer {key}`
- [x] `default_timeout()` = 60s (thinking models)
- [x] Error mapping: 401/403, 429, timeout, connect, empty choices

**Testes com mockito (6):**
```
test_gemini_sends_bearer_auth_header
test_gemini_sends_system_as_message_role
test_gemini_parses_choices_response
test_gemini_401_returns_invalid_key_error
test_gemini_429_returns_rate_limit_error
test_gemini_empty_choices_returns_error
```

---

### Fase 7 — Registry de Providers (`src/ai/mod.rs`) [DONE]
- [x] `build_provider(config)` — instancia Claude ou Gemini
- [x] Erro claro se provider desconhecido ou api_key vazia

**Testes (5):**
```
test_build_claude_provider_from_config
test_build_gemini_provider_from_config
test_build_gemini_without_api_key_returns_error
test_build_unknown_provider_returns_error
test_build_provider_without_api_key_returns_error
```

---

### Fase 8 — Setup Wizard (`src/commands/setup.rs`) [DONE]
- [x] `ProviderInfo` struct com key, label, models
- [x] `needs_setup()` — checa provider + api_key
- [x] `run()` — wizard interativo: provider → key → model → commit type → locale
- [x] Auto-trigger no primeiro uso (checa `atty::is(Stdin)`)
- [x] Subcomando `forged setup` para reconfigurar
- [x] Timeout por provider: config.timeout=0 usa `provider.default_timeout()`

**Testes (8):**
```
test_needs_setup_when_no_provider
test_needs_setup_when_no_api_key
test_no_setup_needed_when_configured
test_available_providers_contains_claude_and_gemini
test_available_provider_labels_match_count
test_find_provider_claude
test_find_provider_gemini
test_find_provider_unknown_returns_none
test_find_provider_by_label
```

---

### Fase 9 — Comando Commit (`src/commands/commit.rs`) [DONE]
- [x] Fluxo: setup → git repo → staged diff → build provider → prompt → AI → TUI → commit
- [x] Headless mode (GIT_PARAMS ou !atty)
- [x] Timeout inteligente: config override ou provider default

**Testes pendentes (integração):**
```
[ ] test_commit_flow_headless_prints_message
[ ] test_commit_flow_skips_confirm_with_yes_flag
[ ] test_commit_aborts_when_nothing_staged
[ ] test_commit_truncates_large_diff
```

---

### Fase 10 — Comando Config (`src/commands/config.rs`) [DONE]
- [x] `config set <key> <value>`
- [x] `config get <key>` (com mascaramento de api_key)
- [x] `config list`

**Testes (4):**
```
test_config_set_persists_to_file
test_config_get_reads_persisted_value
test_config_set_invalid_key_prints_error
test_config_set_provider_updates_correctly
```

---

### Fase 11 — CLI (`src/main.rs`) [DONE]
- [x] `forged [flags]` — comando padrão
- [x] `forged config {set,get,list}`
- [x] `forged setup`
- [x] Flags: -g, -x, -a, -t, -y, -c, -n, -p

**Testes (4):**
```
test_cli_parses_generate_flag
test_cli_parses_exclude_multiple_times
test_cli_default_values
test_cli_config_subcommand_set
```

---

## Estrutura de Arquivos Atual

```
forged/
├── Cargo.toml
├── src/
│   ├── main.rs
│   ├── config.rs
│   ├── git.rs
│   ├── prompt.rs
│   ├── ai/
│   │   ├── mod.rs            # build_provider(), re-exports
│   │   ├── provider.rs       # trait AiProvider, GenerateOpts, generate_messages()
│   │   ├── sanitize.rs       # sanitize_title, sanitize_description, wrap_line, deduplicate
│   │   └── providers/
│   │       ├── mod.rs
│   │       ├── claude.rs     # Anthropic API nativa
│   │       └── gemini.rs     # Google OpenAI-compat endpoint
│   └── commands/
│       ├── mod.rs
│       ├── commit.rs         # fluxo principal
│       ├── config.rs         # get/set/list
│       └── setup.rs          # wizard interativo primeira execução
└── tests/
    └── (commit_integration.rs pendente)
```

---

## Contagem de Testes Atual

| Módulo | Testes |
|---|---|
| config.rs | 8 |
| git.rs | 6 |
| prompt.rs | 9 |
| ai/sanitize.rs | 17 |
| ai/provider.rs | 3 |
| ai/providers/claude.rs | 6 |
| ai/providers/gemini.rs | 6 |
| ai/mod.rs | 5 |
| commands/setup.rs | 8 |
| commands/config.rs | 4 |
| main.rs (CLI) | 4 |
| **Total** | **77 (todos passando)** |

---

## Pendências

- [ ] Testes de integração em `tests/commit_integration.rs`
- [ ] CI pipeline (`cargo test` + `cargo clippy` + `cargo fmt --check`)
- [ ] Futuro: providers OpenRouter, ChatGPT (OpenAI)

---

## Regras de Qualidade

1. **Nenhum `unwrap()` fora de testes** — todo erro propagado com `?` ou mapeado com `anyhow::Context`
2. **Nenhum `clone()` desnecessário** — passar referências onde possível
3. **HTTP só nos providers** — `reqwest::Client` nunca vaza para fora de `ai/providers/`
4. **Testes não dependem de internet** — `mockito` para todos os HTTP, `tempfile` para filesystem
5. **`cargo clippy -- -D warnings` passa** — zero warnings
