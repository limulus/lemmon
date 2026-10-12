# Raw text in templates

## Background

Raw text in templates — a `lem-let x: string` block whose body is scanned as text to its
close tag (a fourth scanner mode beside D29's fields mode), so a template can hand a
multi-line source to a `code` or diagram producer as a fence does; over it, call-with-body
sugar (`lem-with f(args)` for an `html` slot, `lem-raw f(args)` for a string one) lowering
to capture-then-call, revising D22 for both (D42)
