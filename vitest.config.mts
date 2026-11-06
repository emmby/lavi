import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    clearMocks: true,
    setupFiles: ['./tests/vitest.setup.ts'],
    exclude: ['**/node_modules/**', 'tests/e2e/**', 'tests/upstream/**'],
  },
})
