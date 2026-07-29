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
  build: {
    outDir: "../dashboard/dist",
    emptyOutDir: true,
    rollupOptions: {
      output: {
        manualChunks: {
          'vendor-react': ['react', 'react-dom'],
          'vendor-three': ['three', 'react-force-graph-3d'],
          'vendor-tldraw': ['@tldraw/tldraw'],
          'vendor-lucide': ['lucide-react'],
        },
      },
    },
  },
  server: {
    // Force IPv4 loopback. Node/Vite defaulting to IPv6-only (`::1`) here is
    // what broke the HMR websocket ("WebSocket connection to 'ws://localhost:5174/'
    // failed") -- the browser's `localhost` and the bound interface didn't agree
    // on a stack. Same class of bug as the documented server-side one (Windows
    // resolves `localhost` to ::1 before 127.0.0.1) but hitting the dev server
    // instead of the kernel.
    host: '127.0.0.1',
    proxy: {
      // The dashboard's actual SSE endpoint (nexus-bridge.ts) is /api/v1/events,
      // not /sse (that's the separate MCP transport endpoint for MCP clients --
      // unused by the dashboard). Needs explicit anti-buffering headers or
      // Node's proxy can sit on the stream and the UI never sees events.
      '/api/v1/events': {
        target: 'http://127.0.0.1:4000',
        changeOrigin: true,
        configure: (proxy) => {
          proxy.on('proxyRes', (_proxyRes, _req, res) => {
            res.setHeader('Cache-Control', 'no-cache');
            res.setHeader('Connection', 'keep-alive');
            res.setHeader('X-Accel-Buffering', 'no');
          });
        },
      },
      '/api': {
        target: 'http://127.0.0.1:4000',
        changeOrigin: true,
      },
      '/health': {
        target: 'http://127.0.0.1:4000',
        changeOrigin: true,
      },
    },
  },
})
