## Summary

<!-- Explain the technical changes introduced in this PR and why they are structured this way. -->

- **Closes / Part of:** #<!-- issue number -->
- **RFC Reference:** <!-- e.g., RFC-0001 §6.2 (Arena Storage) or N/A -->
- **Affected Crates:** <!-- e.g., `nyrui-core`, `nyrui-macros` -->

## Key Changes

- 

## Verification & Testing

<!-- Describe how these changes were verified (unit tests, trybuild macro tests, TestBackend snapshots, CLI integration tests). -->

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes
- [ ] `cargo test --workspace --all-targets --all-features` passes
- [ ] New public types and traits are documented and covered by unit/integration tests
- [ ] No `unsafe` blocks introduced (enforced by `#![forbid(unsafe_code)]`)

