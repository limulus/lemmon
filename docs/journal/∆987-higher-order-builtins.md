---
id: ∆987
completed: 2026-09-07 15:21:44 -07:00
---

## Backlog item

> - ∆987 Higher-order builtins — a re-entrant `Vm::invoke` so a builtin can call a
>   `Value::Fn`, since `Builtin::call` is a pure `fn(&[Value])` and `Vm::call` only pushes a
>   frame, plus D34's rule hook shaping later arguments from earlier ones, proven by `map`
>   and `filter`. One unit: the hook has no consumer without a higher-order method, and no
>   higher-order method runs without the invoke

## Planning

Surfaced as the top eligible item after [[∆495]]. The plan named the two mechanisms up
front, since [[∆495]]'s retrospective had said that was where its own estimate went wrong.
The invoke turned out cheap: `run` became `run_until(depth)`, the same loop with a floor,
and a builtin's host handle pushes the frame, runs to the floor, and pops the value
`Return` leaves behind. What it needed was an error type with two states — a message the VM
places at the calling instruction, and an error already placed inside the invoked function,
which has to pass through the builtin untouched or the lambda body's span is lost
(invariant 8).

A plan review found one hole before implementation. The shaping rule was going to hand a
lambda the expectation `(T): any`, and `rules::expected` returns the actual type on
success, so the body's inferred type would flow out — except where the body is a literal or
a `?:`, which take an expected `any` as an opt-out and come back `any`. `xs.map(|a| a gt 1 ?
"x" : "y")` would have typed `[any]`. The plan's fix was to infer the body whenever the
expected return is `any`. That fix was wrong at a depth the review did not see; the code
review after implementation did (below).

The other risk named was native stack: every nested invoke now costs a native round trip
(`step` → builtin → `invoke` → `run_until` → `step`), and `MAX_DEPTH` bounds it only
because every invoke pushes a frame. Rather than estimate frame sizes, the plan made a
wasm-bindgen test the decision rule — drive a self-mapping `lem-fn` to the cap under the
debug profile and see whether it reports the depth error or traps — with a separate
invoke budget as the fallback if it trapped. It did not trap; no budget was needed.

## Refinement

Eric asked for a medium-effort code review of the diff before accepting, and it reshaped two
things.

**The `any`-as-infer fix leaked.** Inferring the body whenever the expected return is `any`
applied to every lambda under such a type, not only to `map`'s. An author who declares
`<lem-fn apply(f: (int): any)>` and passes `|a| a gt 1 ? "x" : 1` had a program that
compiled at `HEAD` and failed after the plan's fix, while `<lem-let v: any = …>` of the
same expression still compiled. `any` already meant "opt out" in the language; using it to
also mean "infer" made one spelling carry two rules. The fix is a distinct expectation:
`Builtin::expect` returns either a type or *a function of these parameters, return the
argument's own*, and the latter lowers a lambda under a shape whose return slot is
`None`. An expected `any` means what it always meant, the corpus pins the author's case,
and the messages improved as a side effect — `.map` takes a function of `(string)`, got
`int` — where before they printed the internal `(string): any`.

**A pre-existing link bug under the new tests.** `link` rebases every constant-carrying
operand into the module's joined pool except `CallDynamic`'s, which [[∆495]] added and this
item made load-bearing: every new `runtime-*` corpus case compiles to it. A second file in
the module gives the first a non-zero offset, and the method name then reads whatever
constant sits at the raw index — a wrong method, or the `constant_str` panic when it is not
a string. Nothing caught it because the render harness compiles one file per case, so every
corpus case links at offset zero. The arm is fixed, the `_ => {}` catch-all is now an
exhaustive list so a future table-carrying op cannot fall through, and a two-file `link`
test renders the second file.

The rest of the review was prose the change had falsified — `slugify` still listed as an
ambient function, an expectation-reach bullet that contradicted the Calls bullet fourteen
lines below it, a present-tense "nothing has today" in D34 — plus a `filter` comment that
misstated who reaches its runtime check, and a test keyed on table order.

Left for a decision rather than fixed: a `??` whose default mismatches yields two
diagnostics, the mismatch and a follow-on "cannot print", because a failed check falls back
to the expected type and the walk cannot tell that from a genuinely optional right side.
Pre-existing, now blessed in three more places in `higher-order-errors.html`, and against
§Checking's one-mistake-one-diagnostic rule; it needs a design pass on what type a refused
`??` takes.

The first wasm depth test was ill-typed — `[n].map(dive)` with `dive` returning `[int]` is
`[[int]]` — and the checker refused it across the ABI, which is the shaping rule working
where it was least expected to be exercised.

## Retrospective

**A fix that keys on a type already carrying a meaning is a special case, however small
it looks.** The plan-review fix was one guard on one line and read as obviously right. It
gave `any` a second meaning in one position, and the position was broader than the case
that motivated it. The question to ask before such a fix is not "does this make the failing
case pass" but "what else is spelled this way" — here, every author-written function type
with an `any` return. The right fix was making the absent expectation representable, which
is more code and less cleverness.

**A harness that never builds the multi-file shape cannot see a whole class of bug.** The
render corpus is the project's truth for the compiler and VM, and it compiles exactly one
file per case. Anything that only goes wrong when units are joined — every per-unit table
operand at a non-zero offset — is invisible to it by construction, and the new cases looked
like coverage of the `any` method path while being unable to catch the bug in that path's
own op. The exhaustive match is the durable fix; the two-file link test is the proof.

**When a plan says "verify on wasm", make the verification the decision rule and write it
before deciding.** The stack estimate ran from "fine in release" to "may trap in debug"
depending on who did the arithmetic. The test settled it in one run and left a guard that
will catch anyone raising `MAX_DEPTH` later without noticing it now bounds native
recursion too.

**A review of the finished diff catches what a review of the plan cannot.** [[∆495]]
learned that the plan review finds the shape of the item; this item learned that the
implementation review finds what the plan review's own fixes cost. Both were worth their
time, and the second found a real pre-existing crash the first had no way to see.
