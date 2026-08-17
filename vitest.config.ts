import { defineConfig } from 'vitest/config'

// Projects are listed inline with `extends: true` so the root options below actually
// reach them; a bare `projects: ['packages/*']` glob would start each package with a
// blank config and silently ignore everything set here.
const packages = ['core', 'shell']

export default defineConfig({
  test: {
    coverage: {
      provider: 'v8',
      reporter: ['html', 'json', 'text', 'text-summary'],
      include: ['packages/*/src/**/*.ts'],
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
    projects: packages.map((name) => ({
      extends: true,
      test: { name: `@limulus/lemmon-${name}`, root: `packages/${name}` },
    })),
  },
})
