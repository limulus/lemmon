import config from '@limulus/eslint-config'
import { defineConfig, globalIgnores } from 'eslint/config'

export default defineConfig([
  ...config,
  globalIgnores(['**/dist/**/*', 'coverage/**']),
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
