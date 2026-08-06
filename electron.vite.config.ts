import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "electron-vite";
import path from "node:path";
import packageMetadata from "./package.json";

export default defineConfig(({ command }) => {
  const isDevelopment = command === "serve";
  return {
    main: {
      build: {
        externalizeDeps: { exclude: ["electron-squirrel-startup"] }
      }
    },
    preload: {},
    renderer: {
      base: "./",
      resolve: {
        alias: {
          "@": path.resolve(__dirname, "src/renderer/src")
        }
      },
      define: {
        __APP_VERSION__: JSON.stringify(packageMetadata.version)
      },
      plugins: [
        react({}),
        tailwindcss(),
        {
          name: "development-csp",
          transformIndexHtml(html) {
            if (!isDevelopment) return html;
            return html
              .replace("script-src 'self'", "script-src 'self' 'unsafe-inline'")
              .replace(
                "connect-src 'self'",
                "connect-src 'self' ws://localhost:* http://localhost:*"
              );
          }
        }
      ]
    }
  };
});
