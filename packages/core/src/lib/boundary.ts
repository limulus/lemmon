/**
 * The wasm-bindgen glue module's shape, written by hand so that typechecking never
 * needs the generated `.d.ts` — which only exists once `npm run rust:wasm` has staged
 * the artifact. The specs, which load the real glue, are what hold this to the truth.
 */
export interface WasmBoundary {
  /** Instantiates the module; idempotent — later calls return the first instance. */
  initSync(module: { module: BufferSource | WebAssembly.Module }): unknown
  /** The version of the core the `.wasm` was built from. */
  version(): string
  /** The resident core class; construct one per worker. */
  Core: new () => BoundaryCore
}

/**
 * The raw `Core` the glue exposes: every call carries its bulk data as one JSON string
 * (few large calls), and a throw is a caller bug, never a template problem.
 */
export interface BoundaryCore {
  compile(files: string, host?: string): string
  loadData(input: string): string
  render(page: string): string
  /** Releases the instance's memory on the wasm side. */
  free(): void
}
