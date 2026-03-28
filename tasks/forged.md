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
│   ├── main.rs               # CLI (clap derive) + SetupScope::Local
│   ├── lib.rs                 # re-exports públicos
│   ├── config.rs              # ~/.forged/{global,locals/*} INI read/write/validate/merge
│   ├── git.rs                 # staged diff, unstaged changes, stage, commit, try_repo_root
│   ├── prompt.rs              # system prompt builder
│   ├── clipboard.rs           # copy to clipboard cross-platform
│   ├── ai/
│   │   ├── mod.rs             # build_provider() registry (4 providers)
│   │   ├── provider.rs        # trait AiProvider, generate_messages()
│   │   ├── sanitize.rs        # sanitize, deduplicate, wrap
│   │   └── providers/
│   │       ├── mod.rs
│   │       ├── openai_compat.rs  # base compartilhada OpenAI-compatible
│   │       ├── claude.rs         # Anthropic API nativa
│   │       ├── gemini.rs         # Google (via openai_compat)
│   │       ├── chatgpt.rs        # OpenAI (via openai_compat)
│   │       └── openrouter.rs     # OpenRouter (via openai_compat)
│   └── commands/
│       ├── mod.rs
│       ├── commit.rs          # fluxo principal + action menu + file picker
│       ├── config.rs          # get/set/list (merged config)
│       ├── setup.rs           # wizard global + run_local() per-repo
│       └── hook.rs            # install/uninstall prepare-commit-msg
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

### Fase 16 — Git Hook Integration [DONE]
- [x] `src/commands/hook.rs` — install/uninstall com marker detection e --force
- [x] Hook script `prepare-commit-msg` — fail-safe (|| true), skips merge/amend, checks forged in PATH
- [x] Flag `--hook <file>` (hidden) — modo headless que escreve mensagem no arquivo
- [x] Subcomando `forged hook install/uninstall` com `--force`
- [x] `hook_file` field no `CommitOpts` + early return no `run()`

**Testes novos:**
- hook.rs (6 unit): script content, marker, shell validity, fail-safe, merge/amend skip, detection
- tests/git_hook.rs (6 integration): install executable, uninstall, refuse non-forged, force overwrite, refuse uninstall non-forged, reinstall update

---

### Fase 17 — Novos Providers: ChatGPT + OpenRouter [DONE]

#### 17a — OpenAI-Compatible Base [DONE]
- [x] `src/ai/providers/openai_compat.rs` — struct genérica `OpenAiCompatProvider` parametrizada por `OpenAiCompatConfig`
- [x] Structs serde compartilhadas (Message, RequestBody, Choice, ApiResponse, ApiError)
- [x] Error handling parametrizado: `invalid_key_statuses`, `extra_headers`, provider name em mensagens
- [x] Construtores: `new(api_key, config)` e `with_base_url(api_key, base_url, config)` para testing

#### 17b — Gemini Refactor [DONE]
- [x] `gemini.rs` reduzido de ~170 linhas para ~25 linhas de factory functions
- [x] 6 testes existentes preservados como regressão

#### 17c — Provider ChatGPT (OpenAI) [DONE]
- [x] `src/ai/providers/chatgpt.rs` — `api.openai.com/v1`, default model `gpt-4o`, timeout 30s
- [x] Modelos: gpt-4o, gpt-4o-mini, o3-mini

#### 17d — Provider OpenRouter [DONE]
- [x] `src/ai/providers/openrouter.rs` — `openrouter.ai/api/v1`, default model `anthropic/claude-sonnet-4-6`, timeout 60s
- [x] Header `http-referer` para app attribution
- [x] Modelos: anthropic/claude-sonnet-4-6, google/gemini-2.5-flash, openai/gpt-4o

#### 17e — Registry + Setup Wizard [DONE]
- [x] `build_provider()` com 4 match arms (claude, gemini, chatgpt, openrouter)
- [x] `PROVIDER_LIST` atualizado com 4 providers no wizard

#### 17f — Improved Error Handling [DONE]
- [x] 401/403 agora lê response body e mostra mensagem real da API (ex: "User not found")
- [x] Fallback para mensagem genérica se body vazio

**Testes novos (24):**
- openai_compat.rs: 7 (parse, 429, api_error, empty_choices, custom_statuses, 401, extra_headers)
- chatgpt.rs: 6 (name, model, timeout, bearer_auth, 401, parse_response)
- openrouter.rs: 5 (name, model, timeout, referer_header, parse_response)
- ai/mod.rs: +4 (build_chatgpt, build_openrouter, no_key variants)
- setup.rs: +2 (find_chatgpt, find_openrouter)

---

### Fase 18 — Config Local (Per-Repo) [DONE]

#### 18a — Directory Structure + Migration [DONE]
- [x] `~/.forged/` diretório substitui `~/.forged` arquivo
- [x] `~/.forged/global` — config global
- [x] `~/.forged/locals/<profile>` — overrides por repo
- [x] Migração silenciosa: arquivo antigo → `~/.forged/global` automaticamente
- [x] `ensure_config_dir()` / `ensure_config_dir_at()` com migration logic

#### 18b — Override Parcial + Merge [DONE]
- [x] `apply_map()` extraído de `load_from()` — reuso interno zero duplicação
- [x] `apply_overrides_from(path)` — lê INI, aplica só campos presentes
- [x] `save_diff_to(path, base)` — salva só campos que diferem do global
- [x] `Config::load()` — resolução completa: global → detect repo → merge local
- [x] `Config::load_global()` — só global (para setup/config set)
- [x] `Config::load_with_source()` — retorna config + profile name ativo

#### 18c — Git Helpers [DONE]
- [x] `try_repo_root()` — detecção não-falível do repo root
- [x] `repo_name()` — extrai nome do diretório do path

#### 18d — Comando `forged setup local` [DONE]
- [x] `forged setup local` — wizard interativo para config por repo
- [x] Detecta repo via `assert_git_repo()`, usa nome do diretório como profile
- [x] Wizard: provider, api_key (opcional, herda global), model, commit_type, locale
- [x] Salva overrides em `~/.forged/locals/<profile>`
- [x] Cria `.forged` na root do repo com nome do profile
- [x] Sugere adicionar `.forged` ao `.gitignore`

#### 18e — Call Sites Atualizados [DONE]
- [x] `commands/config.rs`: `run_set` usa `load_global()`/`save_global()`, `run_get`/`run_list` usam `load()` (merged)
- [x] `config list` mostra `# profile: <name>` quando local ativo
- [x] `commands/setup.rs`: `run()` salva em `~/.forged/global`
- [x] `main.rs`: `Setup { scope: Option<SetupScope> }` com `SetupScope::Local`

**Testes novos (13):**
- config.rs: 9 (apply_map_partial, validation, overrides_empty, overrides_nonexistent, save_diff, save_diff_empty, migration_file, migration_dir, load_merge)
- git.rs: 3 (try_repo_root, repo_name, repo_name_root)
- setup.rs: 1 (atualizado labels_match_count)

---

### Fase 18f — Config Management UX [DONE]

#### Remover config local [DONE]
- [x] `forged setup local --remove` — remove profile + `.forged` pointer
- [x] `remove_local_profile()` / `remove_local_profile_at()` em `config.rs`
- [x] Mensagem clara se não há config local

#### Listar e reusar profiles [DONE]
- [x] `forged setup local --list` — lista profiles com paths
- [x] `forged setup local --use [PROFILE]` — reusar profile existente (picker interativo se nome omitido)
- [x] `list_profiles()` / `list_profiles_at()` em `config.rs`
- [x] `profile_exists()` em `config.rs`

#### Config list interativo [DONE]
- [x] `forged config list` — Select interativo entre Global + profiles locais
- [x] Merged view: campos com tags `[local]`, `[global]`, `[default]`
- [x] Campos locais destacados em verde
- [x] Clear screen entre iterações + `Press Enter to go back`
- [x] `Esc` para sair do menu
- [x] `ConfigSource` enum + `ConfigWithSources` struct
- [x] `Config::load_with_sources()` / `load_with_sources_at()` — carrega com source tracking
- [x] Path functions (`config_dir`, `global_config_path`, `locals_dir`, `local_config_path`) agora `pub(crate)`

#### CLI [DONE]
- [x] `SetupScope::Local` de unit variant para struct variant com `--remove`, `--use`, `--list`
- [x] Dispatch atualizado em `main.rs`

**Testes novos (6):**
- config.rs: `list_profiles_empty`, `list_profiles_returns_names`, `remove_local_profile_deletes`, `remove_local_profile_nonexistent_ok`, `load_with_sources_global_only`, `load_with_sources_with_local_overrides`

---

### Fase 19 — Multi-Provider com Failover [DONE]

#### 19a — AiError Enum + Trait Update [DONE]
- [x] `AiError` enum: `Retryable` (429, timeout, 5xx), `ProviderFatal` (401/403), `Fatal` (parse error)
- [x] `AiProvider::complete()` retorna `Result<String, AiError>` em vez de `anyhow::Result`
- [x] `AiError::should_failover()` helper
- [x] `impl Display + Error` para `AiError`

#### 19b — Providers Atualizados [DONE]
- [x] `claude.rs`: classifica erros na origem (timeout→Retryable, 401→ProviderFatal, parse→Fatal)
- [x] `openai_compat.rs`: mesma classificação (cobre gemini, chatgpt, openrouter)
- [x] Todos os 24+ testes de providers passando sem mudança de assertions

#### 19c — generate_messages/description com AiError [DONE]
- [x] `generate_messages()` retorna `Result<Vec<String>, AiError>`
- [x] `generate_description()` retorna `Result<String, AiError>`
- [x] `Fatal` propaga imediato, `Retryable`/`ProviderFatal` preserva último erro
- [x] Mocks em unit tests e integration tests atualizados

#### 19d — Config Multi-Provider [DONE]
- [x] `ProviderEntry` struct (name, api_key, model)
- [x] `Config.fallback_providers: Vec<ProviderEntry>` — máx 3 fallbacks (4 total)
- [x] Formato INI com seções: `[provider.claude]`, `[provider.gemini]`
- [x] `ParsedIni` struct + `parse_ini_sections()` — parser com suporte a seções
- [x] `apply_parsed()` — carrega formato novo (`providers=`) e legado (`provider=`)
- [x] `serialize_parsed()` — sempre serializa com seções
- [x] `save_to()` / `save_diff_to()` atualizados para formato com seções
- [x] Backwards compat: leitura do formato legado sem seções funciona
- [x] Validação: nomes de provider válidos, máx 4 providers

#### 19e — build_providers() + Failover Logic [DONE]
- [x] `ProviderWithOpts` struct (provider, model, timeout)
- [x] `FailoverReport` / `FailoverFailure` structs
- [x] `build_provider_from_entry(name, api_key)` — factory extraída
- [x] `build_providers(config)` → `Vec<ProviderWithOpts>` (primary + fallbacks)
- [x] `generate_messages_with_failover()` — tenta providers em ordem, Fatal para imediato
- [x] `generate_description_with_failover()` — mesma lógica

#### 19f — Commit Flow + UI [DONE]
- [x] `commit.rs` usa `build_providers()` + failover functions
- [x] `generate_full_messages()` retorna `(Vec<String>, FailoverReport)`
- [x] Log: `:: Generated with gemini-2.5-flash (fallback: claude failed — rate limit)`
- [x] `print_failover_report()` — exibe provider usado e falhas
- [x] model/timeout resolvidos per-provider (não mais globais)
- [x] Regenerate e Settings usam failover
- [x] Setup wizard: "Add a fallback provider?" com loop para até 3 fallbacks
- [x] `run_local()` com fallback providers
- [x] `config list` mostra fallback providers na tabela

**Testes novos (17):**
- config.rs: 8 (parse_ini_with_sections, apply_multi_provider, apply_single_provider_backwards_compat, save_roundtrip_multi_provider, save_single_provider_uses_sections, save_diff_multi_provider, max_four_providers_validation, invalid_provider_name_validation)
- ai/provider.rs: 6 (failover_to_second_on_retryable, failover_report_tracks_failures, fatal_stops_failover, single_provider_no_failover, all_providers_fail, description_failover)
- ai/mod.rs: 3 (build_providers_single, build_providers_with_fallback, build_providers_fallback_default_model)

---

## Contagem de Testes Atual

| Módulo | Testes |
|---|---|
| config.rs | 32 |
| git.rs | 12 |
| prompt.rs | 9 |
| ai/sanitize.rs | 17 |
| ai/provider.rs | 11 |
| ai/providers/openai_compat.rs | 7 |
| ai/providers/claude.rs | 6 |
| ai/providers/gemini.rs | 6 |
| ai/providers/chatgpt.rs | 6 |
| ai/providers/openrouter.rs | 5 |
| ai/mod.rs | 12 |
| commands/setup.rs | 11 |
| commands/commit.rs | 8 |
| commands/hook.rs | 6 |
| commands/config.rs | 4 |
| commands/upgrade.rs | 4 |
| clipboard.rs | 2 |
| main.rs (CLI) | 4 |
| **Total unitários** | **162** |

### Testes de Integração (`tests/`)

| Arquivo | Testes |
|---|---|
| tests/git_commit.rs | 5 |
| tests/full_flow.rs | 3 |
| tests/generate_pipeline.rs | 4 |
| tests/provider_http.rs | 4 |
| tests/git_hook.rs | 6 |
| **Total integração** | **22** |

| **Total geral** | **184 (todos passando)** |

---

## Pendências

### Futuro

#### LLMs locais
- [ ] Provider `local` via endpoint OpenAI-compatible (Ollama, llama.cpp, LM Studio, vLLM)
- [ ] Config: `local_endpoint=http://localhost:11434/v1` (default Ollama)
- [ ] Auto-detect: tentar `http://localhost:11434` se nenhum provider cloud configurado
- [ ] Listar modelos disponíveis via `/v1/models` no setup wizard
- [ ] Zero API key necessária — funciona offline

#### Outros
- [ ] Large diff chunking (>50 files → chunks de 10 → combine)

---

## Regras de Qualidade

1. **Nenhum `unwrap()` fora de testes** — todo erro propagado com `?` ou `anyhow::Context`
2. **Nenhum `clone()` desnecessário** — referências onde possível
3. **HTTP só nos providers** — `reqwest::Client` nunca vaza
4. **Testes não dependem de internet** — `mockito` + `tempfile`
5. **`cargo clippy -- -D warnings`** — zero warnings
