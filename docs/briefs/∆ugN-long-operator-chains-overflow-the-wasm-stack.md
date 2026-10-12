# Long operator chains overflow the `.wasm` stack

## Background

Long operator chains overflow the `.wasm` stack — `<lem.1 + 1 + …>`, or `.first()` chained a
few thousand times, traps `compile` with no span, and the trapped instance fails every later
call, so one such file stops a `∆4tG` worker. No depth cap counts a chain, but the
compiler's walk in `crates/lemmon-vm/src/compile/expr.rs` recurses per term, as does
dropping rowan's tree. Decide: a length cap, or walks that do not recurse (found in `∆vnI`)
