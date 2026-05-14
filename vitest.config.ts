import path from "path"
import { defineConfig } from "vitest/config"

export default defineConfig({
  test: {
    environment: "jsdom",
    globals: false,
    include: ["src/**/*.{test,spec}.{ts,tsx}"],
    exclude: ["**/node_modules/**", "**/dist/**", "src-tauri/**"],
    setupFiles: ["./src/test-utils/setup.ts"],
    clearMocks: true,
    restoreMocks: true,
    coverage: {
      provider: "v8",
      reporter: ["text", "lcov", "html"],
      reportsDirectory: "./coverage",
      include: ["src/**/*.{ts,tsx}"],
      exclude: [
        "src/**/*.{test,spec}.{ts,tsx}",
        "src/test-utils/**",
        "src/main.tsx",
        "src/vite-env.d.ts",
      ],
    },
  },
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
})
