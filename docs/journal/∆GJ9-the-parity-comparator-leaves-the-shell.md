---
id: ∆GJ9
completed: 2026-11-07 10:42:36 -07:00
---

## Backlog item

> - `∆GJ9` The parity comparator leaves the shell
>   - `rendered-html.ts` returns to `scripts/lib` for the fidelity check before the first
>     release, once `lemmon-port`'s last parity run no longer needs it from the shell's API

## Planning

This section folds in the item's brief. A review of the slices found the problem on
2026-10-20. `rendered-html.ts` is the 625-line page comparator (D28). In `d2b0949`
(`∆Zo7`) it moved from `scripts/lib` into the shell, so that limulus.net's port could
import it from `@limulus/lemmon-shell` for `npm run parity`, and the shell's barrel
exported it. No build, check or dev path used it.

Eric first decided, on 2026-10-24, to move it back after the cutover. On 2026-11-01 he
changed that to before the first release, since removing it from a published API would
be a major version. It had to wait for his last parity run on `lemmon-port`, which it
would stop, so that `∆Lj6`, `∆H7Q` and `∆FbD` were checked by that run first. The cutover
branch (`lemmon-cutover` at `28dde17`) imports none of it, and that was checked again
before the move.

No plan file was written. The move was planned in the session: the files go back, the
barrel loses one line, the fidelity script imports from `scripts/lib`, and the root
manifest takes back the `parse5` devDependency that `d2b0949` had moved to the shell. The
shell keeps its own `parse5`, which its link reader and body pass use.

## Refinement

Built on 2026-11-07 as lemmon `73ee780`, `refactor(shell)!`, after Eric accepted slice 6
on the morning's parity run:

- Both files are renames, and the module's only change is one JSDoc sentence.
- Production code changed by 14 lines (imports and the barrel), manifests by 3, docs
  by 58.
- `docs/shell.md` loses its section on the comparator, since the module's own docs
  already said all of it but one fact.
- Four docs that named the shell path now name `scripts/lib`.
- The architecture doc now says, in the past tense, that parity was the acceptance bar,
  met with slice 6.
- The `BREAKING CHANGE:` footer names the fifteen exports that left the API.

The evidence:

- The fidelity check matched its 575 nodes with the same forgiveness counts.
- Importing `@limulus/lemmon-shell` no longer yields `renderedPage` or `compareRendered`.
- `npm run parity` on `lemmon-port` now stops at its first import, naming
  `compareRendered`, which is the intended result.

The diff's review found nothing blocking. Two of its notes were taken:

- The one fact only the deleted section had held, that the comparator also compares the
  pages' doctypes and the document modes they set, went into `compareRendered`'s JSDoc.
- An awkward parenthesis in the architecture doc was reworded.

A third note, that `CLAUDE.md`'s status line was stale, was wrong. `4000300` had already
updated the line, and the reviewer had read an older copy.

`npm run verify` passed before the review and again after its fixes. Eric accepted the
item on 2026-11-07.

## Retrospective

Moving the comparator into the shell for one consumer turned an internal convenience into
an API promise, and undoing that cost a breaking change. Doing it before the first
release made the change free, which was the point of Eric's second decision. A reviewer's
finding about a file is checked against the file, not taken from the reviewer's
context.
