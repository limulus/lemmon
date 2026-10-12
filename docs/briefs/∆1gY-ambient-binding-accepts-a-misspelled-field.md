# An ambient binding accepts a misspelled field

## Background

An ambient binding accepts a misspelled field — `AmbientBinding` in
`crates/lemmon-wasm/src/wire.rs` is an untagged serde enum, the one inbound shape without
`deny_unknown_fields`, so `{"bindingsOf": "site.lem", "typo": 1}` is taken with no error
(invariant 10). Refuse it, naming the field, which serde's untagged error does not, and drop
the exception `packages/core`'s `types.ts` header records (found in `∆rKx`)
