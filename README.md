# Nyrui

**Nyrui** (`nyrui`) is a declarative, component-based Terminal User Interface (TUI) and dual-interface (TUI + CLI) application framework for Rust, built on top of [`ratatui`](https://github.com/ratatui/ratatui) and [`crossterm`](https://github.com/crossterm-rs/crossterm).

It is designed for medium-to-large terminal applications that require retained component state, fine-grained reactivity, structured keyboard/mouse event routing, modal overlays, typed forms, and a shared action layer capable of exposing domain operations simultaneously as interactive TUI views and headless CLI subcommands.

---

## Core Architecture

Nyrui separates **component state and reconciliation** from **immediate-mode terminal cell drawing**:

1. **Declarative Surface (`ui!` & Builders):** A procedural macro (`ui!`) with native Rust control flow (`if`/`else`, `match`, `for`) and a 1:1 equivalent typed builder API.
2. **UI Description Layer (`UiNode`):** Short-lived, statically typed node trees with explicit and positional scoped keys (`NodeKey`).
3. **Single-Threaded Arena Runtime (`nyrui-core`):** Generational `SlotMap<NodeId, InstanceNode>` managing component lifecycles, local state, and top-down dirty-node reconciliation without `Arc<Mutex<...>>` overhead.
4. **Hybrid Reactivity (`Signal<T>` & `Computed<T>`):** Automatic dependency tracking during `render()` combined with manual component invalidation and batched frame scheduling.
5. **Ratatui Rendering & Escape Hatches:** Single-pass layout resolution (`Rect` caching for spatial mouse hit-testing) and zero-cost `NativeWidget` / `CanvasDraw` escape hatches to raw `ratatui::Frame`.
6. **Dual TUI + CLI Action Layer (`nyrui-cli`):** Define typed `Action` implementations once with shared `Validate` domain rules and execute them interactively from TUI components or headlessly via `clap`-backed CLI subcommands.

---

## Architecture Specifications (RFCs)

All architectural decisions and contracts are documented in [`docs/rfcs/`](docs/rfcs/):

| RFC | Title | Status |
| --- | ----- | ------ |
| [RFC-0001](docs/rfcs/0001-declarative-ui-runtime.md) | Declarative Architecture and Runtime for Nyrui | Accepted |
| [RFC-0002](docs/rfcs/0002-dual-interface-architecture.md) | Dual-Interface Architecture (TUI + CLI Action Layer) | Accepted |

---

## Workspace Layout

```text
nyrui/
├── Cargo.toml                 # Workspace root, shared lints (#![forbid(unsafe_code)])
├── crates/
│   ├── nyrui-core/            # Arena runtime, reconciler, reactivity, events, layout, theme
│   ├── nyrui-macros/          # (Planned) Proc-macros: ui!, #[component], #[derive(Props)], #[action]
│   ├── nyrui-widgets/         # (Planned) Primitive and composite TUI widgets
│   ├── nyrui-cli/             # (Planned) Headless CLI adapter and clap integration
│   └── nyrui/                 # (Planned) Public facade crate
├── docs/
│   └── rfcs/                  # Architectural Request for Comments (RFCs)
└── .githooks/                 # Shared Git hooks (pre-commit, commit-msg, pre-push)
```

---

## Development Setup

### Prerequisites

- **Rust:** `1.85+` (Edition 2024)
- `rustfmt` and `clippy` components installed (`rustup component add rustfmt clippy`)

### Enable Git Hooks

This repository tracks shared Git hooks in [`.githooks/`](.githooks/) to enforce formatting, `clippy` lints, Conventional Commits, and test runs locally before pushing. Configure Git to use them once after cloning:

```bash
git config core.hooksPath .githooks
```

### Common Commands

```bash
# Check formatting across the workspace
cargo fmt --all -- --check

# Run Clippy with warnings denied
cargo clippy --workspace --all-targets --all-features -- -D warnings

# Run all unit, reconciliation, and integration tests
cargo test --workspace --all-targets --all-features
```

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for branch conventions, issue workflow, Conventional Commits format, and PR requirements.

## License

Dual-licensed under either [MIT](https://opensource.org/licenses/MIT) or [Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0) at your option.
