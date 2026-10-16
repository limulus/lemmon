import { defineConfig } from 'vitest/config'

// Projects are listed inline with `extends: true` so the root options below actually
// reach them; a bare `projects: ['packages/*']` glob would start each package with a
// blank config and silently ignore everything set here.
const packages = ['core', 'shell']

const project = (name: string, root: string) => ({
  extends: true as const,
  test: { name, root },
})

export default defineConfig({
  // A spec that imports a sibling package at run time gets its `src/`, as `tscc` and the
  // `lem` bin do. The shell's own modules import core for types alone so far, so only
  // its specs reach core this way: one compiles through the staged wasm. Specs run in
  // Vite's server environment, which reads `ssr.resolve` and ignores a top-level
  // `resolve.conditions`; the list is Vite's server defaults with `source` in front,
  // since setting it replaces them.
  ssr: { resolve: { conditions: ['source', 'module', 'node', 'development|production'] } },
  test: {
    coverage: {
      provider: 'v8',
      reporter: ['html', 'json', 'text', 'text-summary'],
      include: ['packages/*/src/**/*.ts', 'scripts/lib/**/*.ts'],
      exclude: ['**/node_modules/**', '**/mocks/**', '**/*.spec.ts'],
      thresholds: {
        branches: 100,
        functions: 100,
        lines: 100,
        statements: 100,
      },
    },
    environment: 'node',
    exclude: ['**/node_modules/**', '**/dist/**'],
    projects: [
      ...packages.map((name) => project(`@limulus/lemmon-${name}`, `packages/${name}`)),
      // `scripts/lib/` holds logic the repo scripts share; unlike the scripts
      // themselves it is covered like a package.
      project('scripts', 'scripts'),
    ],
  },
})
