---
id: ∆vnI
completed: 2026-09-12 15:28:20 -07:00
---

## Backlog item

> - ∆vnI Pin the stack the `.wasm` is linked with — `MAX_NATIVE_DEPTH` and the parser's own
>   `MAX_DEPTH` are both budgeted against a megabyte, and `nests_to_the_cap_within_the_stack_the_wasm_is_given`
>   (`crates/lemmon-vm/src/vm.rs`) tests against one, but nothing makes the artifact have it:
>   the build takes whatever `wasm-ld` defaults to. Pass `-z stack-size` and assert it in
>   `scripts/wasm-artifact.ts`, so the number the caps were measured against is the one that ships

## Planning

**The item's premise was half right.** The build did not take `wasm-ld`'s default, which
is 64 KiB. rustc's wasm32 target passes `-z stack-size=1048576 --stack-first` itself,
which `cargo rustc … -- --print link-args` showed, and the staged artifact's
`__stack_pointer` started at 1048576 with static data above it. So the megabyte the caps
were measured against was real, but the toolchain set it, not the repo.

**The first plan pinned and asserted, and Eric cut the assertion.** It had a build script
for the pin, plus a reader in `scripts/lib/` that pulled the stack size out of the staged
`.wasm`'s first global for `wasm:check` to compare. Eric stopped at approval to ask what
the change solves. The answer was that nothing is broken: the change guards against a
toolchain or flag change shrinking the stack under the caps, where an overflow becomes a
trap with no span. He then asked which stack. It is the Rust shadow stack in linear
memory, not the VM's heap-allocated operand stack and frames, and not the engine's call
stack. With that settled he took the pin alone. The ABI test that reaches the VM's cap is
the guard, since the pin reaches the wasm32 test binary too. The item's "assert it in
`scripts/wasm-artifact.ts`" half was dropped on purpose.

**A build script, not `.cargo/config.toml` rustflags.** A `RUSTFLAGS` or
`CARGO_TARGET_*_RUSTFLAGS` variable replaces config rustflags outright, so a pin kept there
can vanish without notice. `cargo::rustc-link-arg` reaches every linked target of
`lemmon-wasm`, including the test binary, and is guarded on `CARGO_CFG_TARGET_ARCH` so host
builds get nothing.

## Refinement

**The parser's cap had the same dependency and no test on wasm.** Shrinking the pin to
64 KiB trapped only the VM's depth-cap test. Probing at the megabyte showed that 200
nested `lem-if` blocks compile and render, and trap at 256 KiB, because checking, lowering,
and rendering recurse as deep as the parser does. Blocks cost more stack per level than
nested parentheses, lists, or negations. A new ABI test,
`renders_blocks_nested_to_the_parser_cap_without_trapping`, covers it, and also checks
that level 201 is refused so the test tracks the cap. In the debug build the tests run,
both caps' deepest nesting needs more than 256 KiB and less than 384 KiB.

**Two stale sentences were found by the sweep.** A doc comment about the megabyte thread
sat above `counts_ids_from_zero_again_at_every_render` in `vm.rs` instead of the test it
describes. `docs/architecture.md` still said the frame cap bounds native recursion, which
stopped being true when [[∆Q6L]] gave re-entry its own cap.

**The review found a recursion no cap counts.** The parser builds `1 + 1 + …` and
`.first().first()…` in a loop, so neither cap sees a chain, but later stages recurse once
per term. The staged release artifact renders a 1,000-term chain and traps during
`compile` at 5,000. A trap also leaves the stack pointer where it was, so every later call
into that instance traps, even a new `Core` compiling `1 + 1`. On a one-megabyte host
stack, the compiler's expression walk overflowed first, at 2,000 terms, and dropping
rowan's green tree overflowed at 5,000. This predates the item and needs its own design,
so it went to [[∆ugN]]. The build script's comment now says a trap makes the instance
unusable.

## Retrospective

**I asked for approval before saying what the change was for.** The plan described the
mechanism in detail and never said in plain words that nothing was broken and this was
insurance. Eric had to reject the approval request to get that answer, and the answer
changed the scope. For a change that guards against drift, lead with what it prevents,
whether that has ever happened, and the smallest version that still guards.

**The first plan offered the larger version first.** A hand-written wasm section reader
and its spec were proposed to assert a value rustc already sets. The pin-only version was
there from the start and came up only when Eric asked.

**Caps were tested with inputs that nest, not inputs that grow.** The existing parser
tests nest brackets 100,000 deep and blocks 50,000 deep. A chain is the obvious shape that
does not nest, and no test tried one until the review. When a cap is meant to stop
recursion, probe the shapes that recurse without passing through the cap.
