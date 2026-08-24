# Journal

Per-completed-item write-ups capturing what shipped and what we learned. One file per
backlog item, scaffolded by `delto complete` (see `delto complete --help`).

## Convention

- **Filename**: `∆xxx-slug.md` — `∆xxx` is the immutable deltoid from `BACKLOG.md`,
  `slug` a short kebab-case title.
- **Frontmatter**: `id` (the deltoid, `∆` included) and `completed`
  (`YYYY-MM-DD HH:MM:SS ±HH:MM`), per the delto spec v1.0.
- **Sort order**: filenames are not chronological by design. The `completed`
  frontmatter is the source of truth; `git log docs/journal/` gives chronological
  history when needed.
- **Cross-references**: link to other entries as `[[∆xxx]]`.

## CLAUDE.md distillation

Journal entries are periodically reviewed and their durable lessons distilled into
`CLAUDE.md`. Everything completed at or before the watermark below is settled —
incorporated or deliberately excluded — and is not revisited on later runs.

**Last distilled: 2026-08-23 18:29:57 -07:00.**

To find the entries a new run needs to review, compare each entry's `completed`
frontmatter against the watermark:

```sh
grep -H '^completed:' docs/journal/∆*.md
```

Review every entry completed after the watermark. When one sits near the boundary,
review it — a redundant read is cheap, a missed lesson is not.

When running a distillation: verify every concrete claim (helper names, script names,
config values) against the current code before writing it into `CLAUDE.md` — entries
describe the code as it was, and later work may have superseded them.
Then advance the watermark in the same commit as the `CLAUDE.md` changes, using the
datetime at which the review ran.
