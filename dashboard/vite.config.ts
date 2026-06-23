import { nodePolyfills } from 'vite-plugin-node-polyfills'
import react from "@vitejs/plugin-react"
import { defineConfig } from "vite"

export default defineConfig({
  plugins: [
    react(),
    nodePolyfills({
      include: ['path', 'fs', 'util'],
      globals: {
        Buffer: true,
        global: true,
        process: true,
      },
    }),
  ],
  server: {
    port: 5174,
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:3033',
        changeOrigin: true,
      },
      '/health': {
        target: 'http://127.0.0.1:3033',
        changeOrigin: true,
      },
      '/sse': {
        target: 'http://127.0.0.1:3033',
        changeOrigin: true,
        ws: true,
      },
      '/messages': {
        target: 'http://127.0.0.1:3033',
        changeOrigin: true,
      },
    },
  },
})
