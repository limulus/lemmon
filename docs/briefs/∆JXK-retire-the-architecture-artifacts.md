# Retire the architecture artifacts

## Value

Lemmon's whole design record is public: a reader of the repo has everything that settled
its design, and no rule points anyone at documents they cannot open.

## Background

The full architecture record — its diagrams, the migration plan, and the rationale — lives
in three Claude Artifacts Eric keeps privately: "Shell & Core", "The Lemmon Language", and
"From Eleventy to Lemmon". The repo docs are their working distillation; the rationale for
D01–D24 is in the artifacts alone (`docs/decisions/README.md`), while every decision from
D25 on has its record in the repo. Until 2026-11-01 each implementation pass logged what
an artifact must change in `docs/artifact-changes.md`, for one reconciliation at the end
(`∆MlD`); the ledger had grown to 76 entries over 723 lines.

On 2026-11-01, as lemmon went public (`∆P6l`), Eric questioned `CLAUDE.md`'s rule that the
artifacts win where a repo doc contradicts itself, since the aim is fully public docs, and
decided:

- the artifacts are retired rather than reconciled, in this item, which supersedes `∆MlD`;
- passes stop writing ledger entries at once, and `CLAUDE.md`'s rule to write them goes;
- a repo doc that seems to contradict itself is settled in the repo doc, with Eric asked,
  not by the artifacts.

## Scope and constraints

- The repo docs take in what only the artifacts hold: D01–D24's rationale, the diagrams,
  and the migration plan.
- It comes after the cutover, where `∆MlD` sat (Eric, 2026-11-01).

## Implementation suggestions

- The frozen ledger lists what each artifact gets wrong today, which is what the
  retirement must not carry into the repo docs.
