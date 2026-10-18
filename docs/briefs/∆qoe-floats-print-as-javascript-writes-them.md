# Floats print as JavaScript writes them

## Background

Found on 2026-10-18 while fixing the data reader (`fix(wasm): read data floats exactly`,
lemmon `2ff19a6`): once a float reads as the double the shell wrote, `Value::emit`
(`crates/lemmon-vm/src/value.rs`) prints it with Rust's `{}`, whose digits always match
JavaScript's `Number#toString` (both write the shortest text that reads back as the same
double) but whose notation differs at the ends of the range and for the special values:

| Value | Lemmon prints | JavaScript prints |
|---|---|---|
| `1e21` and above | `1000000000000000000000` (every digit; `f64::MAX` is 309) | `1e+21`, `1.7976931348623157e+308` |
| below `1e-6` | `0.0000001`, `5e-324` as 324 decimal places | `1e-7`, `5e-324` |
| infinity | `inf`, `-inf` | `Infinity`, `-Infinity` |
| negative zero | `-0` | `0` |

They agree on NaN and on everything from `1e-6` to `1e20`, which is every number
limulus.net prints (all 59 fractional values in its photo data). Arithmetic can reach the
others (`f64::MAX * 10.0` is infinity), and data cannot carry `-0`, which JSON writes `0`.

## Scope and constraints

- A decision first: the docs never say how a float prints, so matching `Number#toString`
  (about 30 lines over `{:e}`, by the fixer's estimate, unmeasured) changes what
  `<lem.x>` prints. The alternatives include refusing to print a non-finite float
  (invariant 10), which `.fixed()` and `.round()` already do for what they cannot write.
- Corpus coverage and docs/language.md for whichever is chosen.
