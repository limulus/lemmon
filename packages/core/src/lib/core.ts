import type { BoundaryCore } from './boundary.ts'
import type {
  CompileResult,
  DataInput,
  Diagnostic,
  HostDecl,
  PageRef,
  RenderResult,
  SourceFileInput,
} from './types.ts'

/** The reply's header: everything a render returns except the page itself. */
type RenderMeta = Omit<RenderResult, 'html'>

const decoder = new TextDecoder()

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
   * the program. No `error` diagnostic means success: a program is still produced when
   * the only diagnostics are warnings, and on failure the previous program stays
   * renderable, stale until fixed.
   *
   * The diagnostics describe the files passed to this call (plus any resident file the
   * relink found stale), not the whole program: a resident file's warnings were
   * reported by the call that compiled it. A caller that shows warnings keeps them per
   * path and replaces a path's entry when it recompiles that path.
   *
   * The payloads are those of every code block the files that compiled write, each
   * once (D42) — including a file that compiled beside one that did not, since it will
   * not be compiled again. A caller keeps them by id, which is what a render's `code`
   * mark names, and a producer's output is cached on that id.
   *
   * `host` is the ambient declaration (D30), and is resident like the units: a call
   * that changes nothing about the environment omits it, and one that carries it
   * replaces what the core holds. A core given none has no ambient values at all, so
   * `page` in a template is an undefined name like any other.
   *
   * A compile that installs a new program drops the data loaded against the old one:
   * call {@link loadData} again before {@link render}, or an ambient read fails as
   * not bound.
   */
  compile(files: SourceFileInput[], host?: HostDecl): CompileResult {
    const declaration = host === undefined ? undefined : JSON.stringify(host)
    return JSON.parse(
      this.#core.compile(JSON.stringify(files), declaration)
    ) as CompileResult
  }

  /**
   * Loads the data every render binds, validating it against the program (D37): each
   * value against the type the host or a `from` binding declared for it, omitted
   * fields filled by their defaults, references resolved through their `by` indexes,
   * and every record tagged with its type. No `error` diagnostic means the data is
   * loaded; on an error the previous data stays bound, stale until fixed, as the
   * program does after a failed compile. A load diagnostic points at the positions
   * the host sent with the value, or at the declaration that expected it when the
   * host sent none.
   *
   * A throw is a caller bug: an envelope the program cannot be loaded from — a name
   * the host does not bind, a source no `from` binding asked for, positions with no
   * file to place them in, a document the program never compiled — a load before any
   * compile has succeeded, or one after the declaration changed without a compile
   * succeeding under it.
   */
  loadData(input: DataInput): Diagnostic[] {
    return JSON.parse(this.#core.loadData(JSON.stringify(input))) as Diagnostic[]
  }

  /**
   * Renders one page: its bytes with the marks standing in them and the fragments it
   * collected, or `null`s alongside the runtime diagnostic. A loaded document renders
   * with the host's document value bound to its own record; any other file reads the
   * value the envelope carried, if it did.
   *
   * The reply crosses as a four-byte length, that many bytes of metadata JSON, then the
   * page. `html` is a view onto those bytes, so a caller reading `html.buffer` must
   * honour `byteOffset`; it is `null` only where the render failed, an empty page being
   * an empty view.
   */
  render(page: PageRef): RenderResult {
    const reply = this.#core.render(JSON.stringify(page))
    const length = new DataView(reply.buffer, reply.byteOffset, 4).getUint32(0, true)
    const meta = JSON.parse(decoder.decode(reply.subarray(4, 4 + length))) as RenderMeta
    return {
      ...meta,
      html: meta.marks === null ? null : reply.subarray(4 + length),
    }
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
