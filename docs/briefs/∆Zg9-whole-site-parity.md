# Whole-site parity

## Background

### From `∆hyz` Parity harness

Parity harness — build old (11ty) and new (`lem`) dists, normalize, diff with an allowlist
of expected differences, report per-page, as a local `npm run parity` in limulus-dot-net
(not CI: diffs are nonzero until cutover and the tool dies with it). The acceptance bar for
the whole initiative, so it lands before the port is "done", not after

## Scope and constraints

- Slice 6's acceptance, which this item serves: Eric sees `npm run parity` report zero
  unexpected differences over the whole `dist/`, and limulus.net builds and deploys with
  `lem` with its CloudFront config untouched.
- The per-page comparator from `∆Zo7` over every page, with the allowlist it grew; the whole
  initiative's acceptance bar, from the original item.
