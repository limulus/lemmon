import config from '@limulus/eslint-config'
import { defineConfig, globalIgnores } from 'eslint/config'

export default defineConfig([
  ...config,
  // `artifacts/` holds generated wasm-bindgen output, not source.
  globalIgnores(['**/dist/**/*', 'coverage/**', 'artifacts/**']),
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
