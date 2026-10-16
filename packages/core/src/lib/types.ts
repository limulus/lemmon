/**
 * The shapes that cross the WASM boundary, declared for the JS side: what a {@link Core}
 * method sends and what it gets back. They mirror `crates/lemmon-wasm/src/wire.rs`,
 * written out by hand — nothing generates either side from the other.
 *
 * The two directions fail differently. An inbound field the core does not know is
 * refused by `deny_unknown_fields`, so a name misspelled here throws on the next call —
 * in every inbound shape but an ambient binding's `{bindingsOf}` form, an untagged
 * variant that accepts an unknown field in silence. An outbound field renamed in Rust is
 * not refused at all: `core.ts` casts each parsed reply to the type it declares without
 * checking it, so a rename shows up only where a spec reads that field.
 */

/** A JSON-serialisable value — what crosses the boundary as data. */
export type JsonValue =
  null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue }

/** One source file handed to {@link Core.compile}. */
export interface SourceFileInput {
  /** The path the file is known by — what diagnostics and renders name it with. */
  path: string
  /** Its contents. */
  text: string
  /**
   * The record this file's content is: the element type of the collection whose glob
   * matched it, or the type its own `type:` key named (D30). It types the host's
   * document value inside this file, and is one of the unit's compile-time facts. A
   * document without one is unknown-typed and renders.
   */
  record?: string
  /**
   * Where {@link record} was named — the `type:` key's value, as byte offsets into
   * `text` — so a diagnostic about the record points at the key rather than at the
   * front matter as a whole. The host holds the YAML, so the host says where.
   */
  recordSpan?: { start: number; end: number }
}

/**
 * What one ambient name is bound to: a type the program declares, or the record of the
 * top-level bindings of one of its files. `null` names the value without typing it,
 * which gives it the checker's unknown type.
 */
export type AmbientBinding = string | { bindingsOf: string } | null

/**
 * The environment a host declares (D30): the core knows no type or value by name, so
 * every ambient value a program may read is named here, and the types themselves are
 * ordinary declarations in files the host compiles with the program.
 */
export interface HostDecl {
  /** Every ambient value, by the name templates read it as. */
  ambient: Record<string, AmbientBinding>
  /**
   * Which ambient value a file's {@link SourceFileInput.record} retypes — `page` in the
   * standard shell. A host that renders no documents names none.
   */
  document?: string
  /**
   * The field a page is rendered through — `layout` in the standard shell — required on
   * every record that extends the document value's type.
   */
  entry?: string
  /**
   * The field a page's url is read from — `url` in the standard shell — which the
   * document value's type declares as `url` (D43). {@link Core.plan} reports each
   * page's url from it, and two loaded pages at one url is a load error. A host that
   * names none has no page placed, and no url checked.
   */
  url?: string
  /**
   * The manifest the host's build produced: each asset's logical path mapped to the URL
   * it shipped at, in the order a diagnostic should list them.
   *
   * The value is the finished URL and not a content hash — the core does no path
   * arithmetic, since where an asset lands is the shell's question (D38). It belongs to
   * the declaration rather than to {@link DataInput} because `asset("…")` is answered
   * while compiling (D10): the URL is folded into the bytecode, and the entry it came
   * from is a compile-time fact the link step compares. A rebuild that re-hashes an
   * asset is therefore a change to this declaration, and recompiles the files that
   * named it — and only those.
   */
  assets?: Record<string, string>
  /**
   * What the host's mark handlers claim (D42): the names `mark()` may place, and for
   * each the keys its producers cover and the payload its marks carry, which the core
   * checks a call and a code block's language against while compiling. Omitted, the
   * host declares no handlers at all and the core says nothing about any name — what a
   * playground or an editor wants, since a mark there is meant to keep its default
   * content. An empty list still declares handlers, and warns at every name none of
   * them claims. Several claims for one name are merged; two naming different payloads
   * make the call throw.
   *
   * On the declaration for the manifest's reason: the answer is given while a file
   * compiles, so a plugin list that changes what it says about a name and a key leaves
   * the files that asked stale, and only those.
   */
  marks?: Claim[]
}

/**
 * One claim a host's handlers make (D42). A decorator's is the name alone; a producer's
 * lists the keys it covers, since there is no wildcard producer, and names the record
 * its marks carry.
 */
export interface Claim {
  /** The name `mark()` places and the handler answers for — `code` for a code block. */
  name: string
  /**
   * The keys a producer covers: a highlighter's languages. A key none of the name's
   * producers covers is a warning where the author wrote it.
   */
  keys?: string[]
  /**
   * The declared record a mark of this name carries, which a `mark()` call's payload is
   * checked against — `Code` for `code`, which the shell's `ambient.lem` declares.
   */
  payload?: string
  /**
   * The field of {@link payload} holding a mark's key — `language` for `code` — so a
   * template's mark is dispatched as a code block is. Needs `payload`.
   */
  keyedBy?: string
}

/**
 * What a code block hands the producer for its language (D42), reported once by
 * {@link Core.compile} and named by {@link Mark.payloadId} in every render that marks
 * the block. The id is a hash of the info string and the code, so a host keeps what a
 * producer made on it across builds.
 */
export interface CodePayload {
  id: string
  /** The first word of the info string — the mark's key — or `null` where none. */
  language: string | null
  /** The whole info string: `ts {1,3}` for a fence opening ```` ```ts {1,3} ````. */
  info: string
  /** The code, unescaped. */
  code: string
}

/**
 * One `from` binding of the program's bindings files, as {@link Core.compile} reports
 * it: what the host fulfils in {@link DataInput.sources}, under {@link name}.
 *
 * A `file` binding that {@link collects} a record is a glob: every document and page
 * template it matches is compiled with that record and loaded, as a list, in its
 * binding. Any other `file` binding is a data file, and `plugin` and `env` bindings
 * are host sources; each of the three is fulfilled by one value. Which is which is
 * decided by the binding's type, so the core answers it and the host does not repeat
 * the rule.
 */
export interface FromBinding {
  /** The binding's name: the key {@link DataInput.sources} fulfils it under. */
  name: string
  /**
   * The form after `from`: a string literal names a `file` — a glob or a data file — and
   * `plugin("…")` or `env("…")` a plugin's data hook or an environment value.
   */
  kind: 'file' | 'plugin' | 'env'
  /**
   * The string the form holds: the glob or the data file's path, relative to the input
   * directory; the plugin's name; the environment variable's name.
   */
  literal: string
  /**
   * The record each page the glob matches is — the element type of the binding's list,
   * which extends the host's document type. Absent for a data file, a plugin, or an
   * environment value.
   */
  collects?: string
  /** The bindings file the binding stands in. */
  path: string
  /**
   * The binding's name in that file: where a host reports a source it cannot fulfil —
   * an environment variable that is not set, a data file that is not there.
   */
  span: Span
}

/**
 * What {@link Core.compile} returns: the diagnostics about the files it was given, the
 * payload of every code block they write, each once, and every `from` binding of the
 * program the call installed, in the order its bindings file declares them — none
 * where the call installed no program, or the program has no bindings file. `facts`
 * (∆Qr2) is an additive field that arrives with that item.
 */
export interface CompileResult {
  diagnostics: Diagnostic[]
  payloads: CodePayload[]
  sources: FromBinding[]
}

/**
 * What {@link Core.loadData} takes (D37): the host's own ambient values, what fulfils
 * each `from` binding of the program's bindings files, and the pages no glob holds.
 * Which names must be present is the program's to say, so an empty envelope is a
 * well-formed one.
 */
export interface DataInput {
  /**
   * The host's own values, by the name it bound each: `build`; `page` where no loaded
   * page stands in for it — a file that is no page, or a page no record typed;
   * whatever an untyped host names. A value the host reads from a file's bindings
   * (`{bindingsOf}`) is built by the core, never carried. The document value is bound
   * as sent, not validated: it is one value for every such render, and a loaded page
   * never reads it, since its own record stands in.
   */
  ambient?: Record<string, JsonValue>
  /**
   * What fulfils each `from` binding, by binding name: one {@link LoadedValue} per
   * page, as a list, for a glob — a document, or a template with front matter; one
   * for a data file, a plugin, or an environment value. Every `from` binding the
   * program declares is fulfilled here, or the call throws.
   */
  sources?: Record<string, LoadedValue | LoadedValue[]>
  /**
   * The front matter of every page no glob holds, whose own `type:` key typed it
   * (D41), each with its path. Loaded after every binding, in this order, so a `by`
   * index in one may read a map the site loaded. A page carried twice, here or in a
   * glob, makes the call throw; a page not carried at all is simply not loaded —
   * {@link Core.plan} omits it, and rendering it throws.
   */
  pages?: LoadedValue[]
}

/** One value the host loaded, with where it came from. */
export interface LoadedValue {
  /**
   * The value as parsed — objects as maps in key order, every string untrusted. For a
   * page, its front matter with the `type:` key stripped.
   */
  value: JsonValue
  /**
   * The file the value came from: required for a page, which the program must have
   * compiled; a data file's path; absent for a plugin or environment value.
   */
  path?: string
  /**
   * Where each field path stands in that file — `title`, `hero.caption`, `tags.2`,
   * and `''` for the value as a whole — each spanning the key through the value. A
   * load diagnostic about a field points here; a path with no entry falls back to its
   * nearest ancestor, then to the whole value, then to the declaration of what was
   * expected. The host computes offsets and positions both, since the core holds no
   * index for a file it never compiled.
   */
  spans?: Record<string, Span>
}

/**
 * What {@link Core.render} takes: the path of a file the program compiled — a page
 * {@link Core.plan} listed, or any other file. `index`, numbering a generator's pages,
 * is an additive field that arrives with ∆99b.
 */
export interface PageRef {
  path: string
}

/**
 * One page as {@link Core.plan} lists it (D43): what {@link Core.render} takes, with
 * the url the host-named field holds — absent where the host names no
 * {@link HostDecl.url}.
 */
export interface PlannedPage extends PageRef {
  url?: string
}

/**
 * A place in a source file, as the Language Server Protocol defines it: both fields
 * zero-based, and `character` counting UTF-16 code units from the start of the line —
 * not bytes, and not characters. A display that counts from one adds one to each.
 */
export interface Position {
  line: number
  character: number
}

/** Both ends of a {@link Span} as positions — the LSP's `Range`. */
export interface Range {
  start: Position
  end: Position
}

/**
 * A range in a source file, as both of the things a consumer needs: the byte offsets the
 * core works in, and the same two points as LSP positions. The core computes `range`
 * because it holds the bytes; deriving a UTF-16 column from a byte offset here would mean
 * re-encoding the source.
 */
export interface Span {
  start: number
  end: number
  range: Range
}

/**
 * How much a diagnostic matters: an `error` stops its file from compiling; a `warning`
 * is reported and the file compiles anyway.
 */
export type Severity = 'error' | 'warning'

/**
 * A secondary span a {@link Diagnostic} points at with a note of its own — where a name
 * was first defined, what a caller was compiled against. Path-qualified, since it may
 * be in another file.
 */
export interface Label {
  path: string
  span: Span
  message: string
}

/**
 * A problem in a template or document, pointing at source (invariant 8), with the
 * secondary spans worth reading with it. A `code` would be an additive field.
 */
export interface Diagnostic {
  path: string
  span: Span
  severity: Severity
  message: string
  labels: Label[]
}

/**
 * A named range of a rendered page for the host's handlers to fill (D42), with its
 * default content already inside it.
 */
export interface Mark {
  /** The name the handlers are dispatched by. */
  name: string
  /**
   * What a producer is chosen by under that name: a code block's language, or the
   * payload field the claim's `keyedBy` names. Absent where the mark has none.
   */
  key?: string
  /** The payload a `mark()` call built, as JSON. */
  payload?: JsonValue
  /**
   * The id of the {@link CodePayload} a code block carries, which {@link Core.compile}
   * reported — the code never rides on the render.
   */
  payloadId?: string
  /** Byte offsets into {@link RenderResult.html}, not character indexes. */
  start: number
  end: number
  /** The file the `mark()` call stands in — a layout, not the page being rendered. */
  path: string
  /** That call, so a handler's own diagnostic can point at source (invariant 8). */
  span: Span
}

/** One thing a `lem-style`, `lem-head`, or `lem-script` gathered, and where from. */
export interface Fragment {
  /** The text the block rendered, or the URL the script named. */
  text: string
  path: string
  span: Span
}

/**
 * What a page collected, by name: a name with no fragments is absent rather than empty.
 */
export type Fragments = Record<string, Fragment[]>

/**
 * What a render returns: the page's bytes with the marks standing in them and the
 * fragments gathered beside it, or `null`s with the runtime diagnostic.
 *
 * The page is bytes and not a string because a mark's offsets name those bytes, and
 * because escaping a page into the reply's JSON and parsing it back costs more than the
 * render. A host that wants a string decodes once, after {@link resolveMarks} has
 * spliced (D42). `deps` (∆xBO) is an additive field that arrives with that item.
 */
export interface RenderResult {
  html: Uint8Array | null
  marks: Mark[] | null
  fragments: Fragments | null
  diagnostics: Diagnostic[]
}
