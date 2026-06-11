import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { VitePWA } from "vite-plugin-pwa";

// The PWA is served by songarr under /wave/ in production, so all asset URLs
// are base-relative. In dev, Vite proxies the API paths to the running dev
// proxy (config.example.toml dev instance) so the app stays same-origin.
const SONGARR = process.env.SONGARR_URL ?? "http://127.0.0.1:4534";

export default defineConfig({
  base: "/wave/",
  plugins: [
    react(),
    tailwindcss(),
    VitePWA({
      registerType: "autoUpdate",
      // Cache the app shell only; audio and the wave API must stay live.
      workbox: {
        navigateFallbackDenylist: [/^\/rest/, /^\/wave\/api/],
        runtimeCaching: [],
      },
      manifest: {
        name: "Songarr — Твоя волна",
        short_name: "Волна",
        description: "Endless personalized music from your Songarr library",
        start_url: "/wave/",
        scope: "/wave/",
        display: "standalone",
        background_color: "#0b0b0f",
        theme_color: "#0b0b0f",
        icons: [
          { src: "icon-192.png", sizes: "192x192", type: "image/png" },
          { src: "icon-512.png", sizes: "512x512", type: "image/png" },
          {
            src: "icon-512.png",
            sizes: "512x512",
            type: "image/png",
            purpose: "maskable",
          },
        ],
      },
    }),
  ],
  server: {
    proxy: {
      "/rest": { target: SONGARR, changeOrigin: true },
      "/wave/api": { target: SONGARR, changeOrigin: true },
    },
  },
});
