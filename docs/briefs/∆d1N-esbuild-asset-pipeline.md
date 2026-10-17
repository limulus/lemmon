# esbuild asset pipeline

## Background

esbuild asset pipeline — bundle `**/index.ts` + `**/worker.ts` with `/lib/*` external and
the wasm loader (D09), resolve the `XXXXXXXX` placeholder in client TS (D10), and hand the
manifest to the host declaration at `compile`, its entries reported as compile-time facts
(D25, D38). Not `11ty/esbuild.ts` 1:1: values are finished URLs, not the bare hashes it
stores, since the core does no path arithmetic

## Scope and constraints

- limulus.net's module scripts carry `async`, and `bundler()`'s `scripts` producer writes
  `<script type="module" src="…">` without it (∆F4y), so this item needs an option on
  `bundler()` or its three producers exported separately.
- Placement needs an answer too: limulus.net loads its scripts in three places —
  `/lib/common` in `head`, `/lib/video-on-demand` inside `article-vod`, and `/lib/rum`
  through an inline loader at the end of `body` (`SCRIPT_LIBS` and `unbuiltScripts` in the
  port branch's `bin/parity/classes.ts`) — while `bundler()` places every collected URL at
  every `scripts` mark.
