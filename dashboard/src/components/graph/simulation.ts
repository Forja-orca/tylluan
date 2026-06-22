/**
 * HippocampusGraph — Living memory visualization
 *
 * Canvas 2D · Custom force simulation · Zero graph library dependencies
 * Simulation engine extracted for cleaner component separation.
 *
 * - New nodes appear live via SSE events (memory_added) — no manual reload
 * - Nodes fire and propagate signals when the kernel uses them
 * - The graph breathes, drifts, and pulses like living neural tissue
 * - Incremental refresh every 20s catches any nodes missed by events
 * - Zoom: scroll wheel (zoom-to-cursor) · Pan: click+drag on empty space
 */

import type { NexusBridge, GraphNode, NexusEvent } from '../../lib/nexus-bridge';

// ─── Visual constants ─────────────────────────────────────────────────────────

export const PALETTE: Record<string, string> = {
  concept:    '#22c55e',
  episode:    '#3b82f6',
  lesson:     '#f59e0b',
  experience: '#3b82f6',
  identity:   '#f59e0b',
  tool_call:  '#8b5cf6',
  agent:      '#ec4899',
  image:      '#f97316',
  document:   '#06b6d4',
  system:     '#94a3b8',
  agnostic:   '#6b7280',
};
export const DEFAULT_COLOR  = '#6b7280';

// Louvain cluster glow colors (ring around node, distinct from node_type color)
export const CLUSTER_COLORS = [
  '#10b981', '#3b82f6', '#8b5cf6', '#f59e0b', '#ef4444',
  '#06b6d4', '#ec4899', '#84cc16', '#6366f1', '#f43f5e',
];
export function clusterColor(id?: number): string | null {
  if (id === undefined || id === null) return null;
  return CLUSTER_COLORS[id % CLUSTER_COLORS.length];
}

// Cluster gravity strength — pulls nodes toward community centroid
export const CLUSTER_GRAVITY = 0.0006;
const BG_CENTER      = '#030d1e';
export const BG_EDGE_COLOR  = '#010409';
const TAU            = Math.PI * 2;

const REPULSION      = 2200;
const SPRING_LEN     = 85;
const SPRING_K       = 0.038;
const GRAVITY        = 0.003;
const DAMPING        = 0.80;
export const DRIFT_STRENGTH = 0.06;
export const ENERGY_STABLE  = 0.12;

export const ZOOM_MIN = 0.15;
export const ZOOM_MAX = 6.0;

export const LAYOUT_STORAGE_KEY = 'tylluan_hippocampus_v2';
export const LAYOUT_SAVE_DEBOUNCE_MS = 3000;

// ─── localStorage layout persistence ─────────────────────────────────────────

interface StoredLayout {
  fingerprint: string; // hash of sorted node ids
  positions:   { id: string; x: number; y: number }[];
  ts:          number;
}

export function graphFingerprint(ids: string[]): string {
  return [...ids].sort().join('|').slice(0, 120);
}

export function loadStoredLayout(): StoredLayout | null {
  try {
    const raw = localStorage.getItem(LAYOUT_STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as StoredLayout;
    if (!parsed || !Array.isArray(parsed.positions)) return null;

    // Check if any position contains NaN, non-finite, or abnormally large coordinates.
    // Standard viewport dimensions are ~800x500. A healthy layout might spread slightly beyond,
    // but anything exceeding 100,000 pixels is definitely an exploded physics state.
    const isCorrupt = parsed.positions.some(p =>
      typeof p.x !== 'number' || typeof p.y !== 'number' ||
      !isFinite(p.x) || !isFinite(p.y) ||
      isNaN(p.x) || isNaN(p.y) ||
      Math.abs(p.x) > 100000 || Math.abs(p.y) > 100000
    );

    if (isCorrupt) {
      console.warn("Detected corrupt or exploded coordinates in localStorage layout. Clearing cache to prevent invisible canvas.");
      localStorage.removeItem(LAYOUT_STORAGE_KEY);
      return null;
    }

    return parsed;
  } catch { return null; }
}

export function saveLayout(ns: SimNode[]) {
  if (ns.length === 0) return;
  // Extra safety: Do not save layout if any node has invalid or exploded positions
  const hasCorrupt = ns.some(n =>
    !isFinite(n.x) || !isFinite(n.y) ||
    isNaN(n.x) || isNaN(n.y) ||
    Math.abs(n.x) > 100000 || Math.abs(n.y) > 100000
  );
  if (hasCorrupt) return;

  const layout: StoredLayout = {
    fingerprint: graphFingerprint(ns.map(n => n.id)),
    positions:   ns.map(n => ({ id: n.id, x: n.x, y: n.y })),
    ts:          Date.now(),
  };
  try { localStorage.setItem(LAYOUT_STORAGE_KEY, JSON.stringify(layout)); } catch { /* quota */ }
}

// ─── Types ────────────────────────────────────────────────────────────────────

export interface SimNode {
  id:          string;
  x:           number;
  y:           number;
  vx:          number;
  vy:          number;
  r:           number;
  color:       string;
  label:       string;
  kind:        string;
  weight:      number;
  content?:    string;
  created?:    string;
  birth:       number;
  activation:  number;
  nextFire:    number;
  cluster_id?: number; // Louvain community from kernel
  heat:        number; // access count from traces
  last_agent?: string;
}

export interface SimEdge {
  s:    string;
  t:    string;
  si:   number;
  ti:   number;
  born: number;
}

export interface Particle {
  ei:    number;
  t:     number;
  speed: number;
  alpha: number;
}

export interface Camera {
  x:     number; // world-space pan offset
  y:     number;
  scale: number;
}

export interface Detail {
  id:      string;
  label:   string;
  kind:    string;
  color:   string;
  weight:  number;
  content?: string;
  created?: string;
  last_agent?: string;
}

export interface PathTraceState {
  picks: string[];
  nodes: string[];
  edges: string[];
  found: boolean | null;
  hops:  number;
}

export interface Props {
  bridge:       NexusBridge;
  events?:      NexusEvent[];
  onNodeClick?: (node: GraphNode) => void;
}

export const EMPTY_PATH_TRACE: PathTraceState = {
  picks: [],
  nodes: [],
  edges: [],
  found: null,
  hops:  0,
};

// ─── Pure helpers ─────────────────────────────────────────────────────────────

export function nodeColor(node: GraphNode): string {
  // TMS: deprecated nodes render dimmed
  if (node.content?.startsWith('[DEPRECATED by')) return '#ef4444'; // red-500, semi-transparent
  return PALETTE[node.node_type || node.type || 'agnostic'] ?? DEFAULT_COLOR;
}

export const AGENT_COLOR_HEX = [
  '#22d3ee', // cyan-400
  '#a78bfa', // violet-400
  '#fbbf24', // amber-400
  '#34d399', // emerald-400
  '#fb7185', // rose-400
  '#60a5fa', // blue-400
];

export function getAgentColor(agentId?: string): string {
  if (!agentId) return DEFAULT_COLOR;
  const cleanId = agentId.trim().toLowerCase();
  if (['user', 'human'].includes(cleanId)) {
    return '#34d399'; // emerald-400
  }
  const hash = cleanId.split('').reduce((acc, char) => acc + char.charCodeAt(0), 0);
  return AGENT_COLOR_HEX[Math.abs(hash) % AGENT_COLOR_HEX.length];
}

export function scheduleNextFire(weight: number, now: number, noveltyFactor = 1.0): number {
  const base = 28000 - weight * 24000;
  return now + base * (0.5 + Math.random()) * noveltyFactor;
}

export function edgeKey(a: string, b: string): string {
  return a < b ? `${a}::${b}` : `${b}::${a}`;
}

export function shortestVisiblePath(
  source: string,
  target: string,
  edges: SimEdge[],
): { nodes: string[]; edges: string[]; hops: number } | null {
  if (source === target) {
    return { nodes: [source], edges: [], hops: 0 };
  }

  const adjacency = new Map<string, string[]>();
  for (const e of edges) {
    if (!adjacency.has(e.s)) adjacency.set(e.s, []);
    if (!adjacency.has(e.t)) adjacency.set(e.t, []);
    adjacency.get(e.s)!.push(e.t);
    adjacency.get(e.t)!.push(e.s);
  }

  const visited = new Set<string>([source]);
  const previous = new Map<string, string>();
  const queue = [source];

  for (let qi = 0; qi < queue.length; qi++) {
    const current = queue[qi];
    const neighbors = adjacency.get(current) ?? [];
    for (const next of neighbors) {
      if (visited.has(next)) continue;
      visited.add(next);
      previous.set(next, current);

      if (next === target) {
        const path = [target];
        let cursor = target;
        while (cursor !== source) {
          const parent = previous.get(cursor);
          if (!parent) return null;
          cursor = parent;
          path.push(cursor);
        }
        path.reverse();

        const pathEdges: string[] = [];
        for (let i = 1; i < path.length; i++) {
          pathEdges.push(edgeKey(path[i - 1], path[i]));
        }

        return { nodes: path, edges: pathEdges, hops: Math.max(0, path.length - 1) };
      }

      queue.push(next);
    }
  }

  return null;
}

// ─── Simulation builder ───────────────────────────────────────────────────────

export function buildSim(
  rawNodes: GraphNode[],
  rawEdges: { source?: string; from?: string; target?: string; to?: string }[],
  W: number, H: number,
  existingIdx?: Map<string, SimNode>,
): { nodes: SimNode[]; edges: SimEdge[] } {
  const safeW = isFinite(W) && !isNaN(W) && W > 10 ? W : 800;
  const safeH = isFinite(H) && !isNaN(H) && H > 10 ? H : 500;
  const cx = safeW / 2, cy = safeH / 2;
  const now = Date.now();

  // Pre-compute cluster spawn angles so each community starts in its own sector
  const clusterIds = [...new Set(rawNodes.map(n => (n as any).cluster_id as number | undefined).filter(c => c !== undefined))] as number[];
  const clusterAngle = new Map(clusterIds.map((c, i) => [c, (i / Math.max(1, clusterIds.length)) * TAU]));

  const nodes: SimNode[] = rawNodes.map((n, i) => {
    const w          = isNaN(Number(n.weight)) ? 0.4 : Math.min(1, Number(n.weight));
    const cid        = (n as any).cluster_id as number | undefined;
    const isDeprecated = n.content?.startsWith('[DEPRECATED by');
    const heatVal = (n as any).stigmergy_heat ?? 0;
    const lastAgent = (n as any).last_agent;
    const baseR = isDeprecated ? Math.max(3, w * 6) : Math.max(6, w * 12);
    const r = baseR * (1 + Math.min(heatVal * 0.15, 0.3));

    const existing = existingIdx?.get(n.id);
    if (existing) {
      const ex = isFinite(existing.x) && !isNaN(existing.x) ? existing.x : cx + (Math.random() - 0.5) * 100;
      const ey = isFinite(existing.y) && !isNaN(existing.y) ? existing.y : cy + (Math.random() - 0.5) * 100;
      return {
        id:          n.id,
        x:           ex,
        y:           ey,
        vx:          isFinite(existing.vx) && !isNaN(existing.vx) ? existing.vx : 0,
        vy:          isFinite(existing.vy) && !isNaN(existing.vy) ? existing.vy : 0,
        r:           r,
        color:       nodeColor(n),
        label:       (n.label || n.content?.slice(0, 36) || n.id.slice(0, 10)).replace(/\n/g, ' '),
        kind:        n.node_type || n.type || 'agnostic',
        weight:      w,
        content:     n.content,
        created:     n.created_at,
        birth:       existing.birth || now,
        activation:  existing.activation || 0,
        nextFire:    existing.nextFire || scheduleNextFire(w, now),
        cluster_id:  cid,
        heat:        heatVal,
        last_agent:  lastAgent || existing.last_agent || '',
      };
    }

    const baseAngle  = cid !== undefined ? (clusterAngle.get(cid) ?? 0) : (i / Math.max(1, rawNodes.length)) * TAU;
    const jitter     = ((i % 8) / 8) * 0.6 - 0.3; // spread within community sector
    const angle      = baseAngle + jitter;
    const ring       = Math.min(safeW, safeH) * 0.25 * (0.5 + Math.random() * 0.9);

    return {
      id:          n.id,
      x:           cx + Math.cos(angle) * ring,
      y:           cy + Math.sin(angle) * ring,
      vx:          (Math.random() - 0.5) * 3,
      vy:          (Math.random() - 0.5) * 3,
      r:           r,
      color:       nodeColor(n),
      label:       (n.label || n.content?.slice(0, 36) || n.id.slice(0, 10)).replace(/\n/g, ' '),
      kind:        n.node_type || n.type || 'agnostic',
      weight:      w,
      content:     n.content,
      created:     n.created_at,
      birth:       now,
      activation:  0,
      nextFire:    scheduleNextFire(w, now),
      cluster_id:  cid,
      heat:        heatVal,
      last_agent:  lastAgent || '',
    };
  });

  const nodeIdx = new Map(nodes.map((n, i) => [n.id, i]));
  const edgeSet = new Set<string>();
  const edges: SimEdge[] = [];

  rawEdges.forEach(e => {
    const s = (e.source || e.from) as string;
    const t = (e.target || e.to) as string;
    if (!s || !t) return;
    const si = nodeIdx.get(s) ?? -1;
    const ti = nodeIdx.get(t) ?? -1;
    if (si < 0 || ti < 0) return;
    const key = `${s}→${t}`;
    if (edgeSet.has(key)) return;
    edgeSet.add(key);
    edges.push({ s, t, si, ti, born: now });
  });

  return { nodes, edges };
}

// ─── Physics tick ─────────────────────────────────────────────────────────────

export function tick(
  nodes:         SimNode[],
  edges:         SimEdge[],
  W:             number,
  H:             number,
  stable:        boolean,
  noveltyFactor: number = 1.0,
  driftBoost:    number = 1.0,
): number {
  const safeW = isFinite(W) && !isNaN(W) && W > 10 ? W : 800;
  const safeH = isFinite(H) && !isNaN(H) && H > 10 ? H : 500;
  const cx = safeW / 2, cy = safeH / 2;
  const now = Date.now();

  for (const n of nodes) {
    if (isNaN(n.x) || !isFinite(n.x)) n.x = cx + (Math.random() - 0.5) * 100;
    if (isNaN(n.y) || !isFinite(n.y)) n.y = cy + (Math.random() - 0.5) * 100;
    if (isNaN(n.vx) || !isFinite(n.vx)) n.vx = 0;
    if (isNaN(n.vy) || !isFinite(n.vy)) n.vy = 0;
  }

  for (const n of nodes) {
    if (now >= n.nextFire) {
      n.activation = 0.85 + Math.random() * 0.15;
      n.nextFire   = scheduleNextFire(n.weight, now, noveltyFactor);
    }
    n.activation *= 0.965;
  }

  if (stable) {
    for (const n of nodes) {
      n.vx += (Math.random() - 0.5) * DRIFT_STRENGTH * driftBoost;
      n.vy += (Math.random() - 0.5) * DRIFT_STRENGTH * driftBoost;
      n.vx *= 0.92; n.vy *= 0.92;
      n.x  += n.vx; n.y  += n.vy;
      const pad = n.r + 20;
      const BPAD = 60; const BF = 0.15;
      if (n.x < BPAD) n.vx += BF * (BPAD - n.x);
      if (n.x > W - BPAD) n.vx -= BF * (n.x - (W - BPAD));
      if (n.y < BPAD) n.vy += BF * (BPAD - n.y);
      if (n.y > H - BPAD) n.vy -= BF * (n.y - (H - BPAD));
      n.x = Math.max(0, Math.min(W, n.x));
      n.y = Math.max(0, Math.min(H, n.y));
    }
    return 0;
  }



  for (const e of edges) {
    const a = nodes[e.si], b = nodes[e.ti];
    if (!a || !b) continue;
    const dx = b.x - a.x, dy = b.y - a.y;
    const d  = Math.sqrt(dx * dx + dy * dy) || 1;
    const f  = (d - SPRING_LEN) * SPRING_K;
    const fx = (dx / d) * f, fy = (dy / d) * f;
    a.vx += fx; a.vy += fy;
    b.vx -= fx; b.vy -= fy;
  }

  // Cluster gravity — pull each node toward its community centroid
  // Only active when cluster_ids are present
  const clusterCx = new Map<number, { sx: number; sy: number; n: number }>();
  for (const n of nodes) {
    if (n.cluster_id === undefined) continue;
    const acc = clusterCx.get(n.cluster_id) ?? { sx: 0, sy: 0, n: 0 };
    acc.sx += n.x; acc.sy += n.y; acc.n++;
    clusterCx.set(n.cluster_id, acc);
  }
  for (const n of nodes) {
    if (n.cluster_id === undefined) continue;
    const acc = clusterCx.get(n.cluster_id);
    if (!acc || acc.n < 2) continue;
    const ccx = acc.sx / acc.n, ccy = acc.sy / acc.n;
    n.vx += (ccx - n.x) * CLUSTER_GRAVITY;
    n.vy += (ccy - n.y) * CLUSTER_GRAVITY;
  }

  let energy = 0;
  const pad  = 20;
  for (const n of nodes) {
    n.vx += (cx - n.x) * GRAVITY;
    n.vy += (cy - n.y) * GRAVITY;
    n.vx *= DAMPING; n.vy *= DAMPING;
    n.x  += n.vx;   n.y  += n.vy;
    const BPAD2 = 60; const BF2 = 0.15;
    if (n.x < BPAD2) n.vx += BF2 * (BPAD2 - n.x);
    if (n.x > W - BPAD2) n.vx -= BF2 * (n.x - (W - BPAD2));
    if (n.y < BPAD2) n.vy += BF2 * (BPAD2 - n.y);
    if (n.y > H - BPAD2) n.vy -= BF2 * (n.y - (H - BPAD2));
    n.x = Math.max(0, Math.min(W, n.x));
    n.y = Math.max(0, Math.min(H, n.y));
    energy += n.vx * n.vx + n.vy * n.vy;
  }
  return energy;
}

// ─── Renderer ─────────────────────────────────────────────────────────────────

export function render(
  ctx:             CanvasRenderingContext2D,
  bgCanvas:        HTMLCanvasElement,
  nodes:           SimNode[],
  edges:           SimEdge[],
  particles:       Particle[],
  hover:           string | null,
  selId:           string | null,
  query:           string,
  now:             number,
  W:               number,
  H:               number,
  cam:             Camera,
  pathNodeIds:     Set<string>,
  pathEdgeKeys:    Set<string>,
  pathEndpointIds: Set<string>,
  hbDivisor:       number = 1800,
) {
  ctx.drawImage(bgCanvas, 0, 0, W, H);

  if (nodes.length === 0) return;

  // Apply camera transform — everything below is in world space
  ctx.save();
  const safeCamX = isFinite(cam.x) && !isNaN(cam.x) ? cam.x : 0;
  const safeCamY = isFinite(cam.y) && !isNaN(cam.y) ? cam.y : 0;
  const safeScale = isFinite(cam.scale) && !isNaN(cam.scale) && cam.scale > 0.01 ? cam.scale : 1.0;
  ctx.translate(safeCamX, safeCamY);
  ctx.scale(safeScale, safeScale);

  const hiNodes = new Set<string>();
  const hiEdges = new Set<number>();
  if (hover) {
    hiNodes.add(hover);
    edges.forEach((e, i) => {
      if (e.s === hover || e.t === hover) {
        hiNodes.add(e.s); hiNodes.add(e.t); hiEdges.add(i);
      }
    });
  }
  const qLow    = query.trim().toLowerCase();
  const matched = new Set<string>();
  if (qLow) {
    nodes.forEach(n => {
      if (n.label.toLowerCase().includes(qLow) ||
          n.id.toLowerCase().includes(qLow) ||
          n.content?.toLowerCase().includes(qLow)) matched.add(n.id);
    });
  }

  const hasPath   = pathNodeIds.size > 0;
  const hasFilter = !!hover || !!qLow || hasPath;
  const active    = hasFilter ? new Set<string>() : null;
  if (active) {
    pathNodeIds.forEach(id => active.add(id));
    hiNodes.forEach(id => active.add(id));
    matched.forEach(id => active.add(id));
  }

  // ── Edges ────────────────────────────────────────────────────────────────────
  edges.forEach((e, i) => {
    const a = nodes[e.si], b = nodes[e.ti];
    if (!a || !b) return;

    const onPath  = pathEdgeKeys.has(edgeKey(e.s, e.t));
    const lit     = onPath || (active ? (active.has(e.s) && active.has(e.t)) : true);
    const newEdge = (now - e.born) < 1200;
    const flash   = newEdge ? Math.max(0, 1 - (now - e.born) / 1200) : 0;

    ctx.globalAlpha = onPath ? 0.92 : (lit ? (0.18 + flash * 0.5) : 0.04);
    ctx.strokeStyle = onPath ? 'rgba(34,211,238,0.95)' : (lit ? `rgba(148,163,184,${0.55 + flash})` : 'rgba(148,163,184,0.3)');
    ctx.lineWidth   = 1.5 / cam.scale;

    const mx = (a.x + b.x) / 2 + (b.y - a.y) * 0.1;
    const my = (a.y + b.y) / 2 - (b.x - a.x) * 0.1;
    ctx.beginPath();
    ctx.moveTo(a.x, a.y);
    ctx.quadraticCurveTo(mx, my, b.x, b.y);
    ctx.stroke();
  });

  // ── Signal particles ──────────────────────────────────────────────────────────
  for (const p of particles) {
    const e = edges[p.ei];
    if (!e) continue;
    const a = nodes[e.si], b = nodes[e.ti];
    if (!a || !b) continue;

    const t  = p.t;
    const mx = (a.x + b.x) / 2 + (b.y - a.y) * 0.1;
    const my = (a.y + b.y) / 2 - (b.x - a.x) * 0.1;
    const px = (1-t)*(1-t)*a.x + 2*(1-t)*t*mx + t*t*b.x;
    const py = (1-t)*(1-t)*a.y + 2*(1-t)*t*my + t*t*b.y;

    const srcColor = nodes[e.si]?.color ?? '#94a3b8';
    ctx.globalAlpha = p.alpha * (1 - t * 0.4);
    const g = ctx.createRadialGradient(px, py, 0, px, py, 4);
    g.addColorStop(0, srcColor);
    g.addColorStop(1, 'transparent');
    ctx.fillStyle = g;
    ctx.beginPath();
    ctx.arc(px, py, 3.5, 0, TAU);
    ctx.fill();
  }

  // ── Nodes ─────────────────────────────────────────────────────────────────────
  const hb = 0.5 + 0.5 * Math.sin(now / 2800);

  for (const n of nodes) {
    if (!isFinite(n.x) || !isFinite(n.y) || n.r <= 0 || !isFinite(n.r)) continue;
    const dimmed  = hasFilter && !(active?.has(n.id));
    const isSel   = n.id === selId;
    const isHover = n.id === hover;
    const isPath  = pathNodeIds.has(n.id);
    const isEndpoint = pathEndpointIds.has(n.id);
    const act          = Math.max(-1, Math.min(1, isFinite(n.activation) ? n.activation : 0));
    const displayColor = act < -0.05 ? '#ef4444' : n.color;
    const alpha        = dimmed ? 0.08 : 1.0;

    // Cluster membership ring — outer halo in community color
    const cc = clusterColor(n.cluster_id);
    if (cc && !dimmed) {
      const haloR = n.r * (1.9 + hb * 0.15);
      ctx.globalAlpha = 0.18 + hb * 0.05;
      ctx.strokeStyle = cc;
      ctx.lineWidth   = (n.cluster_id !== undefined ? 1.4 : 0) / cam.scale;
      ctx.beginPath();
      ctx.arc(n.x, n.y, haloR, 0, TAU);
      ctx.stroke();
    }

    // Birth ring
    const age = (now - n.birth) / 1000;
    if (age < 3.0) {
      const t  = age / 3.0;
      const pr = n.r * (1.4 + t * 3);
      ctx.globalAlpha = (1 - t) * 0.45 * alpha;
      ctx.strokeStyle = n.color;
      ctx.lineWidth   = 1.2 / cam.scale;
      ctx.beginPath();
      ctx.arc(n.x, n.y, pr, 0, TAU);
      ctx.stroke();
    }

    // Activation burst (red flash on guild crash: act < -0.05)
    if (act > 0.05 || act < -0.05) {
      const pr = n.r * (1 + Math.abs(act) * 3.5);
      ctx.globalAlpha = Math.abs(act) * 0.5 * alpha;
      ctx.strokeStyle = displayColor;
      ctx.lineWidth   = 1.5 / cam.scale;
      ctx.beginPath();
      ctx.arc(n.x, n.y, pr, 0, TAU);
      ctx.stroke();
    }

    // Outer glow
    if (!dimmed) {
      const glowR   = n.r * (2.2 + hb * 0.4 + act * 1.8 + (isHover ? 1.0 : 0) + (isSel ? 0.6 : 0));
      const glow    = ctx.createRadialGradient(n.x, n.y, 0, n.x, n.y, glowR);
      const opacity = (0.28 + hb * 0.08 + act * 0.35 + (isHover ? 0.2 : 0)).toFixed(2);
      glow.addColorStop(0,   n.color + Math.round(parseFloat(opacity) * 255).toString(16).padStart(2,'0'));
      glow.addColorStop(0.5, n.color + '14');
      glow.addColorStop(1,   'transparent');
      ctx.globalAlpha = alpha * 0.9;
      ctx.fillStyle   = glow;
      ctx.beginPath();
      ctx.arc(n.x, n.y, glowR, 0, TAU);
      ctx.fill();
    }

    // Stigmergy Heat (Amber Glow)
    if (n.heat > 0.1 && !dimmed) {
      const glowR = n.r * (2.0 + Math.min(n.heat * 0.8, 1.5));
      const glow = ctx.createRadialGradient(n.x, n.y, 0, n.x, n.y, glowR);
      const intensity = Math.min(0.85, 0.25 + n.heat * 0.3);
      glow.addColorStop(0, `rgba(251, 191, 36, ${intensity * alpha})`);
      glow.addColorStop(0.4, `rgba(251, 191, 36, ${intensity * 0.35 * alpha})`);
      glow.addColorStop(1, 'transparent');
      ctx.globalAlpha = 1.0;
      ctx.fillStyle = glow;
      ctx.beginPath();
      ctx.arc(n.x, n.y, glowR, 0, TAU);
      ctx.fill();
    }

    // Core
    const breathe = 1 + Math.sin(now / hbDivisor + n.x * 0.01) * (0.025 + Math.abs(act) * 0.04);
    const coreR   = n.r * breathe * (isHover ? 1.12 : 1.0);

    ctx.globalAlpha = alpha;
    ctx.fillStyle   = displayColor;
    ctx.beginPath();
    ctx.arc(n.x, n.y, coreR, 0, TAU);
    ctx.fill();

    // Shine
    if (!dimmed) {
      const shine = ctx.createRadialGradient(
        n.x - coreR * 0.3, n.y - coreR * 0.3, 0,
        n.x, n.y, coreR,
      );
      shine.addColorStop(0, 'rgba(255,255,255,0.22)');
      shine.addColorStop(1, 'transparent');
      ctx.globalAlpha = alpha * 0.7;
      ctx.fillStyle   = shine;
      ctx.beginPath();
      ctx.arc(n.x, n.y, coreR, 0, TAU);
      ctx.fill();
    }

    // Active Agent Presence (Stigmergy)
    if (n.last_agent && !dimmed) {
      const agentColor = getAgentColor(n.last_agent);
      
      // Outer pulsing ring in the agent's color
      const pulseSpeed = 900;
      const pulse = 1.0 + Math.sin(now / pulseSpeed) * 0.12;
      
      ctx.globalAlpha = (0.45 + Math.sin(now / pulseSpeed) * 0.15) * alpha;
      ctx.strokeStyle = agentColor;
      ctx.lineWidth   = 2.0 / cam.scale;
      ctx.beginPath();
      ctx.arc(n.x, n.y, coreR * pulse + 3.0, 0, TAU);
      ctx.stroke();

      // Small anchor dot in the agent's color on the top-right of the node core
      const dotR = 3.5 / cam.scale;
      const dotX = n.x + coreR * Math.cos(-Math.PI / 4);
      const dotY = n.y + coreR * Math.sin(-Math.PI / 4);
      
      ctx.globalAlpha = alpha;
      ctx.fillStyle   = agentColor;
      ctx.beginPath();
      ctx.arc(dotX, dotY, dotR, 0, TAU);
      ctx.fill();

      // Border for the anchor dot
      ctx.strokeStyle = '#020617';
      ctx.lineWidth   = 1.0 / cam.scale;
      ctx.beginPath();
      ctx.arc(dotX, dotY, dotR, 0, TAU);
      ctx.stroke();
    }

    // Selection ring
    if (isPath) {
      const pulse = 1 + Math.sin(now / 650) * 0.06;
      ctx.globalAlpha = isEndpoint ? 0.92 : 0.72;
      ctx.strokeStyle = isEndpoint ? '#f8fafc' : '#22d3ee';
      ctx.lineWidth   = (isEndpoint ? 1.8 : 1.2) / cam.scale;
      ctx.beginPath();
      ctx.arc(n.x, n.y, coreR * pulse + (isEndpoint ? 6 : 4), 0, TAU);
      ctx.stroke();
    }

    if (isSel) {
      const pulse = 1 + Math.sin(now / 500) * 0.08;
      ctx.globalAlpha = 0.85;
      ctx.strokeStyle = '#ffffff';
      ctx.lineWidth   = 1.5 / cam.scale;
      ctx.beginPath();
      ctx.arc(n.x, n.y, coreR * pulse + 3, 0, TAU);
      ctx.stroke();
    }

    // Label
    if (isHover || isSel || isEndpoint || (isPath && pathNodeIds.size <= 6) || (qLow && matched.has(n.id))) {
      let labelText = n.label;
      if (n.last_agent) {
        labelText = `[${n.last_agent}] ${labelText}`;
      }
      const text = labelText.length > 36 ? labelText.slice(0, 35) + '\u2026' : labelText;
      const fontSize = 12;
      ctx.font        = `600 ${fontSize}px "Inter", ui-sans-serif, sans-serif`;
      const tw        = ctx.measureText(text).width;
      const tx        = n.x + coreR + 6;
      const ty        = n.y + 4;
      ctx.globalAlpha = dimmed ? 0.3 : 0.92;
      ctx.fillStyle   = 'rgba(2,6,23,0.82)';
      ctx.beginPath();
      ctx.roundRect(tx - 2, ty - fontSize - 2, tw + 10, fontSize + 6, 3 / cam.scale);
      ctx.fill();
      ctx.fillStyle = isHover || isSel ? '#f1f5f9' : '#94a3b8';
      ctx.fillText(text, tx + 3, ty);
    }
  }

  ctx.restore();
  ctx.globalAlpha = 1;

  // Mini-map drawn in screen space (outside camera transform)
  renderMinimap(ctx, nodes, cam, W, H);
}

// ─── Mini-map ─────────────────────────────────────────────────────────────────

export const MM_W = 128, MM_H = 80, MM_PAD = 8, MM_MARGIN = 12;

function renderMinimap(
  ctx:   CanvasRenderingContext2D,
  nodes: SimNode[],
  cam:   Camera,
  W:     number,
  H:     number,
) {
  if (nodes.length === 0) return;

  // Bounding box of all nodes
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  for (const n of nodes) {
    minX = Math.min(minX, n.x); minY = Math.min(minY, n.y);
    maxX = Math.max(maxX, n.x); maxY = Math.max(maxY, n.y);
  }
  const gw = Math.max(maxX - minX, 1), gh = Math.max(maxY - minY, 1);

  // Scale to fit all nodes in MM bounds with padding
  const scaleX = (MM_W - MM_PAD * 2) / gw;
  const scaleY = (MM_H - MM_PAD * 2) / gh;
  const mmScale = Math.min(scaleX, scaleY);
  const offX = MM_PAD + (MM_W - MM_PAD * 2 - gw * mmScale) / 2 - minX * mmScale;
  const offY = MM_PAD + (MM_H - MM_PAD * 2 - gh * mmScale) / 2 - minY * mmScale;

  const ox = W - MM_MARGIN - MM_W;
  const oy = H - MM_MARGIN - MM_H;

  // Background
  ctx.globalAlpha = 0.82;
  ctx.fillStyle   = 'rgba(1,4,9,0.94)';
  ctx.strokeStyle = 'rgba(148,163,184,0.12)';
  ctx.lineWidth   = 0.5;
  ctx.beginPath();
  ctx.roundRect(ox, oy, MM_W, MM_H, 5);
  ctx.fill(); ctx.stroke();

  // Nodes as dots
  ctx.globalAlpha = 0.7;
  for (const n of nodes) {
    const mx = ox + n.x * mmScale + offX;
    const my = oy + n.y * mmScale + offY;
    const r  = Math.max(1, n.r * mmScale * 0.8);
    ctx.fillStyle = n.activation > 0.08 ? n.color : n.color + '70';
    ctx.beginPath();
    ctx.arc(mx, my, r, 0, TAU);
    ctx.fill();
  }

  // Viewport rectangle — maps screen (0,0)-(W,H) back to world, then to minimap
  const vpX0 = (-cam.x) / cam.scale;
  const vpY0 = (-cam.y) / cam.scale;
  const vpX1 = (W - cam.x) / cam.scale;
  const vpY1 = (H - cam.y) / cam.scale;

  const rx = ox + vpX0 * mmScale + offX;
  const ry = oy + vpY0 * mmScale + offY;
  const rw = (vpX1 - vpX0) * mmScale;
  const rh = (vpY1 - vpY0) * mmScale;

  ctx.globalAlpha = 0.55;
  ctx.strokeStyle = 'rgba(16,185,129,0.9)';
  ctx.lineWidth   = 1;
  ctx.beginPath();
  ctx.rect(rx, ry, rw, rh);
  ctx.stroke();

  ctx.globalAlpha = 1;
}

// ─── Background cache ─────────────────────────────────────────────────────────

export function makeBackground(W: number, H: number, dpr: number): HTMLCanvasElement {
  const c   = document.createElement('canvas');
  c.width   = W * dpr;
  c.height  = H * dpr;
  const ctx = c.getContext('2d')!;
  ctx.scale(dpr, dpr);

  const g = ctx.createRadialGradient(W / 2, H / 2, 0, W / 2, H / 2, Math.max(W, H) * 0.7);
  g.addColorStop(0,   BG_CENTER);
  g.addColorStop(0.6, '#020914');
  g.addColorStop(1,   BG_EDGE_COLOR);
  ctx.fillStyle = g;
  ctx.fillRect(0, 0, W, H);
  return c;
}

// ─── Hit test (accounts for camera) ──────────────────────────────────────────

export function hitTest(nodes: SimNode[], screenX: number, screenY: number, cam: Camera): SimNode | null {
  // Convert screen coords to world coords
  const wx = (screenX - cam.x) / cam.scale;
  const wy = (screenY - cam.y) / cam.scale;
  for (let i = nodes.length - 1; i >= 0; i--) {
    const n  = nodes[i];
    const d2 = (wx - n.x) ** 2 + (wy - n.y) ** 2;
    if (d2 <= (n.r + 6) ** 2) return n;
  }
  return null;
}
