/** A JSON-serialisable value — what crosses the boundary as data. */
export type JsonValue =
  null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue }

/** One source file handed to {@link Core.compile}. */
export interface SourceFileInput {
  /** The path the file is known by — what diagnostics and renders name it with. */
  path: string
  /** Its contents. */
  text: string
}

/**
 * The data {@link Core.loadData} stores — the spike's `DataInput`: the ambient records
 * only. `records`, data files, and the asset manifest become additive fields later.
 */
export interface DataInput {
  ambient: AmbientData
}

/** The three ambient records every template may read. */
export interface AmbientData {
  site?: JsonValue
  page?: JsonValue
  build?: JsonValue
}

/**
 * What {@link Core.render} takes — the spike's `PageRef`: a path names the page until
 * `plan()` exists to mint richer references.
 */
export interface PageRef {
  path: string
}

/** A byte range in a source file. */
export interface Span {
  start: number
  end: number
}

/**
 * A problem in a template or document, pointing at source (invariant 8). `severity`
 * and `code` arrive with the checker (∆C8d) as additive optional fields.
 */
export interface Diagnostic {
  path: string
  span: Span
  message: string
}

/**
 * What a render returns: the page's HTML, or `null` with the runtime diagnostic.
 * `channels` and `deps` are additive fields that arrive with ∆RGQ.
 */
export interface RenderResult {
  html: string | null
  diagnostics: Diagnostic[]
}
