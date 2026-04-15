# Migração: inquire → ratatui

## Objetivo

Substituir o `inquire` por `ratatui` + `crossterm` em todos os pontos de interação do
`forged`, adotando a separação de responsabilidades do claude-code-rust:

- `src/tui/` — renderização pura (leitura de estado, zero mutações)
- Estado encapsulado em structs dedicadas por view
- Navegação vim-like (`j/k`, `Enter`, `Esc`, `Space`)
- Sem setinhas como único modo de navegação

Referência de arquitetura: `study/claude-code-rust/src/{app,ui}/`

---

## Inventário do inquire atual

| Componente | Arquivo | inquire API usada |
|---|---|---|
| Action menu pós-geração | `commands/commit.rs` | `Select` |
| File picker (no staged) | `commands/commit.rs` | `MultiSelect` |
| Settings sub-menu | `commands/commit.rs` | `Select` + `Text` |
| Setup wizard global | `commands/setup.rs` | `Select`, `Text`, `Password` |
| Setup local (per-repo) | `commands/setup.rs` | `Select`, `Text`, `Password` |
| Config list interativo | `commands/config.rs` | `Select` |
| Editor (edit message) | `commands/commit.rs` | `Editor` → mantém $EDITOR externo |

**Total de call sites para migrar:** ~14 pontos de uso do inquire.

---

## Arquitetura alvo

```
src/
├── tui/
│   ├── mod.rs            # terminal lifecycle: init, restore, run_with
│   ├── theme.rs          # cores e estilos centralizados
│   ├── events.rs         # loop de eventos crossterm
│   ├── widgets/
│   │   ├── mod.rs
│   │   ├── select.rs     # lista vertical com j/k + Enter/Esc
│   │   ├── multi_select.rs  # lista com Space para toggle + a/n
│   │   ├── text_input.rs    # linha de texto inline (para settings)
│   │   └── password_input.rs  # máscara de senha
│   └── views/
│       ├── mod.rs
│       ├── action_menu.rs   # menu pós-geração
│       ├── file_picker.rs   # staging de arquivos
│       ├── settings.rs      # session settings
│       ├── setup_wizard.rs  # wizard global + local
│       └── config_list.rs   # browser de configs
```

### Padrão de separação

```
State struct (puro dado)
    ↓ mutado por
Event handler (recebe KeyEvent → muta estado → retorna Action)
    ↓ Action processada por
Caller (commit.rs, setup.rs, config.rs)
    ↓ estado lido por
render() (leitura pura, zero mutações)
```

Cada view segue o mesmo contrato:

```rust
pub struct FooState { /* campos */ }
pub enum FooAction { Done(T), Cancel, /* outros */ }

impl FooState {
    pub fn new(/* params */) -> Self { ... }
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<FooAction> { ... }
}

pub fn render(frame: &mut Frame, area: Rect, state: &FooState) { ... }

pub fn run(/* params */) -> anyhow::Result<Option<T>> {
    // init terminal, loop events, draw, restore terminal
}
```

---

## Navegação padrão (todos os componentes)

| Tecla | Ação |
|---|---|
| `j` / `↓` | próximo item |
| `k` / `↑` | item anterior |
| `g` / `Home` | primeiro item |
| `G` / `End` | último item |
| `Enter` | confirmar seleção |
| `Esc` / `q` | cancelar / voltar |
| `Space` | toggle (só MultiSelect) |
| `a` | selecionar todos (só MultiSelect) |
| `n` | desselecionar todos (só MultiSelect) |
| `/` | filtro de busca (só MultiSelect com muitos itens) |

---

## Fases

### Fase 1 — Infraestrutura TUI (não-breaking)

**Arquivos criados:** `src/tui/mod.rs`, `src/tui/theme.rs`, `src/tui/events.rs`

**Cargo.toml — adicionar:**
```toml
ratatui = "0.30"
crossterm = { version = "0.29", features = ["event-stream"] }
```

**Cargo.toml — manter por enquanto:**
```toml
inquire = { version = "0.7", features = ["editor"] }
```

**`src/tui/mod.rs`:**
- `pub fn init() -> Terminal<CrosstermBackend<Stdout>>`
- `pub fn restore(terminal: Terminal<...>)`
- `pub fn run_with<S, A>(state: S, render_fn, handle_fn) -> anyhow::Result<Option<A>>`

**`src/tui/theme.rs`:**
- Paleta de cores: `PRIMARY`, `SELECTED`, `DIM`, `ERROR`, `SUCCESS`
- Estilos reutilizáveis: `selected_style()`, `normal_style()`, `hint_style()`

**`src/tui/events.rs`:**
- `pub fn next_event(timeout: Duration) -> anyhow::Result<Option<Event>>`
- Loop de eventos com drain não-bloqueante

**Testes:** Nenhum novo (infraestrutura pura). Todos os 184 testes existentes devem continuar passando.

---

### Fase 2 — Widget `select` + Action Menu

**Arquivos criados:** `src/tui/widgets/select.rs`, `src/tui/views/action_menu.rs`

**`SelectState<T>`:**
```rust
pub struct SelectState<T> {
    pub items: Vec<SelectItem<T>>,
    pub selected: usize,
    pub title: String,
    pub filter: String,       // busca inline opcional
}

pub struct SelectItem<T> {
    pub label: String,
    pub value: T,
    pub hint: Option<String>, // texto dim à direita
}

pub enum SelectAction<T> {
    Picked(T),
    Cancelled,
}
```

**`src/tui/views/action_menu.rs`:**
- `pub fn run(messages: &[String]) -> anyhow::Result<Option<CommitAction>>`
- Exibe a mensagem gerada no topo (em bloco com borda)
- Lista de ações abaixo: Commit / Edit / Regenerate / Settings / Cancel
- Highlight do item selecionado com cor `SELECTED` + seta `❯`

**Migrar em:** `commands/commit.rs` — substituir a chamada ao `inquire::Select` do action menu.

**Testes novos (widget):**
- `test_select_state_j_moves_down`
- `test_select_state_k_moves_up`
- `test_select_state_wraps_at_bottom`
- `test_select_state_enter_returns_picked`
- `test_select_state_esc_returns_cancelled`
- `test_select_state_g_jumps_to_top`

---

### Fase 3 — Widget `multi_select` + File Picker

**Arquivos criados:** `src/tui/widgets/multi_select.rs`, `src/tui/views/file_picker.rs`

**`MultiSelectState<T>`:**
```rust
pub struct MultiSelectState<T> {
    pub items: Vec<MultiSelectItem<T>>,
    pub cursor: usize,
    pub filter: String,
    pub filtered_indices: Vec<usize>,  // calculado após cada keystroke
}

pub struct MultiSelectItem<T> {
    pub label: String,
    pub value: T,
    pub checked: bool,
}

pub enum MultiSelectAction<T> {
    Confirmed(Vec<T>),   // itens marcados
    Cancelled,
}
```

**Rendering:**
- `[x] src/main.rs` (marcado, cursor) em verde
- `[ ] src/lib.rs` (desmarcado) normal
- `❯ [ ] src/config.rs` (cursor atual) com fundo highlight
- Footer: `space=toggle  a=all  n=none  enter=confirm  esc=cancel`
- Campo `/` filtra por substring (atualiza `filtered_indices`)

**`src/tui/views/file_picker.rs`:**
- `pub fn run(files: &[String]) -> anyhow::Result<Option<Vec<String>>>`
- Todos os itens pré-selecionados (comportamento atual preservado)

**Migrar em:** `commands/commit.rs` — bloco `if unstaged.is_empty()`.

**Testes novos:**
- `test_multiselect_space_toggles`
- `test_multiselect_a_selects_all`
- `test_multiselect_n_deselects_all`
- `test_multiselect_filter_reduces_visible`
- `test_multiselect_confirm_returns_checked`
- `test_multiselect_empty_confirm_ok`

---

### Fase 4 — Widget `text_input` + Settings Menu

**Arquivos criados:** `src/tui/widgets/text_input.rs`, `src/tui/views/settings.rs`

**`TextInputState`:**
```rust
pub struct TextInputState {
    pub value: String,
    pub cursor: usize,
    pub label: String,
    pub hint: Option<String>,
    pub validator: Option<Box<dyn Fn(&str) -> bool>>,
    pub error: Option<String>,
}
```

**Teclas no text_input:**
- Caracteres alfanuméricos → inserir
- `Backspace` → apagar
- `←`/`→` → mover cursor
- `Ctrl+A` → início, `Ctrl+E` → fim
- `Enter` → confirmar (chama validator)
- `Esc` → cancelar

**`src/tui/views/settings.rs`:**
- Sub-menu com 4 campos: locale, commit_type, max_length, generate
- Primeiro: `Select` para escolher qual campo editar
- Segundo: `TextInput` ou `Select` dependendo do campo
- Retorna `SessionConfig` atualizado

**Migrar em:** `commands/commit.rs` — `settings_menu()`.

**Testes novos:**
- `test_text_input_typing`
- `test_text_input_backspace`
- `test_text_input_enter_validates`
- `test_text_input_esc_cancels`
- `test_settings_view_returns_updated_session`

---

### Fase 5 — Widget `password_input` + Setup Wizard

**Arquivos criados:** `src/tui/widgets/password_input.rs`, `src/tui/views/setup_wizard.rs`

**`PasswordInputState`:** idêntico ao `TextInputState` mas `render()` exibe `*` por caractere.

**`src/tui/views/setup_wizard.rs`:**

O wizard tem múltiplos passos sequenciais. Cada passo é uma view própria:

```
Passo 1: Select provider
Passo 2: PasswordInput API key
Passo 3: Select model (filtrado por provider)
Passo 4: Select commit type
Passo 5: Confirm "Add fallback provider?" (Select com [Yes, No])
  → Se Yes: repete passos 1-3 para fallback (máx 3x)
Passo 6: Resumo + salvar
```

**`WizardState`:**
```rust
pub struct WizardState {
    pub step: WizardStep,
    pub provider: Option<String>,
    pub api_key: String,
    pub model: Option<String>,
    pub fallbacks: Vec<ProviderEntry>,
    pub error: Option<String>,
}

pub enum WizardStep {
    SelectProvider,
    EnterApiKey,
    SelectModel,
    SelectCommitType,
    AskFallback,
    Done,
}
```

**Header fixo** em todas as telas: título do wizard + progresso (`1/5`).
**Rodapé fixo**: `enter=avançar  esc=voltar  q=cancelar`.

**Migrar em:** `commands/setup.rs` — `run()` e `run_local()`.

**Testes novos:**
- `test_wizard_step_advance`
- `test_wizard_step_back`
- `test_wizard_password_masked_render` (verifica que render não expõe chars)
- `test_wizard_fallback_loop_max_3`

---

### Fase 6 — Config Browser

**Arquivo criado:** `src/tui/views/config_list.rs`

**`ConfigBrowserState`:**
```rust
pub struct ConfigBrowserState {
    pub profiles: Vec<ProfileEntry>,
    pub cursor: usize,
    pub detail: Option<ConfigWithSources>,
}

pub struct ProfileEntry {
    pub name: String,       // "Global" | nome do profile local
    pub path: PathBuf,
}
```

**Layout em duas colunas:**
- Esquerda (30%): lista de profiles com j/k
- Direita (70%): config renderizada com source tags coloridas
  - `[local]` em verde
  - `[global]` em azul dim
  - `[default]` em cinza

**Migrar em:** `commands/config.rs` — `run_list()`.

**Testes novos:**
- `test_config_browser_navigation`
- `test_config_browser_displays_sources`

---

### Fase 7 — Limpeza

- Remover `inquire` de `Cargo.toml` (verificar que zero uses restam)
- Remover feature `editor` se `inquire::Editor` não é mais usado
  - **Nota:** `Edit message` abre `$EDITOR` diretamente via `std::process::Command` — não usa inquire. Manter esse comportamento.
- Atualizar `src/lib.rs` para re-exportar `tui` se necessário
- Atualizar `CLAUDE.md` da fase se necessário
- Rodar `cargo clippy -- -D warnings` e `cargo test`

---

## O que NÃO muda

| Item | Razão |
|---|---|
| `Edit message` → abre `$EDITOR` | Já usa `std::process::Command`, não inquire |
| Toda lógica de geração AI | Não toca em UI |
| Config parsing/saving | Sem UI |
| Git operations | Sem UI |
| Todos os testes existentes (unitários + integração) | Testam lógica, não UI |
| `headless mode` (GIT_PARAMS / !atty) | Bypass de UI, mantido |
| `--yes` flag | Bypass de UI, mantido |

---

## Critérios de aceitação

- [ ] `cargo test` — 184 testes existentes passando
- [ ] `cargo clippy -- -D warnings` — zero warnings
- [ ] Navegação j/k funciona em todos os menus
- [ ] Esc cancela em todos os menus
- [ ] File picker preserva pré-seleção de todos os arquivos
- [ ] Setup wizard preserva fallback provider loop
- [ ] Config browser mostra source tags corretamente
- [ ] `forged --yes` ainda faz commit sem UI
- [ ] `forged --hook <file>` ainda funciona em headless

---

## Ordem de execução recomendada

```
Fase 1 (infra)  →  Fase 2 (action menu)  →  Fase 3 (file picker)
→  Fase 4 (settings)  →  Fase 5 (setup wizard)  →  Fase 6 (config browser)
→  Fase 7 (limpeza)
```

Cada fase: implementar → `cargo test` → aprovar → próxima fase.
Nunca mais de 5 arquivos por resposta.

---

## Contagem de testes alvo pós-migração

| Módulo | Testes atuais | Novos | Total |
|---|---|---|---|
| tui/widgets/select.rs | — | 6 | 6 |
| tui/widgets/multi_select.rs | — | 6 | 6 |
| tui/widgets/text_input.rs | — | 4 | 4 |
| tui/widgets/password_input.rs | — | 1 | 1 |
| tui/views/setup_wizard.rs | — | 4 | 4 |
| tui/views/config_list.rs | — | 2 | 2 |
| tui/views/settings.rs | — | 1 | 1 |
| **Subtotal novos** | | **24** | **24** |
| **Total geral** | **184** | +24 | **208** |
