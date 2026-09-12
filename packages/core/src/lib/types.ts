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
}

/**
 * What {@link Core.loadData} takes (D37): the host's own ambient values, and what
 * fulfils each `from` binding of the program's bindings files. Which names must be
 * present is the program's to say, so an empty envelope is a well-formed one.
 */
export interface DataInput {
  /**
   * The host's own values, by the name it bound each: `build`; `page` where no loaded
   * document stands in for it — a template, or a document no collection matched;
   * whatever an untyped host names. A value the host reads from a file's bindings
   * (`{bindingsOf}`) is built by the core, never carried. The document value is bound
   * as sent, not validated: it is one value for every such render until `plan()`
   * gives each page its own (∆RGQ).
   */
  ambient?: Record<string, JsonValue>
  /**
   * What fulfils each `from` binding, by binding name: one {@link LoadedValue} per
   * document, as a list, for a glob; one for a data file, a plugin, or an environment
   * value. Every `from` binding the program declares is fulfilled here, or the call
   * throws.
   */
  sources?: Record<string, LoadedValue | LoadedValue[]>
}

/** One value the host loaded, with where it came from. */
export interface LoadedValue {
  /**
   * The value as parsed — objects as maps in key order, every string untrusted. For a
   * document, its front matter with the `type:` key stripped.
   */
  value: JsonValue
  /**
   * The file the value came from: required for a document, which the program must
   * have compiled; a data file's path; absent for a plugin or environment value.
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
 * What {@link Core.render} takes — the spike's `PageRef`: a path names the page until
 * `plan()` exists to mint richer references.
 */
export interface PageRef {
  path: string
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
 * What a render returns: the page's HTML, or `null` with the runtime diagnostic.
 * `channels` and `deps` are additive fields that arrive with ∆RGQ.
 */
export interface RenderResult {
  html: string | null
  diagnostics: Diagnostic[]
}
