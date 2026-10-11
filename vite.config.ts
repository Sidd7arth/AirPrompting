import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";
import { resolve } from "node:path";

import { APP_NAME, LEGACY_STORAGE_PREFIX, STORAGE_PREFIX } from "./src/identity";

// Tauri expects a fixed dev port and must not clear the screen on error.
const host = process.env.TAURI_DEV_HOST;

/**
 * The HTML entries cannot import a module, so they carry `%TOKEN%` placeholders
 * that are filled here from `src/identity.ts` — the same single source the React
 * code reads. Without this, the window title and the theme key would be the two
 * places a future rename would miss.
 */
function identityTokens(): Plugin {
  const tokens: Record<string, string> = {
    APP_NAME,
    STORAGE_PREFIX,
    LEGACY_STORAGE_PREFIX,
  };

  return {
    name: "airprompt-identity-tokens",
    transformIndexHtml: {
      order: "pre",
      handler: (html) =>
        Object.entries(tokens).reduce(
          (markup, [token, value]) =>
            markup.replace(new RegExp(`%${token}%`, "g"), value),
          html
        ),
    },
  };
}

export default defineConfig({
  plugins: [react(), identityTokens()],
  clearScreen: false,
  resolve: {
    alias: { "@": resolve(__dirname, "src") },
  },
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    // Tauri uses Chromium (WebView2 on Windows, WKWebView on macOS).
    target: process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome105" : "safari15",
    minify: !process.env.TAURI_ENV_DEBUG ? "esbuild" : false,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    rollupOptions: {
      input: {
        // Main dashboard window.
        main: resolve(__dirname, "index.html"),
        // Always-on-top dictation overlay.
        overlay: resolve(__dirname, "overlay.html"),
        // Public download page (not bundled into the app).
        landing: resolve(__dirname, "landing.html"),
      },
    },
  },
});
