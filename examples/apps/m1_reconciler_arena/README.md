# Milestone 1 Verification: Generational Arena & Reconciler

This example app validates the core architectural invariants of Milestone 1 as specified in **RFC-0001 §13.1**:

1. Tree mounting, pre-order `on_mount`, and state initialization.
2. In-place property updates and local instance state preservation.
3. Keyed sibling matching and state tracking across permutations (`[A, B, C] -> [C, A, B]`).
4. Element removal, post-order `on_unmount`, and generational ABA protection.
5. Component type replacement under an identical key.
6. Positional node isolation across distinct conditional `scope` IDs.
7. Recursive post-order child-first destruction.

## Running

```bash
cargo run -p m1_reconciler_arena
```
