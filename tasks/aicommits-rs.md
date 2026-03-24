# forged — Plano de Implementação

Port do aicommits (TypeScript) para Rust. Nome do projeto: **forged** (abreviação de forgecommit).
Providers: **Claude** (Anthropic API nativa), **Gemini** (OpenAI-compat endpoint).
Filosofia: **feature → teste imediato → próxima feature**. Nenhum módulo avança sem cobertura de testes.

---

## Princípios de Teste

- Todo módulo core tem testes unitários no próprio arquivo (`#[cfg(test)]`)
- Chamadas HTTP são sempre abstraídas via trait → mockáveis sem internet
- Se um teste quebrar após mudança, a mudança está errada — não o teste
- Nomes de teste descrevem comportamento: `test_sanitize_removes_think_tags`, não `test1`

---

## Fases Concluídas

### Fase 0 — Scaffolding [DONE]
- [x] `cargo init --name forged`
- [x] Estrutura de módulos
- [x] `Cargo.toml` com dependências

**Dependências atuais:**
```toml
clap = { version = "4", features = ["derive"] }
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", features = ["json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
dirs = "6"
inquire = { version = "0.7", features = ["editor"] }
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
- [x] Struct `Config` com: provider, api_key, model, locale, commit_type, max_length, generate, timeout
- [x] `Config::load()` / `load_from()` — lê `~/.forged` (INI parser manual)
- [x] `Config::save()` / `save_to()`
- [x] `Config::set(key, value)` — validação
- [x] timeout default = 0 (usa default do provider)

**Testes (8):** `defaults_when_file_missing`, `load_from_ini_string`, `invalid_max_length_below_20`, `generate_above_5`, `generate_zero`, `invalid_commit_type`, `set_unknown_key`, `save_and_reload_roundtrip`

---

### Fase 2 — Git (`src/git.rs`) [DONE]
- [x] `assert_git_repo()`
- [x] `staged_diff()` / `staged_diff_impl()` com `git -C` para testabilidade
- [x] `truncate_diff()` — 30.000 chars max
- [x] `commit()` → retorna `CommitResult` (Success / HookFailed)
- [x] Lock file detection e exclusão condicional
- [x] `unstaged_changes()` / `unstaged_changes_impl()` — `git status --porcelain`
- [x] `stage_all()` — `git add -A`
- [x] `stage_files()` — `git add -- <files>`

**Testes (9):** `lock_file_detection`, `staged_diff_none`, `truncate_short`, `truncate_at_limit`, `staged_diff_with_files`, `lock_not_excluded_only_locks`, `unstaged_detects_modified`, `unstaged_detects_new`, `unstaged_empty_when_clean`

---

### Fase 3 — Prompt (`src/prompt.rs`) [DONE]
- [x] `build_system_prompt()` — plain/conventional/gitmoji/subject+body
- [x] `build_description_prompt()`

**Testes (9):** `contains_locale`, `contains_max_length`, `conventional_feat_fix`, `gitmoji_emoji`, `custom_prompt_appended`, `subject_body`, `filters_empty`, `description_locale`, `description_custom`

---

### Fase 4 — Sanitização (`src/ai/sanitize.rs`) [DONE]
- [x] `sanitize_title()` — strip `<think>`, first line, trailing dot, quotes, leading tags
- [x] `sanitize_description()` — multiline
- [x] `deduplicate()`, `wrap_line()`

**Testes (17):** strip think (3), first line, trailing dot (2), quotes (2), html tag, normal, dedup (2), wrap (4), description multiline

---

### Fase 5 — Trait do Provider (`src/ai/provider.rs`) [DONE]
- [x] `AiProvider` trait: `name()`, `default_model()`, `default_timeout()`, `complete()`
- [x] `generate_messages()` — N completions em paralelo, sanitiza, deduplica

**Testes (3):** dedup identical, returns unique, sanitizes each

---

### Fase 6a — Provider Claude (`src/ai/providers/claude.rs`) [DONE]
- [x] Anthropic API nativa, `default_timeout()` = 30s
- [x] Error mapping: 401, 429, timeout, connect, malformed

**Testes mockito (6):** headers, system+user, parse response, 401, 429, malformed

---

### Fase 6b — Provider Gemini (`src/ai/providers/gemini.rs`) [DONE]
- [x] OpenAI-compat endpoint, `default_timeout()` = 60s
- [x] Error mapping: 401/403, 429, timeout, connect, empty choices

**Testes mockito (6):** bearer auth, system as message, parse choices, 401, 429, empty choices

---

### Fase 7 — Registry de Providers (`src/ai/mod.rs`) [DONE]
- [x] `build_provider(config)` — Claude ou Gemini

**Testes (5):** build claude, build gemini, gemini no key, unknown provider, claude no key

---

### Fase 8 — Setup Wizard (`src/commands/setup.rs`) [DONE]
- [x] `ProviderInfo` struct, `needs_setup()`, wizard interativo
- [x] Auto-trigger na primeira execução
- [x] Subcomando `forged setup`
- [x] Modelos: Claude (sonnet/haiku/opus), Gemini (2.5-flash/2.5-pro/2.0-flash)

**Testes (8):** needs_setup (2), no_setup_needed, providers_contains_both, labels_match_count, find_claude, find_gemini, find_unknown, find_by_label

---

### Fase 9 — Comando Commit (`src/commands/commit.rs`) [DONE]
- [x] Fluxo: setup → git repo → staged diff → provider → prompt → AI → TUI → commit
- [x] Headless mode (GIT_PARAMS ou !atty)
- [x] Timeout inteligente: config override ou provider default

**Testes (2):** action_menu_options_complete, pick_message_single

---

### Fase 10 — Comando Config (`src/commands/config.rs`) [DONE]
- [x] `config set/get/list`

**Testes (4):** set_persists, get_reads, set_invalid_key, set_provider

---

### Fase 11 — CLI (`src/main.rs`) [DONE]
- [x] `forged [flags]`, `forged config {set,get,list}`, `forged setup`

**Testes (4):** generate_flag, exclude_multiple, default_values, config_subcommand

---

### Fase 12 — Melhorias de DX [DONE]

#### 12a — Loop interativo pós-geração [DONE]
- [x] Menu de ações: Commit / Edit / Regenerate / Cancel
- [x] Edit abre `$EDITOR` via `inquire::Editor`
- [x] Regenerate chama a API de novo e volta ao menu
- [x] `--yes` pula o menu (commit direto)
- [x] `--clipboard` mostra "Copy to clipboard" no lugar de "Commit"

#### 12b — DX "no staged changes" com file picker [DONE]
- [x] Detecta unstaged changes via `git status --porcelain`
- [x] `MultiSelect` com todos pré-selecionados para toggle individual
- [x] `space` = toggle, `→` = all, `←` = none, `enter` = confirma
- [x] Filtragem por texto habilitada (útil em projetos grandes)
- [x] Após stage, continua o fluxo direto (sem pedir para rodar de novo)
- [x] Working tree clean → mensagem clara

#### 12c — Clipboard funcional [DONE]
- [x] `src/clipboard.rs` — detecta wl-copy, xclip, xsel, pbcopy, clip
- [x] Flag `--clipboard` agora funciona (antes era "coming soon")

#### 12d — Commit error handling [DONE]
- [x] `CommitResult` enum: Success / HookFailed
- [x] Pre-commit hook failure → mensagem com dica de `--no-verify`

**Testes novos na Fase 12 (7):**
- git.rs: `unstaged_detects_modified`, `unstaged_detects_new`, `unstaged_empty_when_clean`
- commit.rs: `action_menu_options_complete`, `pick_message_single`
- clipboard.rs: `clipboard_commands_non_empty`, `copy_does_not_panic`

---

## Estrutura de Arquivos Atual

```
forged/
├── Cargo.toml
├── src/
│   ├── main.rs               # CLI (clap derive)
│   ├── config.rs              # ~/.forged INI read/write/validate
│   ├── git.rs                 # staged diff, unstaged changes, stage, commit
│   ├── prompt.rs              # system prompt builder
│   ├── clipboard.rs           # copy to clipboard cross-platform
│   ├── ai/
│   │   ├── mod.rs             # build_provider() registry
│   │   ├── provider.rs        # trait AiProvider, generate_messages()
│   │   ├── sanitize.rs        # sanitize, deduplicate, wrap
│   │   └── providers/
│   │       ├── mod.rs
│   │       ├── claude.rs      # Anthropic API nativa
│   │       └── gemini.rs      # Google OpenAI-compat endpoint
│   └── commands/
│       ├── mod.rs
│       ├── commit.rs          # fluxo principal + action menu + file picker
│       ├── config.rs          # get/set/list
│       └── setup.rs           # wizard interativo
└── tests/
```

---

### Fase 13 — Subject+Body 2-Step Generation [DONE]
- [x] `generate_description()` em `provider.rs` — chamada única à IA com subject+diff, sanitiza com `sanitize_description()`
- [x] `generate_full_messages()` em `commit.rs` — orquestra 2 passos quando `commit_type == SubjectBody`
- [x] `combine_subject_body()` — combina subject + `\n\n` + body (body vazio = só subject)
- [x] Regenerate usa o mesmo fluxo de 2 passos

**Testes novos (4):**
- provider.rs: `test_generate_description_returns_sanitized_body`, `test_generate_description_includes_subject_in_user_prompt`
- commit.rs: `test_combine_subject_body_joins_with_blank_line`, `test_combine_subject_body_empty_body_returns_subject_only`

---

### Fase 14 — Testes de Integração [DONE]
- [x] `src/lib.rs` criado para expor módulos publicamente (necessário para `tests/`)
- [x] `src/main.rs` atualizado para `use forged::{commands, config}`
- [x] `tests/git_commit.rs` — 5 testes: single-line, subject+body, hook failure, no-verify, extra args
- [x] `tests/full_flow.rs` — 3 testes: plain commit, subject+body commit, sanitization chain (config → git → mock AI → commit)
- [x] `tests/generate_pipeline.rs` — 4 testes: plain subjects, subject+body pipeline, dedup, empty body
- [x] `tests/provider_http.rs` — 4 testes: Claude/Gemini full JSON roundtrip, Claude/Gemini timeout enforcement
- [x] `serial_test` adicionado como dev-dependency para testes que alteram CWD

---

### Fase 15 — Session Settings Buffer [DONE]
- [x] `SessionConfig` struct — buffer efêmero com locale, commit_type, max_length, generate
- [x] `build_generation_params()` — centraliza construção de system/desc_system/gen_opts
- [x] `Action::Settings` no action menu
- [x] `settings_menu()` — submenu interativo para editar campos com validação
- [x] Regenerate usa `SessionConfig` atualizado (buffer refletido automaticamente)
- [x] Settings + mudança → regenera automaticamente com novos valores
- [x] `PasswordDisplayMode::Masked` no setup wizard (fix cursor)

**Testes novos (4):**
- `test_session_config_from_config_defaults`, `test_session_config_from_config_with_overrides`
- `test_build_generation_params_plain`, `test_build_generation_params_subject_body`

---

## Contagem de Testes Atual

| Módulo | Testes |
|---|---|
| config.rs | 8 |
| git.rs | 9 |
| prompt.rs | 9 |
| ai/sanitize.rs | 17 |
| ai/provider.rs | 5 |
| ai/providers/claude.rs | 6 |
| ai/providers/gemini.rs | 6 |
| ai/mod.rs | 5 |
| commands/setup.rs | 8 |
| commands/commit.rs | 8 |
| commands/config.rs | 4 |
| clipboard.rs | 2 |
| main.rs (CLI) | 4 |
| **Total unitários** | **92** |

### Testes de Integração (`tests/`)

| Arquivo | Testes |
|---|---|
| tests/git_commit.rs | 5 |
| tests/full_flow.rs | 3 |
| tests/generate_pipeline.rs | 4 |
| tests/provider_http.rs | 4 |
| **Total integração** | **16** |

| **Total geral** | **108 (todos passando)** |

---

## Pendências

- [x] subject+body generation em 2 passos (subject → description com context)
- [x] Testes de integração em `tests/`
- [ ] Git hook integration (`forged hook install/uninstall`, prepare-commit-msg)
- [ ] CI pipeline (`cargo test` + `cargo clippy` + `cargo fmt --check`)
- [ ] Futuro: providers OpenRouter, ChatGPT (OpenAI)
- [ ] Futuro: large diff chunking (>50 files → chunks de 10 → combine)
- [ ] Futuro: auto-update via crates.io

---

## Regras de Qualidade

1. **Nenhum `unwrap()` fora de testes** — todo erro propagado com `?` ou `anyhow::Context`
2. **Nenhum `clone()` desnecessário** — referências onde possível
3. **HTTP só nos providers** — `reqwest::Client` nunca vaza
4. **Testes não dependem de internet** — `mockito` + `tempfile`
5. **`cargo clippy -- -D warnings`** — zero warnings
