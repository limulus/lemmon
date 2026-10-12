# esbuild asset pipeline

## Background

esbuild asset pipeline — bundle `**/index.ts` + `**/worker.ts` with `/lib/*` external and
the wasm loader (D09), resolve the `XXXXXXXX` placeholder in client TS (D10), and hand the
manifest to the host declaration at `compile`, its entries reported as compile-time facts
(D25, D38). Not `11ty/esbuild.ts` 1:1: values are finished URLs, not the bare hashes it
stores, since the core does no path arithmetic
