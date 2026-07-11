# Docs-Site Deploy Plan (postponed)

## What
`docs-site/` — Next.js standalone app with 7 SVG architecture diagrams. Built and committed.

## Recommended host
**Vercel** (free tier). Zero config — connected Next.js repo auto-detects.

## Steps (when ready)
1. Push repo to GitHub (if not already)
2. Go to vercel.com → Import repo → Select `docs-site/` as root directory
3. Build command: `npm run build` (default)
4. Output: `.next` (default)
5. Env: none required (static content)
6. Deploy — each `main` push auto-redeploys

## Alternative
Cloudflare Pages: works too, but needs framework preset set to "Next.js" manually.
GitHub Pages: NOT recommended for Next.js (requires `next export` which is deprecated).

## Port
Dev: `npm run dev -p 3010` · Prod: `npm run start -p 3010`
