import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    clearMocks: true,
    exclude: ['**/node_modules/**', 'tests/e2e/**', 'tests/upstream/**'],
  },
})
