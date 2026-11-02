# Lemmon

A static site generator and a typed, tag-based template language.

> **Early development.** Lemmon is young and changing fast. Expect frequent major
> releases, each with significant breaking changes.

A Node/TypeScript **shell** owns files, processes, plugins, and the dev server; a Rust
**core** — scanner, parser, a compiler that type-checks as it lowers to bytecode, and a VM,
with comrak for markdown — is compiled to WebAssembly and does everything deterministic.
Templates are HTML with Lemmon tags in it: only `<lem.…>` and `<lem-…>` tags are parsed, the
rest is text, and every error points at the source.

## A taste

```html
<lem-type Post>
  title: string
  url: url
  subhead: string?
  tags: [string]
</lem-type>

<lem-fn card(post: Post)>
  <article>
    <h2><a href="<lem.post.url>"><lem.post.title></a></h2>
    <lem-if post.subhead>
      <p><lem.post.subhead></p>
    </lem-if>
    <ul>
      <lem-for tag in post.tags>
        <li><lem.tag></li>
      </lem-for>
    </ul>
  </article>
</lem-fn>

<lem.card({
  title: "The Rebound",
  url: url("/feed/the-rebound/"),
  subhead: "I’m rebuilding limulus.net. It’s way past time to have a website again.",
  tags: ["article"]
})>
```

This renders the post's card, with its subhead in a `<p>` because the post has one. Drop
the `lem-if` and the file does not compile: "cannot print a `string?`: narrow it with
`lem-if`, or give it a default with `??`". Misspell a field, as in `<lem.post.titel>`, and
it does not compile either: "`Post` has no field `titel`; did you mean `title`?". Printing
escapes a `string`, so a title with an `&` comes out as `&amp;`; only an `html` value
prints as it is.

## Documentation

- [`packages/shell/README.md`](packages/shell/README.md) — installing `lem` and building a
  first site. Start here.
- [`agents/skills/lemmon/SKILL.md`](agents/skills/lemmon/SKILL.md) — the author's
  reference for writing `.lem`, with a guide to porting WebC beside it.
- [`docs/language.md`](docs/language.md) — the language in full: scanner, tags,
  expressions, types, namespaces, document mode. A few forms it describes are not built yet
  (`∆bah`).
- [`docs/shell.md`](docs/shell.md) — the shell in full: every command, config field,
  plugin hook and build step.
- [`docs/architecture.md`](docs/architecture.md) — the core/shell split, the WASM API,
  the build pipeline.
- [`docs/decisions/`](docs/decisions/README.md) — every settled decision. A `Dnn` in the
  docs or the code names one.
- [`packages/core/README.md`](packages/core/README.md) — the core's JS API, for running
  Lemmon from your own code.
- [`BACKLOG.md`](BACKLOG.md) and [`docs/journal/`](docs/journal/) — the plan and the
  record of what shipped, kept with [delto](https://github.com/limulus/delto).
- [`CLAUDE.md`](CLAUDE.md) — the invariants and conventions every change keeps.

## Layout

| Path | What |
|---|---|
| `crates/lemmon-syntax/` | scanner, parser, syntax trees, diagnostics |
| `crates/lemmon-vm/` | types, the checking compiler, the VM, document mode |
| `crates/lemmon-wasm/` | the WebAssembly boundary: the resident `Core` |
| `crates/lemmon-corpus/` | discovery and blessing for the golden corpus |
| `packages/core/` | `@limulus/lemmon-core` — the typed JS wrapper around the `.wasm` |
| `packages/shell/` | `@limulus/lemmon-shell` — the `lem` CLI: build, check, dev server |
| `tests/corpus/` | golden inputs and their expected trees, output and diagnostics |
| `tests/fidelity/` | one limulus.net post, held against its Eleventy build |
| `scripts/` | repo tooling the npm scripts run |
| `docs/` | the language, the shell, architecture, decisions, journal and briefs |
| `agents/skills/lemmon/` | the authoring skill: `SKILL.md` and its references |

## Development

You need Node 24 or later, [rustup](https://rustup.rs), and `wasm-bindgen-cli` at the
version `Cargo.toml` pins, which `npm run rust:wasm` names in the command it prints when
the CLI is missing. `rust-toolchain.toml` pins the Rust toolchain, and rustup installs it,
its components and the `wasm32-unknown-unknown` target on the first `cargo` command:

```sh
cargo install wasm-bindgen-cli --version 0.2.127 --locked
npm install          # also installs the git hooks: commitlint, and verify on pre-push
npm run verify       # lint, tests with coverage, typecheck, the wasm harnesses, cargo
                     # fmt/clippy/test; stages the .wasm first
npm run rust:wasm    # cross-compile the boundary crate and stage the .wasm, which
                     # `npx vitest` or a harness run on its own needs first
npm run build        # emit packages/*/dist
```

`npm run verify` is the whole gate for both halves of the repo and is what pre-push runs.
CI runs its parts as separate jobs so the Rust checks run once rather than once per Node
version, and so the `.wasm` is built once and then exercised on every supported Node.

## License

[MIT](LICENSE), © 2026 Eric McCarthy. The post in `tests/fidelity/` is Eric's, published on
limulus.net under CC BY 4.0 and included here under MIT as well.
