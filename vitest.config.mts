import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    globals: true,
    clearMocks: true,
    setupFiles: ['./tests/vitest.setup.ts'],
    exclude: ['**/node_modules/**', 'tests/e2e/**', 'tests/upstream/**', '.tmp/**'],
  },
})
