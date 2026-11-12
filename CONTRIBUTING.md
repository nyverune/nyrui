# Contributing to Nyrui

Thank you for contributing to **Nyrui**. Because Nyrui is a foundational framework with strict ownership, lifetime, and architectural invariants, all contributors follow a coordinated workflow anchored by our [RFC specifications](docs/rfcs/).

---

## 1. Initial Repository Setup

After cloning the repository, activate the tracked Git hooks so formatting, lint checks, and commit message validation run automatically:

```bash
git config core.hooksPath .githooks
chmod +x .githooks/*
```

Verify your toolchain (`rustc 1.85+`, Edition 2024):

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
```

---

## 2. Issue & RFC Workflow

1. **No Unscoped Code Drops:** Every pull request must link to an open Issue (`Closes #123` or `Part of #123`).
2. **Follow the RFCs:**
   - [RFC-0001: Declarative Architecture and Runtime](docs/rfcs/0001-declarative-ui-runtime.md)
   - [RFC-0002: Dual-Interface Architecture (TUI + CLI)](docs/rfcs/0002-dual-interface-architecture.md)
3. **Architectural Changes:** If you discover an edge case requiring a change to core traits (`Component`, `Action`, `UiNode`, `Signal`) or crate boundaries, open a **Feature / RFC Proposal** issue or a PR updating the relevant RFC before implementing large changes.

---

## 3. Branching & Commit Conventions

### Branch Naming

Use descriptive prefixes matching the work type:

- `feat/<scope>-<short-description>` (e.g., `feat/runtime-generational-arena`)
- `fix/<scope>-<short-description>` (e.g., `fix/reconciler-keyed-reorder`)
- `refactor/<scope>-<short-description>`
- `docs/<short-description>`
- `test/<scope>-<short-description>`

### Conventional Commits (Enforced by `.githooks/commit-msg`)

All commit titles must follow the [Conventional Commits](https://www.conventionalcommits.org/) specification:

```text
<type>(<optional-scope>): <concise imperative summary>
```

- **Allowed `<type>` values:** `feat`, `fix`, `refactor`, `perf`, `docs`, `test`, `build`, `ci`, `chore`, `style`.
- **Recommended `<scope>` values:** `core`, `runtime`, `reconciler`, `reactivity`, `events`, `focus`, `layout`, `theme`, `overlays`, `router`, `forms`, `macros`, `widgets`, `cli`, `rfcs`.
- **Examples:**
  - `feat(runtime): implement SlotMap instance arena and NodeId generation`
  - `feat(reactivity): add automatic dependency tracking stack to Signal::get`
  - `test(reconciler): verify state preservation across keyed permutations`

---

## 4. Engineering & Code Quality Standards

- **`#![forbid(unsafe_code)]`:** Enforced workspace-wide via `Cargo.toml`. Do not use `unsafe`. Use generational indices (`NodeId`, `SignalId`) and the arena take-and-restore pattern (`Option<Box<dyn ErasedComponent>>`) instead of raw pointers.
- **No Unhandled Panics in Library Code:** Avoid `.unwrap()` in runtime paths (`clippy::unwrap_used` is enabled). Use structured `Result` types or `.expect("explicit invariant explanation")` only when an internal invariant is mathematically guaranteed.
- **Testing Expectations:**
  - **Runtime / Reconciler changes:** Must include deterministic unit tests verifying mount, update, reorder, and unmount behavior.
  - **Macro changes (`nyrui-macros`):** Must include both happy-path tests and `trybuild` compile-fail diagnostics tests.
  - **Visual / Widget changes:** Must include headless `ratatui::backend::TestBackend` tests rather than relying on manual interactive inspection.
- **Documentation:** All public structs, traits, enums, and functions must include concise rustdoc comments explaining their contract.

