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
- Mapped on 2026-10-18: parity compares all 55 `index.html` pages, the 10 CSS bundles they
  link, and the 37 script files byte for byte, setting Eleventy's unhashed stubs aside. It
  sees no other file. The feeds and sitemap get their XML leg with `∆zu0`; the 34
  passthrough files (fonts, images, `feed.xsl`, `robots.txt`, the xsd, a pdf) were all
  identical that day and are compared byte for byte. The whole-site run puts every file
  either output holds into a category and fails on any it cannot place.
- Build the Eleventy reference with `TZ=UTC`, as CD does, so its build-time dates match
  production's.
- The last green run is on the commit before the switch (`∆oAl`): parity dies with
  Eleventy.
