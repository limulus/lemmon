# Parity harness

## Background

Parity harness — build old (11ty) and new (`lem`) dists, normalize, diff with an allowlist
of expected differences, report per-page, as a local `npm run parity` in limulus-dot-net
(not CI: diffs are nonzero until cutover and the tool dies with it). The acceptance bar for
the whole initiative, so it lands before the port is "done", not after
