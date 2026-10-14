/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig(({ mode }) => ({
  plugins: [react()],
  clearScreen: false,
  server: {
    host: "127.0.0.1",
    port: 1420,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test-setup.ts"],
    // Room for the longer asynchronous waits configured in the setup file.
    testTimeout: 20000,
  },
  build: {
    // Tauri webviews support ES2020; one coherent language target avoids
    // toolchain-specific downlevel transforms while retaining broad support.
    target: "es2020",
    minify: mode === "development" ? false : "esbuild",
    sourcemap: mode === "development",
  },
}));
