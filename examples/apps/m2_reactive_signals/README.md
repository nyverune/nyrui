# Milestone 2 Verification: Reactivity & Scheduler

This example app validates the core architectural guarantees of Milestone 2 as specified in **RFC-0001 §7 and §13**:

1. Fine-grained signal tracking & selective re-rendering without full-tree invalidations.
2. Derived computations (`Computed<T>`) push-pull evaluation & lazy caching.
3. Diamond dependency graph resolution without glitches (`A -> B, A -> C, (B, C) -> D`).
4. Min-heap depth-ordered dirty scheduler coalescing simultaneous parent + child mutations.
5. Clean pruning and render skipping for dirty descendants unmounted by ancestors.
6. Multi-mutation batch coalescing via `Runtime::batch`.

## Running

```bash
cargo run -p m2_reactive_signals
```
