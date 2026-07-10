# Tylluan Architecture Visualization

Standalone Next.js site with 7 interactive SVG diagrams explaining Tylluan's architecture, based on the research report (8 papers, 2025-2026): system map, FSRS memory model, retrieval pipeline, federation mesh, sleep cycle, dispatch flow, and roadmap.

This is a documentation/presentation artifact — independent of the main React dashboard (`dashboard/`). Self-contained `package.json`, does not affect the kernel or the main dashboard build.

## Run locally

```bash
cd docs-site
npm install
npm run dev   # http://localhost:3010
```

## Build

```bash
npm run build && npm start
```
