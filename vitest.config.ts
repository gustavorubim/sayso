import react from '@vitejs/plugin-react'
import { defineConfig } from 'vitest/config'

export default defineConfig({
  plugins: [react()],
  test: {
    // Interface tests ask for a browser-like environment themselves, with a
    // `@vitest-environment jsdom` comment at the top of the file.
    include: ['src/**/*.test.{ts,tsx}']
  }
})
