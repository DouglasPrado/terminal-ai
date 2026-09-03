import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  // Component tests need a DOM. `@testing-library/react` was already a devDependency with no
  // environment to run in; feature 003 is the first to render a component under test.
  test: { environment: "jsdom" },
});
