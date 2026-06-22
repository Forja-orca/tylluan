// graphLayout.worker.ts — Barnes-Hut O(N log N) force layout
// Runs off the main thread so 200+ nodes don't freeze the UI.

// ─── Constants (mirrored from HippocampusGraph.tsx) ──────────────────────────

const REPULSION     = 2200;
const SPRING_LEN    = 85;
const SPRING_K      = 0.038;
const GRAVITY       = 0.003;
const DAMPING       = 0.80;
const CLUSTER_GRAV  = 0.0006;
const THETA         = 0.5;   // Barnes-Hut opening angle
const ENERGY_STABLE = 0.12;
const PAD           = 20;
const MAX_ITER      = 500;

// ─── Types ──────────────────────────────────────────────────────────────────

interface NodeData {
  id: string; x: number; y: number; vx: number; vy: number;
  r: number; weight: number; cluster_id?: number;
}
interface EdgeData { si: number; ti: number; }
interface InputMsg {
  gen: number; nodes: NodeData[]; edges: EdgeData[];
  width: number; height: number; iterations: number;
}
interface OutputMsg {
  gen: number;
  positions: Record<string, { x: number; y: number; vx: number; vy: number }>;
  converged: boolean;
}

// ─── QuadTree (Barnes-Hut) ─────────────────────────────────────────────────

interface QPoint { x: number; y: number; idx: number; }

class QuadTree {
  x: number; y: number; w: number; h: number;
  mass = 0; cx = 0; cy = 0; count = 0;
  point: QPoint | null = null;
  children: QuadTree[] | null = null;

  constructor(x: number, y: number, w: number, h: number) {
    this.x = x; this.y = y; this.w = w; this.h = h;
  }

  insert(p: QPoint, depth = 0): void {
    if (depth > 64) return; // guard against infinite recursion on coincident nodes
    if (this.point !== null && this.children === null) {
      if (this.w < 0.01 && this.h < 0.01) { this.count++; return; } // cell too small
      this.subdivide();
      for (const c of this.children!) c.insert(this.point!, depth + 1);
      this.point = null;
    }
    if (this.children !== null) {
      this.getChild(p.x, p.y).insert(p, depth + 1);
    } else {
      this.point = p;
    }
    this.cx = (this.cx * this.count + p.x) / (this.count + 1);
    this.cy = (this.cy * this.count + p.y) / (this.count + 1);
    this.count++;
  }

  private subdivide(): void {
    const hw = this.w / 2;
    const hh = this.h / 2;
    this.children = [
      new QuadTree(this.x - hw, this.y - hh, hw, hh),
      new QuadTree(this.x + hw, this.y - hh, hw, hh),
      new QuadTree(this.x - hw, this.y + hh, hw, hh),
      new QuadTree(this.x + hw, this.y + hh, hw, hh),
    ];
  }

  private getChild(px: number, py: number): QuadTree {
    const left = px <= this.x;
    const top  = py <= this.y;
    return this.children![(left ? 0 : 1) + (top ? 0 : 2)];
  }

  force(px: number, py: number, theta: number, skipIdx: number): { fx: number; fy: number } {
    if (this.count === 0) return { fx: 0, fy: 0 };

    const dx = this.cx - px;
    const dy = this.cy - py;
    const d  = Math.sqrt(dx * dx + dy * dy) || 1;
    const s  = Math.max(this.w, this.h);

    // Leaf with single node — direct force (skip self)
    if (this.count === 1 && this.point !== null) {
      if (this.point.idx === skipIdx) return { fx: 0, fy: 0 };
      const d2 = Math.max(d * d, 1);
      const f  = Math.min(REPULSION / d2, 80);
      return { fx: (dx / d) * f, fy: (dy / d) * f };
    }

    // Far enough — approximate as center-of-mass point
    if (s / d < theta) {
      const d2 = Math.max(d * d, 1);
      const f  = Math.min(REPULSION / d2, 80) * this.count;
      return { fx: (dx / d) * f, fy: (dy / d) * f };
    }

    // Too close — recurse into children
    if (this.children) {
      let fx = 0, fy = 0;
      for (const c of this.children) {
        const f = c.force(px, py, theta, skipIdx);
        fx += f.fx; fy += f.fy;
      }
      return { fx, fy };
    }

    return { fx: 0, fy: 0 };
  }
}

// ─── Single simulation step ────────────────────────────────────────────────

function simulate(nodes: NodeData[], edges: EdgeData[], W: number, H: number): boolean {
  const cx = W / 2, cy = H / 2;

  // Build quadtree
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  for (const n of nodes) {
    if (n.x < minX) minX = n.x;
    if (n.y < minY) minY = n.y;
    if (n.x > maxX) maxX = n.x;
    if (n.y > maxY) maxY = n.y;
  }
  const span   = Math.max(Math.max(maxX - minX, 1) + PAD * 2, Math.max(maxY - minY, 1) + PAD * 2) / 2;
  const midX   = (minX + maxX) / 2;
  const midY   = (minY + maxY) / 2;
  const qt     = new QuadTree(midX, midY, span, span);
  for (let i = 0; i < nodes.length; i++) qt.insert({ x: nodes[i].x, y: nodes[i].y, idx: i });

  // Barnes-Hut repulsion
  for (let i = 0; i < nodes.length; i++) {
    const f = qt.force(nodes[i].x, nodes[i].y, THETA, i);
    nodes[i].vx -= f.fx;
    nodes[i].vy -= f.fy;
  }

  // Spring force
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

  // Cluster gravity
  const ccx = new Map<number, { sx: number; sy: number; n: number }>();
  for (const n of nodes) {
    if (n.cluster_id === undefined) continue;
    const acc = ccx.get(n.cluster_id) ?? { sx: 0, sy: 0, n: 0 };
    acc.sx += n.x; acc.sy += n.y; acc.n++;
    ccx.set(n.cluster_id, acc);
  }
  for (const n of nodes) {
    if (n.cluster_id === undefined) continue;
    const acc = ccx.get(n.cluster_id);
    if (!acc || acc.n < 2) continue;
    n.vx += (acc.sx / acc.n - n.x) * CLUSTER_GRAV;
    n.vy += (acc.sy / acc.n - n.y) * CLUSTER_GRAV;
  }

  // Gravity + damping + integration
  let energy = 0;
  for (const n of nodes) {
    n.vx += (cx - n.x) * GRAVITY;
    n.vy += (cy - n.y) * GRAVITY;
    n.vx *= DAMPING; n.vy *= DAMPING;
    n.x  += n.vx;  n.y  += n.vy;
    if (n.x < 60) n.vx += 0.15 * (60 - n.x);
    if (n.x > W - 60) n.vx -= 0.15 * (n.x - (W - 60));
    if (n.y < 60) n.vy += 0.15 * (60 - n.y);
    if (n.y > H - 60) n.vy -= 0.15 * (n.y - (H - 60));
    n.x = Math.max(0, Math.min(W, n.x));
    n.y = Math.max(0, Math.min(H, n.y));
    energy += n.vx * n.vx + n.vy * n.vy;
  }
  return energy < ENERGY_STABLE;
}

// ─── Message handler ───────────────────────────────────────────────────────

self.onmessage = (e: MessageEvent<InputMsg>) => {
  const { gen, nodes, edges, width, height, iterations } = e.data;
  const sim: NodeData[] = nodes.map(n => ({ ...n }));
  let converged = false;
  const limit = Math.min(iterations, MAX_ITER);
  for (let i = 0; i < limit; i++) {
    converged = simulate(sim, edges, width, height);
    if (converged) break;
  }
  const positions: Record<string, { x: number; y: number; vx: number; vy: number }> = {};
  for (const n of sim) positions[n.id] = { x: n.x, y: n.y, vx: n.vx, vy: n.vy };
  const out: OutputMsg = { gen, positions, converged };
  self.postMessage(out);
};
