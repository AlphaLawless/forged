# Migração: inquire → ratatui

## Status: CONCLUÍDA ✓

Todas as fases implementadas. `inquire` removido. `cargo fmt`, `cargo clippy -- -D warnings`
e `cargo test` passando limpos. 214 unit tests + 26 integration tests.

---

## Arquitetura entregue

```
src/
├── vim/                      # motor vim-like — zero deps de TUI
│   ├── mod.rs                # re-exports: Buffer, VimKey, BufferEvent, Mode, Cursor
│   ├── buffer.rs             # Buffer + Cursor + Mode + Snapshot (undo)
│   ├── motion.rs             # funções puras de cálculo de posição
│   └── command.rs            # mutações: insert, delete, open_line...
│
└── tui/
    ├── mod.rs                # run_with<S,R> — lifecycle + event loop
    ├── theme.rs              # paleta de cores centralizada
    ├── widgets/
    │   ├── select.rs         # SelectState<T> — j/k, Enter, Esc, g/G, hints
    │   ├── multi_select.rs   # MultiSelectState<T> — Space, a, n, / filtro
    │   └── text_input.rs     # TextInputState — cursor completo + run_masked()
    └── views/
        ├── action_menu.rs    # mensagem em box + lista de ações genérica
        ├── file_picker.rs    # MultiSelect para staging de arquivos
        └── editor.rs         # adaptador VimKey ↔ crossterm + render ratatui
```

**Regra de dependência:** `src/vim/` nunca importa de `src/tui/`. Único ponto de
cruzamento: `editor.rs` via `to_vim_key(crossterm::KeyEvent) → VimKey`.

---

## Fases

### Fase 1 — Infraestrutura TUI [DONE]

- [x] `ratatui = "0.30"` e `crossterm = "0.29"` adicionados ao `Cargo.toml`
- [x] `src/tui/mod.rs` — `run_with<S, R>()` com restore garantido via closure
- [x] `src/tui/theme.rs` — `PRIMARY`, `SELECTED_BG`, `DIM`, `SUCCESS`, `ERROR`, `BORDER`
- [x] `src/lib.rs` — `pub mod tui` adicionado

---

### Fase 2 — Widget `select` + Action Menu [DONE]

- [x] `src/tui/widgets/select.rs` — `SelectState<T>`, `SelectItem<T>`, `SelectAction<T>`
  - j/k + ↑/↓, g/G, Enter, Esc/q, hint dim à direita
  - `run<T>(title, items, starting_idx)` standalone
- [x] `src/tui/views/action_menu.rs` — mensagem em box rounded + lista de ações
- [x] `commands/commit.rs` — `action_menu()` e `pick_message()` migrados
- [x] 11 testes novos em `select.rs`

---

### Fase 3 — Widget `multi_select` + File Picker [DONE]

- [x] `src/tui/widgets/multi_select.rs` — `MultiSelectState<T>`, Space/a/n/g/G, filtro `/`
- [x] `src/tui/views/file_picker.rs` — todos os arquivos pré-selecionados
- [x] `commands/commit.rs` — `offer_stage_changes()` migrado
- [x] 10 testes novos em `multi_select.rs`

---

### Fase 4 — Widget `text_input` + Settings Menu [DONE]

- [x] `src/tui/widgets/text_input.rs` — cursor completo (←/→, Ctrl+A/E, Delete), `run_masked()`
- [x] `commands/commit.rs` — `settings_menu()` migrado com `SelectItem::with_hint()` mostrando valores atuais
- [x] 8 testes novos em `text_input.rs`

---

### Fase 5 — Setup Wizard [DONE]

- [x] `text_input::run_masked()` via `TextInputState::with_masked()` — exibe `•` por char
- [x] `commands/setup.rs` — todas as chamadas `inquire::{Select, Text, Password, Confirm}` migradas
  - `pick_provider`, `pick_api_key`, `pick_model`, `pick_commit_type`, `pick_locale`
  - `collect_fallback_providers` — Yes/No via `select::run`
  - `use_profile` — picker interativo via `select::run`
- [x] `exit_cancelled() -> !` — Esc imprime "Cancelled." e sai com código 0

---

### Fase 6 — Config Browser [DONE]

- [x] `commands/config.rs` — `run_list()` migrado para `select::run`
- [x] `use inquire::Select` removido

---

### Fase 7 — Limpeza [DONE]

- [x] `inquire` removido do `Cargo.toml`
- [x] `open_in_editor()` substituído por `editor::run()` inline
- [x] `cargo clippy -- -D warnings` — zero warnings
- [x] `cargo fmt --all --check` — sem diff
- [x] `test_select_G_jumps_to_last` renomeado para `test_select_capital_g_jumps_to_last` (snake_case)

---

### Fase 8 — Motor vim-like (`src/vim/`) [DONE]

- [x] `src/vim/buffer.rs` — `Buffer::apply(VimKey) → BufferEvent`
  - Modos: `Normal` / `Insert`
  - Sequência `dd`, histórico de undo (50 entradas), `pending_d`
- [x] `src/vim/motion.rs` — funções puras: `left`, `right_normal`, `up`, `down`,
  `up_insert`, `down_insert`, `line_start`, `line_end`, `line_end_insert`,
  `word_forward`, `word_backward`
- [x] `src/vim/command.rs` — `insert_char`, `insert_newline`, `backspace`,
  `delete_char`, `delete_line`, `open_line_below`, `open_line_above`
- [x] `src/tui/views/editor.rs` — adaptador único crossterm ↔ vim ↔ ratatui
  - Borda muda de cor: `BORDER` (NORMAL) → `PRIMARY` (INSERT)
  - `[NORMAL]` / `[INSERT]` na borda inferior
- [x] `commands/commit.rs` — `Action::Edit` usa `editor::run()`, edição inline
  - Esc no editor → volta ao action menu sem mudança
  - Enter em Normal → `current_messages = vec![edited]`, loop exibe versão editada
- [x] 27 testes novos em `src/vim/` (10 buffer, 8 motion, 9 command)

---

## Contagem final de testes

| Módulo | Testes |
|---|---|
| `tui/widgets/select.rs` | 11 |
| `tui/widgets/multi_select.rs` | 10 |
| `tui/widgets/text_input.rs` | 8 |
| `vim/buffer.rs` | 10 |
| `vim/motion.rs` | 8 |
| `vim/command.rs` | 9 |
| Demais módulos (config, git, ai, etc.) | 158 |
| **Total unit** | **214** |
| **Total integration** | **26** |
| **Total geral** | **240** |

---

## Navegação padrão implementada

| Tecla | Select | MultiSelect | Editor (Normal) | Editor (Insert) |
|---|---|---|---|---|
| `j` / `↓` | próximo | próximo | linha abaixo | linha abaixo |
| `k` / `↑` | anterior | anterior | linha acima | linha acima |
| `h` / `←` | — | — | cursor ← | cursor ← |
| `l` / `→` | — | — | cursor → | cursor → |
| `g` / `Home` | primeiro | primeiro | — | — |
| `G` / `End` | último | último | — | — |
| `0` | — | — | início da linha | — |
| `$` | — | — | fim da linha | — |
| `w` | — | — | próxima palavra | — |
| `b` | — | — | palavra anterior | — |
| `Enter` | confirma | confirma | `Confirmed` | quebra linha |
| `Esc` / `q` | cancela | cancela | `Cancelled` | → NORMAL |
| `Space` | — | toggle | — | — |
| `a` | — | selecionar todos | → INSERT (após) | — |
| `A` | — | — | → INSERT (fim) | — |
| `i` | — | — | → INSERT (antes) | — |
| `o` / `O` | — | — | nova linha | — |
| `x` | — | — | apaga char | — |
| `dd` | — | — | apaga linha | — |
| `u` | — | — | undo | — |
| `n` | — | desselecionar | — | — |
| `/` | — | filtro | — | — |

---

## O que NÃO mudou (confirmado)

| Item | Status |
|---|---|
| Toda lógica de geração AI | Intacta |
| Config parsing/saving | Intacto |
| Git operations | Intacto |
| `headless mode` (GIT_PARAMS / !atty) | Intacto |
| `--yes` flag (skip confirm) | Intacto |
| `--hook <file>` (git hook headless) | Intacto |
| Todos os testes de integração | Passando (26/26) |
