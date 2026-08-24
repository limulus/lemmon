import type { BoundaryCore } from './boundary.ts'
import type {
  DataInput,
  Diagnostic,
  PageRef,
  RenderResult,
  SourceFileInput,
} from './types.ts'

/**
 * The typed face of the WASM core: one resident instance per worker holding the
 * compiled program and its data across calls. Methods marshal their input to the one
 * JSON string each boundary call carries and parse the reply; a throw from the
 * boundary — a caller bug by construction — is left to propagate.
 */
export class Core {
  readonly #core: BoundaryCore

  constructor(core: BoundaryCore) {
    this.#core = core
  }

  /**
   * Compiles the given files — only the changed ones — into resident units and relinks
   * the program. Empty diagnostics mean success; on failure the previous program stays
   * renderable, stale until fixed.
   */
  compile(files: SourceFileInput[]): Diagnostic[] {
    return JSON.parse(this.#core.compile(JSON.stringify(files))) as Diagnostic[]
  }

  /**
   * Stores the data every render binds. Diagnostics are empty until schemas exist to
   * validate against (∆C8d).
   */
  loadData(input: DataInput): Diagnostic[] {
    return JSON.parse(this.#core.loadData(JSON.stringify(input))) as Diagnostic[]
  }

  /** Renders one page: its HTML, or `null` alongside the runtime diagnostic. */
  render(page: PageRef): RenderResult {
    return JSON.parse(this.#core.render(JSON.stringify(page))) as RenderResult
  }

  /**
   * Releases this core's memory inside the shared wasm instance. The compiled program
   * and its data are resident, so a caller that keeps making cores — a dev server
   * rebuilding, a worker cycling — must release the ones it drops or the instance grows
   * without bound. Using the core after this throws.
   */
  free(): void {
    this.#core.free()
  }
}
