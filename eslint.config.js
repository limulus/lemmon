import config from '@limulus/eslint-config'
import { defineConfig, globalIgnores } from 'eslint/config'

export default defineConfig([
  ...config,
  // The core's `artifacts/` holds generated wasm-bindgen output, and `target/` is cargo's
  // build tree, where `cargo doc` writes a hundred-odd `.js` files under `target/doc/`.
  // Neither is source.
  globalIgnores(['**/dist/**/*', 'coverage/**', 'packages/core/artifacts/**', 'target/**']),
  {
    files: ['**/*.ts', '**/*.tsx', '**/*.d.ts', '**/*.js'],

    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
  },
])
