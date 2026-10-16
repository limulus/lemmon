# Serialize the linked program

## Background

### From `∆Qr2` Facts and modules across the boundary

Facts and modules across the boundary — `compile` reports each unit's compile-time facts
(signatures, ambient bindings, manifest entries, claims, a document's record) so `∆4tG` keys
its unit cache on `hash(text) + hash(facts)` and a content edit never recompiles its layouts
(D25, D42), and `exportModule`/`importModule` serialize the linked program (D14). Deferred
from `∆TNT` because a fact nothing consumes is a comment

## Scope and constraints

- Moved out of `∆Qr2`, which keeps the compile-time facts and the cache keying; nothing
  scheduled needs serialization.
