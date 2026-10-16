---
id: ∆pXO
completed: 2026-10-15 19:57:17 -07:00
---

## Backlog item

> - `∆pXO` Lemmon authoring skill
>   - A checked-in agent skill for writing `.lem`, distilled from docs/language.md, not a copy
>     of it: porting limulus.net's layouts means writing a lot of Lemmon by agent

## Planning

The item's brief, folded here: a checked-in agent skill (`agents/skills/lemmon/`) for
writing `.lem`: tag forms and head-shape rules, expression syntax, type-directed escaping,
document mode, distilled from docs/language.md, not a copy of it. Porting limulus.net's
layouts means writing a lot of Lemmon by agent, and rereading the whole language doc every
time is neither reliable nor cheap.

Built in the slice-1 run of 2026-10-15 by an Opus agent from a brief the managing session
wrote. The brief fixed the location the item named, with a `.claude/skills/lemmon` symlink
so Claude Code loads it as a project skill; eleven content areas (head-shape rules, a tag
table, expressions, types and schemas, `site.lem` and the ambient environment, functions and
components with the whitespace rules, escaping, document mode, the compiler's common
refusals, how to check a file today, and a WebC-to-Lemmon porting guide for the port); a
target of 250–350 lines for `SKILL.md` with depth pushed to `references/`; and one quality
bar: every Lemmon example compiles clean through the staged core, and every example of a
mistake produces the diagnostic it describes.

## Refinement

`SKILL.md` is 359 lines, plus `references/checking.md` (the compile-check script, 34 lines
of Node rather than the brief's ten, because it has to embed `ambient.lem` and type each
document by its `type:` key or glob the way the shell does), `references/mistakes.md` (42
compile-time and 7 load-time diagnostics, each with a line that earns it) and
`references/porting-from-webc.md` (459 lines). A throwaway harness extracted every fenced
example and compiled it: 71 checks, 0 failures, and a negative pass showed each kind of
breakage failing it.

Checking the examples found five places where docs/language.md and the compiler disagree:
string interpolation is not implemented, `lem-import` is refused, `concat` does not exist
and lists do not concatenate, the canonical example does not compile, and the
unquoted-attribute lint has no linter. The skill states what the compiler does; the doc
fixes are `∆bah`.

The managing session extended the placeholder gate
(`crates/lemmon-corpus/tests/placeholders.rs`) to read `agents/` as it reads `docs/`, and
phrased the skill's forward references ("not yet", with the deltoid on the line) so that
landing `∆muD`, `∆1rD`, `∆c4j`, `∆99b`, `∆d1N` or `∆F4y` fails the build until the skill
is swept. CLAUDE.md names the skill under Canonical references and in the layout.

## Retrospective

Compiling every example was the whole value of the check: it is what found the doc's five
disagreements, none of which reading would have caught. A skill that promises forms is
only as good as the gate that notices when the promise comes true; putting it under the
gate cost one line.
