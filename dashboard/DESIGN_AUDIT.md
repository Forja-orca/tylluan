# Design Audit — Tylluan Dashboard
**Date:** 2026-07-30 | **Auditor:** Mimo

---

## 1. Current State Documentation

### Color Tokens (CSS Variables, Dark Mode)

| Token | HSL | Hex Approx | Usage |
|-------|-----|------------|-------|
| `--background` | `222.2 84% 4.9%` | #0B0F17 | Page background |
| `--slate-950` | `229 84% 5%` | #0B0F17 | Deepest surface |
| `--slate-900` | `222 47% 11%` | #151B28 | Card backgrounds |
| `--slate-800` | `217 33% 17%` | #222B3D | Borders, subtle surfaces |
| `--slate-700` | `215 25% 27%` | #384256 | Hover borders |
| `--slate-500` | `215 16% 47%` | #6B7280 | Muted text |
| `--slate-400` | `215 20% 65%` | #94A3B8 | Secondary text |
| `--slate-200` | `214 32% 91%` | #E2E8F0 | Bright text |
| `--slate-100` | `210 40% 96%` | #F1F5F9 | Primary text |

**Hardcoded backgrounds (found in components):**
- `#0B0F17` — CoherenceGatePanel, FrictionPanel, NodesTab, FederationTab, GuildInspector
- `#06080d` — ColoquioCanvasWorkspace (docs, whiteboard, knowledge)
- `#030712` — ColoquioCanvasWorkspace diff view
- `#0d1017` — ColoquioTab, ColoquioMessagesPanel
- `#040918` — KnowledgeGraphTab loading state
- `#0f1117` — ColoquioTab container
- `#0f172a` — Tailwind slate-950 (used in code previews)

**Accent Colors:**

| Color | HSL (Dark) | Hex | Role |
|-------|-----------|-----|------|
| Cyan (signature) | `174 100% 41%` | #00F5D4 | Primary accent — buttons, icons, badges, progress, highlights |
| Hot Pink | `327 100% 62%` | #FF2E93 | Alerts, errors, destructive actions (CoherenceGate, GuildInspector) |
| Emerald | `160 84% 39%` | #10B981 | Success, active states, selection ring |
| Amber | `38 92% 50%` | #F59E0B | Warnings, idle states |
| Red | `0 84% 60%` | #EF4444 | Errors, offline, destructive |
| Rose | `350 89% 60%` | #F43F5E | Error variant |
| Indigo | `239 84% 67%` | #6366F1 | Tab active states, special badges |
| Violet | `258 90% 66%` | #8B5CF6 | Agent identity, special markers |
| Blue | `213 94% 68%` | #3B82F6 | Links, graph edges, sparklines |

**Body gradient (index.css):**
```css
background-image:
  radial-gradient(at 0% 0%, rgba(16, 185, 129, 0.05) 0px, transparent 50%),
  radial-gradient(at 100% 0%, rgba(59, 130, 246, 0.05) 0px, transparent 50%);
```

### Typography

- **Font stack:** Tailwind default `ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, "Noto Sans", sans-serif`
- **No custom font imports** — no Google Fonts, no self-hosted fonts
- **Monospace:** Used pervasively (`font-mono`) for data values, labels, badges, tables, forms — essentially every interactive or data-bearing element
- **Scale:** Predominantly `text-[8px]` through `text-xs` (12px). Very few `text-sm` (14px) or larger
- **Headings:** `tracking-tight`, `uppercase`, `tracking-wider`/`tracking-widest`
- **Selection:** `::selection { background: emerald-500/30; color: emerald-100; }`

### Spacing & Radius

- `--radius: 0.5rem` (8px) via CSS variable
- Cards: `rounded-2xl` (16px) or `rounded-xl` (12px) — consistent
- Badges: `rounded` (4px) or `rounded-lg` (8px)
- Buttons: `rounded-xl` (12px)
- Inputs: `rounded-xl` (12px)

### Layout Patterns

- Sidebar navigation (left)
- Tab-based content switching
- Cards with `border border-slate-800/80` and `bg-[#0B0F17]/90`
- Glassmorphism: `bg-slate-900/40 backdrop-blur-md border border-slate-800/50`
- Grid layouts: `grid grid-cols-1 lg:grid-cols-3 gap-6`

---

## 2. Evaluation Against "Generic AI Dashboard" Patterns

### Pattern: Near-pure black + single neon accent
**Verdict: YES, this is the dominant pattern.**
The dashboard is `#0B0F17` (essentially #000 with a blue tint) with `#00F5D4` cyan as the single accent used for *everything* — buttons, icons, badges, borders, progress bars, hover states, text highlights. This is the single most recognizable "AI dashboard" visual signature. The pink `#FF2E93` adds a second neon but serves the same role (just for destructive/error states).

**What it communicates:** "This is a technical monitoring tool."  
**What it doesn't communicate:** "This is Tylluan. This is an owl. This sees what others can't."

### Pattern: Inter/Space Grotesk as "safe" font
**Verdict: NO — but only by omission.**  
The dashboard uses the system font stack (no custom font). This is actually better than importing Inter, but it means there's zero typographic personality. The system sans-serif is the most generic choice possible.

### Pattern: Monospace everywhere
**Verdict: YES, aggressively.**  
`font-mono` is applied to data values, labels, badges, table headers, form inputs, status indicators, section headings, and tooltips. When everything is monospace, nothing is monospace — the "technical precision" signal becomes noise. The mono font is doing the work that color and weight should be doing.

### Pattern: Everything centered
**Verdict: Partially.**  
Headers are left-aligned (good), but empty states, loading states, and many card layouts center everything. The overview tab is well-structured, but sub-panels often fall into center-everything patterns.

### Pattern: `rounded-lg` on everything
**Verdict: YES — `rounded-xl` (12px) on everything.**  
Every card, button, input, badge, and container uses the same border radius. There's no hierarchy through shape — a small badge has the same roundness as a full-width card.

### Pattern: Accent bar on cards
**Verdict: Not exactly, but close.**  
Cards don't have literal left-border accent bars, but they do have uniform `border border-slate-800/80` borders that create the same visual monotony. Every card looks like every other card.

### Pattern: Tiny text (8-11px)
**Verdict: YES, extreme.**  
The dashboard is dominated by `text-[8px]`, `text-[9px]`, `text-[10px]`, and `text-[11px]`. This creates a "surveillance dashboard" feel that reads as dense and hard to scan, not sophisticated.

### Additional: No visual hierarchy through size
Everything is the same size. A section heading is 10px uppercase. A data label is 10px uppercase. A badge is 9px uppercase. When everything shouts the same volume, the user has no guidance on where to look first.

---

## 3. Proposed Direction: "Nocturnal Observatory"

### Core Idea

Tylluan is an owl. Owls see in the dark — not because they like darkness, but because they see things others miss. The dashboard should feel like looking through an owl's eyes: warm amber light cutting through deep blue-black darkness. Not a terminal. Not a hacker tool. An observatory.

### The Problem With Current Palette

Cyan (#00F5D4) is used for *everything*: buttons, icons, badges, borders, progress bars, hover states, focus rings, text highlights. When one color does all the work, it does none of it well. The eye has nothing to anchor to. The user can't distinguish "this is important" from "this exists."

### The Proposal

**Replace the single-cyan-everything pattern with a three-tier light hierarchy:**

| Tier | Color | Hex | Role | Analogy |
|------|-------|-----|------|---------|
| **Primary (Amber)** | Warm gold | #D4A030 → #E8B84A | Actions, focus, active states, primary accent | Owl's eye catching light |
| **Signal (Cyan)** | Reserved cool cyan | #00F5D4 | Live data, active connections, real-time signals only | The thing being observed |
| **Quiet (Slate-400)** | Muted neutral | #94A3B8 | Labels, secondary text, passive UI | The darkness the owl sees through |

**Background: Deep blue-black (keep #0B0F17 — this is fine)**  
The background works. The problem is what's on top of it.

**Surface hierarchy (instead of uniform #0B0F17 cards):**

| Surface | Token | Purpose |
|---------|-------|---------|
| Base | `#0B0F17` | Page background |
| Raised | `#111827` (slate-900) | Cards, panels |
| Emphasized | `#1E293B` (slate-800) | Active card, hover state |
| Overlay | `#0F172A` + blur | Modals, popovers |

### Typography: One Font, Two Weights

**Drop the monospace monoculture.** Use monospace *only* for:
- Code blocks / previews
- Data values that change (numbers, IDs, timestamps)
- Inline technical references (endpoint paths, command names)

**Use the system sans-serif for everything else** — but with intentional weight contrast:
- **Section headings:** `font-semibold` (600), not uppercase, not mono
- **Card titles:** `font-medium` (500), 13-14px
- **Labels:** `font-medium` (500), 11-12px, mixed case (not ALL CAPS)
- **Data values:** `font-mono font-semibold`, 12-13px (slightly larger than current)
- **Body text:** `font-normal` (400), 13px (larger than current 10-11px)

### Specific Changes

1. **Buttons:** Amber background for primary actions (`bg-amber-600 hover:bg-amber-500`), cyan only for "live signal" buttons (start/stop live processes)
2. **Badges:** Amber text on amber/10 bg for status, cyan only for "connected/online" indicators
3. **Icons:** Amber for interactive icons (clickable), cyan for status indicators (read-only), slate-400 for decorative
4. **Cards:** Remove uniform border. Use subtle `bg-slate-900/60` without border for standard cards, add `border border-amber-500/20` only for the *one* most important card on screen
5. **Focus rings:** Amber (`ring-amber-500/50`) instead of cyan
6. **Text size floor:** Raise minimum from 8px to 10px. Section headings at 12-13px
7. **Mixed case labels:** Replace `UPPERCASE TRACKING-WIDER` with `Sentence case` for section labels. Reserve uppercase for truly urgent indicators

### Why This Works for Tylluan

- **Owl metaphor:** Amber eyes in darkness — warm light in cold space
- **"See what others can't":** The cyan becomes *special* again — it only appears when something is actually live/active/connected. Users learn: "cyan = something is happening right now"
- **Sovereignty/local:** The warm palette feels human, owned, intentional — not a cloud SaaS dashboard
- **Not generic:** No AI dashboard uses amber as primary. The combination of warm gold + deep blue-black + reserved cyan is distinctive

### Anti-Pattern Checklist

| Pattern | Current | Proposed | Status |
|---------|---------|----------|--------|
| Near-black + single neon | ✅ Yes (cyan) | ❌ Three-tier hierarchy | Fixed |
| Monospace everywhere | ✅ Yes | ❌ Reserved for data values | Fixed |
| Everything centered | ⚠️ Partial | ❌ Left-aligned headings, centered data | Fixed |
| Same radius everywhere | ✅ Yes (rounded-xl) | ❌ Radius hierarchy (sm/md/lg) | Fixed |
| Tiny text (8px) | ✅ Yes | ❌ Floor raised to 10px | Fixed |
| No visual hierarchy | ✅ Yes | ❌ Size + weight + color hierarchy | Fixed |
| Accent bar on cards | ⚠️ Uniform borders | ❌ Border only on important card | Fixed |

---

## 4. Mockup: Overview "Golden Signals" Card

**Before (current):**
```
┌─────────────────────────────────┐
│ ⚡ KERNEL                       │
│ v0.15.0          [ok]           │
│ ┌──────┐ ┌──────┐ ┌──────┐    │
│ │Uptime│ │ CPU  │ │ Mem  │    │
│ │ 2h14m│ │  12% │ │ 34%  │    │
│ └──────┘ └──────┘ └──────┘    │
│ ┌──────┐                       │
│ │Guilds│                       │
│ │ 8/10 │                       │
│ └──────┘                       │
└─────────────────────────────────┘
All text: 10px mono, all caps labels
All borders: border-slate-800
Accent: cyan on icons, no hierarchy
```

**After (proposed):**
```
┌─────────────────────────────────┐
│ ⚡ Kernel                  ok   │ ← 13px semibold, amber icon
│                                 │
│ Uptime        CPU         Memory│ ← 11px medium, mixed case, slate-400
│ 2h 14m         12%         34%  │ ← 13px mono semibold, white
│                                 │
│ Guilds                         │
│ 8 / 10 online                  │ ← "online" in amber
└─────────────────────────────────┘
Card: bg-slate-900/60, no border (default)
If this is the focused card: border border-amber-500/20
Primary accent: amber on icon + "ok" badge
Data values: mono, larger, white
Labels: mixed case, muted
```

---

## 5. Implementation Scope

**This proposal does NOT require:**
- New font imports
- New CSS framework
- Changes to the component architecture
- Changes to the Tailwind config structure

**It DOES require:**
1. Updating CSS variables (index.css) — add amber token scale, adjust slate if needed
2. Updating tailwind.config.js — add amber scale tokens
3. A pass through each component to:
   - Replace `font-mono` on labels/headings with sans-serif
   - Replace `text-[8px]`/`text-[9px]` with `text-[10px]` or larger
   - Replace `UPPERCASE tracking-wider` labels with mixed case
   - Replace cyan accents on buttons/badges with amber (keep cyan for live signals only)
   - Add radius hierarchy (smaller badges, larger cards)
4. Updating the body gradient to use amber instead of emerald: `rgba(212, 160, 48, 0.03)`

**Estimated effort:** 2-3 hours for a focused pass across ~20 core components. No architectural changes.

---

*Propuesta lista para revisión por José. No implementar hasta su aprobación.*
