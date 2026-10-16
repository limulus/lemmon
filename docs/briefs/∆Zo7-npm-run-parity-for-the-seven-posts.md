# `npm run parity` for the seven posts

## Background

### From `∆hyz` Parity harness

Parity harness — build old (11ty) and new (`lem`) dists, normalize, diff with an allowlist
of expected differences, report per-page, as a local `npm run parity` in limulus-dot-net
(not CI: diffs are nonzero until cutover and the tool dies with it). The acceptance bar for
the whole initiative, so it lands before the port is "done", not after

## Scope and constraints

- Slice 1's acceptance, which this item serves: Eric runs `lem build` in the limulus-dot-net
  port branch, opens the seven `feed/` posts from `dist/` in a browser, and `npm run parity`
  there reports them against the 11ty `dist/` with every difference in a named forgiveness
  class.
- Per page, in the limulus-dot-net branch, not in CI (from the original item: diffs are
  nonzero until cutover and the tool dies with it). The allowlist names each forgiveness
  class and how often it fires, and fails on anything else, as the fidelity harness does.
- A class that forgives more than it claims hides real differences; the `∆xWs` retrospective
  records that happening once. Test what the allowlist erases, not only what it selects.

## Implementation suggestions

- Lift `renderedPage` and `compareRendered` from `scripts/lib/rendered-html.ts` into the
  shell, or run them from a shell command; the fidelity harness keeps using them for its own
  page.
