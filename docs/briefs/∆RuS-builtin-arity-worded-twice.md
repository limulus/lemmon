# A builtin's arity mistake is worded twice

## Background

A builtin's arity mistake is worded twice — the compiler says `.slugify` takes 0 arguments
(`builtin` in `compile/rules.rs`; `render/errors/method-errors.html`) and the VM says
`.slugify()` (`call_dynamic` in `vm/mod.rs`; `…/runtime-method-arity.html`), so the text
depends on whether the receiver is typed or `any`. Pick one wording and state it once on
`Builtin`; a `lem-fn`'s arity sentence is written twice the same way
