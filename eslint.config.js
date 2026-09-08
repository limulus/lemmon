import config from '@limulus/eslint-config'
import { defineConfig, globalIgnores } from 'eslint/config'

export default defineConfig([
  ...config,
  // `artifacts/` holds generated wasm-bindgen output; `target/` is cargo's build tree,
  // where `cargo doc` writes a hundred-odd `.js` files under `target/doc/`. Not source.
  globalIgnores(['**/dist/**/*', 'coverage/**', 'artifacts/**', 'target/**']),
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
