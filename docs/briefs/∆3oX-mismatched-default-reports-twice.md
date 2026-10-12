# `??` with a mismatched default reports twice

## Background

`??` with a mismatched default reports twice — the mismatch, then "cannot print a `T?`" —
because `Compiler::check` falls back to the expected type on failure and the `??` lowering
in `crates/lemmon-vm/src/compile/expr.rs` cannot tell that from a genuinely optional right
side. Decide what type a refused `??` takes so one mistake costs one diagnostic
(docs/language.md §Checking); see `render/errors/higher-order-errors.html`
