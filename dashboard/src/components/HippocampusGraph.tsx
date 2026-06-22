/**
 * HippocampusGraph — Living memory visualization
 *
 * Canvas 2D · Custom force simulation · Zero graph library dependencies
 * React component wrapper around simulation engine in graph/simulation.ts.
 *
 * - New nodes appear live via SSE events (memory_added) — no manual reload
 * - Nodes fire and propagate signals when the kernel uses them
 * - The graph breathes, drifts, and pulses like living neural tissue
 * - Incremental refresh every 20s catches any nodes missed by events
 * - Zoom: scroll wheel (zoom-to-cursor) · Pan: click+drag on empty space
 */

import React, {
  useEffect, useRef, useState, useCallback, useLayoutEffect,
} from 'react';
import { RefreshCw, Search, X, ZoomIn, ZoomOut, Maximize2, Route, Activity, RotateCcw } from 'lucide-react';
import type { GraphNode } from '../lib/nexus-bridge';
import {
  SimNode, SimEdge, Particle, Camera, Detail, PathTraceState, Props,
  PALETTE, DEFAULT_COLOR, CLUSTER_COLORS, CLUSTER_GRAVITY, DRIFT_STRENGTH,
  ZOOM_MIN, ZOOM_MAX, LAYOUT_STORAGE_KEY, LAYOUT_SAVE_DEBOUNCE_MS,
  ENERGY_STABLE, BG_EDGE_COLOR,
  clusterColor, graphFingerprint, loadStoredLayout, saveLayout,
  nodeColor, scheduleNextFire, edgeKey, shortestVisiblePath,
  buildSim, tick, render, makeBackground, hitTest, getAgentColor,
  MM_W, MM_H, MM_PAD, MM_MARGIN, EMPTY_PATH_TRACE,
} from './graph/simulation';

// ─── Component ────────────────────────────────────────────────────────────────
export function HippocampusGraph({ bridge, events, onNodeClick }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const wrapRef   = useRef<HTMLDivElement>(null);
  const bgRef     = useRef<HTMLCanvasElement | null>(null);

  const nodes     = useRef<SimNode[]>([]);
  const edges     = useRef<SimEdge[]>([]);
  const particles = useRef<Particle[]>([]);
  const stableRef = useRef(false);
  const rafRef    = useRef(0);
  const hoverRef  = useRef<string | null>(null);
  const queryRef  = useRef('');
  const knownIds  = useRef<Set<string>>(new Set());
  const pathTraceRef = useRef<PathTraceState>(EMPTY_PATH_TRACE);
  const pathNodeIdsRef = useRef<Set<string>>(new Set());
  const pathEdgeKeysRef = useRef<Set<string>>(new Set());
  const pathEndpointIdsRef = useRef<Set<string>>(new Set());

  // Worker-driven Barnes-Hut layout
  const workerRef = useRef<Worker | null>(null);
  const layoutGenRef = useRef(0);
  const layoutBusyRef = useRef(false);
  const layoutReadyRef = useRef(false);

  // Camera
  const camRef    = useRef<Camera>({ x: 0, y: 0, scale: 1 });
  const [camScale, setCamScale] = useState(1); // for UI display only

  // Drag state
  const dragRef = useRef<{ active: boolean; moved: boolean; startX: number; startY: number; camX: number; camY: number }>({
    active: false, moved: false, startX: 0, startY: 0, camX: 0, camY: 0,
  });

  // Performance counters — mutated each frame, no re-render
  const perfRef = useRef({ tickMs: 0, renderMs: 0, fps: 0, lastFpsTs: 0, frameCount: 0 });
  const hormoneRef = useRef({ stress: 0, novelty: 0, energy: 1.0, saturation: 0 });
  const layoutSaveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const [loading,   setLoading]   = useState(true);
  const [error,     setError]     = useState<string | null>(null);
  const [reloadKey, setReloadKey] = useState(0);
  const [selected,  setSelected]  = useState<Detail | null>(null);
  const [query,     setQuery]     = useState('');
  const [stats,     setStats]     = useState({ nodes: 0, edges: 0 });
  const [size,      setSize]      = useState({ w: 0, h: 0 });
  const [pathMode,  setPathMode]  = useState(false);
  const [pathTrace, setPathTrace] = useState<PathTraceState>(EMPTY_PATH_TRACE);
  const [showPerf,  setShowPerf]  = useState(false);
  const [perfSnap,  setPerfSnap]  = useState({ tickMs: 0, renderMs: 0, fps: 0, pairs: 0, stable: false });

  const sizeRef = useRef({ w: 0, h: 0 });
  useEffect(() => { sizeRef.current = size; }, [size]);

  const selectedIdRef = useRef<string | null>(null);
  useEffect(() => { selectedIdRef.current = selected?.id ?? null; }, [selected]);

  const showPerfRef = useRef(false);
  useEffect(() => { showPerfRef.current = showPerf; }, [showPerf]);

  // queryRef stays in sync via onChange — no useEffect needed

  const commitPathTrace = useCallback((next: PathTraceState) => {
    pathTraceRef.current = next;
    pathNodeIdsRef.current = new Set(next.nodes);
    pathEdgeKeysRef.current = new Set(next.edges);
    pathEndpointIdsRef.current = new Set(next.picks.slice(0, 2));
    setPathTrace(next);
  }, []);

  const clearPathTrace = useCallback(() => {
    commitPathTrace(EMPTY_PATH_TRACE);
  }, [commitPathTrace]);

  const requestLayout = useCallback(() => {
    const wkr = workerRef.current;
    if (!wkr) return;
    const ns = nodes.current;
    const es = edges.current;
    if (ns.length === 0) return;
    const w = sizeRef.current.w || 800;
    const h = sizeRef.current.h || 500;
    layoutGenRef.current++;
    layoutBusyRef.current = true;
    layoutReadyRef.current = false;
    wkr.postMessage({
      gen: layoutGenRef.current,
      nodes: ns.map(n => ({
        id: n.id, x: n.x, y: n.y, vx: n.vx, vy: n.vy,
        r: n.r, weight: n.weight, cluster_id: n.cluster_id,
      })),
      edges: es.map(e => ({ si: e.si, ti: e.ti })),
      width: w, height: h,
      iterations: 300,
    });
  }, []);

  // ── Resize ───────────────────────────────────────────────────────────────────
  useLayoutEffect(() => {
    const wrap = wrapRef.current;
    if (!wrap) return;
    const initialRect = wrap.getBoundingClientRect();
    if (initialRect.width > 10 && initialRect.height > 10) {
      setSize({ w: Math.floor(initialRect.width), h: Math.floor(initialRect.height) });
    }
    const ro = new ResizeObserver(entries => {
      const { width, height } = entries[0].contentRect;
      if (width > 10 && height > 10) setSize({ w: Math.floor(width), h: Math.floor(height) });
    });
    ro.observe(wrap);
    return () => ro.disconnect();
  }, []);

  // ── Worker lifecycle ─────────────────────────────────────────────────────────
  useEffect(() => {
    const worker = new Worker(
      new URL('../workers/graphLayout.worker.ts', import.meta.url),
    );
    worker.onmessage = (e: MessageEvent) => {
      const data = e.data;
      if (data.gen !== layoutGenRef.current) return;
      const positions = data.positions as Record<string, { x: number; y: number; vx: number; vy: number }>;
      for (const n of nodes.current) {
        const p = positions[n.id];
        if (p) { n.x = p.x; n.y = p.y; n.vx = p.vx; n.vy = p.vy; }
      }
      layoutReadyRef.current = true;
      layoutBusyRef.current = false;
      stableRef.current = true;
    };
    workerRef.current = worker;
    // If graph already loaded, kick off layout
    if (nodes.current.length > 0 && !layoutReadyRef.current) {
      requestLayout();
    }
    return () => { worker.terminate(); workerRef.current = null; };
  }, [requestLayout]);

  // ── Zoom helpers ─────────────────────────────────────────────────────────────
  const applyZoom = useCallback((factor: number, pivotX?: number, pivotY?: number) => {
    const cam = camRef.current;
    const W   = size.w || 800;
    const H   = size.h || 500;
    const px  = pivotX ?? W / 2;
    const py  = pivotY ?? H / 2;

    const newScale = Math.max(ZOOM_MIN, Math.min(ZOOM_MAX, cam.scale * factor));
    // Adjust pan so pivot point stays fixed on screen
    cam.x = px - (px - cam.x) * (newScale / cam.scale);
    cam.y = py - (py - cam.y) * (newScale / cam.scale);
    cam.scale = newScale;
    setCamScale(newScale);
  }, [size]);

  const resetCamera = useCallback(() => {
    camRef.current = { x: 0, y: 0, scale: 1 };
    setCamScale(1);
  }, []);

  // Fit all nodes into view with padding
  const fitToGraph = useCallback(() => {
    const ns = nodes.current;
    if (ns.length === 0 || size.w === 0) { resetCamera(); return; }
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    for (const n of ns) {
      if (!isFinite(n.x) || !isFinite(n.y) || isNaN(n.x) || isNaN(n.y)) continue;
      minX = Math.min(minX, n.x - n.r);
      minY = Math.min(minY, n.y - n.r);
      maxX = Math.max(maxX, n.x + n.r);
      maxY = Math.max(maxY, n.y + n.r);
    }
    if (minX === Infinity || minY === Infinity || maxX === -Infinity || maxY === -Infinity) {
      resetCamera();
      return;
    }
    const PAD    = 40;
    const gw     = maxX - minX + PAD * 2;
    const gh     = maxY - minY + PAD * 2;
    const scale  = Math.max(ZOOM_MIN, Math.min(ZOOM_MAX, Math.min(size.w / gw, size.h / gh)));
    const cx     = size.w / 2 - (minX + (maxX - minX) / 2) * scale;
    const cy     = size.h / 2 - (minY + (maxY - minY) / 2) * scale;
    camRef.current = {
      x: isFinite(cx) && !isNaN(cx) ? cx : size.w / 2,
      y: isFinite(cy) && !isNaN(cy) ? cy : size.h / 2,
      scale: isFinite(scale) && !isNaN(scale) && scale > 0.01 ? scale : 1.0
    };
    setCamScale(camRef.current.scale);
  }, [size, resetCamera]);



  // ── Wheel zoom ───────────────────────────────────────────────────────────────
  const onWheel = useCallback((e: WheelEvent) => {
    e.preventDefault();
    const canvas = canvasRef.current;
    if (!canvas) return;
    const rect   = canvas.getBoundingClientRect();
    const factor = e.deltaY < 0 ? 1.12 : 1 / 1.12;
    applyZoom(factor, e.clientX - rect.left, e.clientY - rect.top);
  }, [applyZoom]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    canvas.addEventListener('wheel', onWheel, { passive: false });
    return () => canvas.removeEventListener('wheel', onWheel);
  }, [onWheel]);

  // ── Signals ──────────────────────────────────────────────────────────────────
  const spawnSignals = useCallback((nodeId: string) => {
    const ni = nodes.current.findIndex(n => n.id === nodeId);
    if (ni < 0) return;
    edges.current.forEach((e, i) => {
      if (e.si !== ni && e.ti !== ni) return;
      const forward = e.si === ni;
      particles.current.push({
        ei:    i,
        t:     forward ? 0 : 1,
        speed: (0.004 + Math.random() * 0.005) * (forward ? 1 : -1),
        alpha: 0.7 + Math.random() * 0.3,
      });
      if (particles.current.length > 80) particles.current.shift();
    });
  }, []);

  const fireNode = useCallback((nodeId: string, strength = 1.0) => {
    const n = nodes.current.find(n => n.id === nodeId);
    if (!n) return;
    n.activation = Math.min(1, n.activation + strength);
    n.nextFire   = scheduleNextFire(n.weight, Date.now(), 1 - hormoneRef.current.novelty * 0.6);
    spawnSignals(nodeId);
  }, [spawnSignals]);

  // ── SSE reactions ────────────────────────────────────────────────────────────
  const prevEventsLen = useRef(0);
  useEffect(() => {
    if (!events || events.length === prevEventsLen.current) return;
    const newEvs = events.slice(prevEventsLen.current);
    prevEventsLen.current = events.length;

    for (const ev of newEvs) {
      switch (ev.type) {
        case 'memory_added':
        case 'memory_updated': {
          const id = (ev.data as { id?: string }).id;
          if (id) fireNode(id, 1.0);
          break;
        }
        case 'tool_call': {
          const d = ev.data as { tool?: string; status?: string; intent?: string };
          if (d.status !== 'finished') break;
          if (d.tool === 'tylluan_recall') {
            // Light up recently active nodes — recall is reading them
            nodes.current
              .filter(n => n.activation > 0.1)
              .slice(0, 4)
              .forEach(n => fireNode(n.id, 0.5));
          } else if (d.tool === 'tylluan_remember') {
            // A new memory was just written — pulse strongly (node may appear on next poll)
            nodes.current
              .sort((a, b) => b.birth - a.birth)
              .slice(0, 1)
              .forEach(n => fireNode(n.id, 1.0));
          } else if (d.tool === 'tylluan_think') {
            // Kernel reasoning — cascade soft activation across random connected cluster
            const pool = nodes.current.filter(n => n.kind === 'concept' || n.kind === 'experience');
            pool.slice(0, Math.min(6, Math.floor(pool.length * 0.15) + 2))
                .sort(() => Math.random() - 0.5)
                .forEach(n => fireNode(n.id, 0.35));
          } else if (d.tool === 'tylluan_do') {
            // Task dispatch — fire tool_call type nodes
            nodes.current
              .filter(n => n.kind === 'tool_call' || n.kind === 'agent')
              .slice(0, 3)
              .forEach(n => fireNode(n.id, 0.6));
          }
          break;
        }
        case 'guild_spawned': {
          // A new guild woke up — pulse agent nodes
          nodes.current
            .filter(n => n.kind === 'agent')
            .slice(0, 2)
            .forEach(n => fireNode(n.id, 0.7));
          break;
        }
        case 'graph_autolinked': {
          // Kernel auto-linked memory nodes — cascade across all edges briefly
          const count = (ev.data as { count?: number }).count ?? 3;
          nodes.current
            .sort(() => Math.random() - 0.5)
            .slice(0, Math.min(count * 2, 12))
            .forEach(n => fireNode(n.id, 0.4));
          break;
        }
        case 'hormone_signal': {
          const d = ev.data as { stress?: number; novelty?: number; energy?: number; saturation?: number };
          if (d.stress    !== undefined) hormoneRef.current.stress    = d.stress;
          if (d.novelty   !== undefined) hormoneRef.current.novelty   = d.novelty;
          if (d.energy    !== undefined) hormoneRef.current.energy    = d.energy;
          if (d.saturation !== undefined) hormoneRef.current.saturation = d.saturation;
          // High stress: pulse agent/tool nodes
          if ((d.stress ?? 0) > 0.6) {
            nodes.current
              .filter(n => n.kind === 'agent' || n.kind === 'tool_call')
              .slice(0, 4)
              .forEach(n => fireNode(n.id, (d.stress ?? 0.6) * 0.8));
          }
          // High novelty: briefly wake the most recently born nodes
          if ((d.novelty ?? 0) > 0.5) {
            nodes.current
              .sort((a, b) => b.birth - a.birth)
              .slice(0, 3)
              .forEach(n => fireNode(n.id, 0.7));
          }
          break;
        }
        case 'guild_health_updated': {
          // A guild crashed or recovered — flash its agent node red (activation < 0)
          const d = ev.data as { guild?: string; status?: string };
          if (d.guild && d.status === 'crashed') {
            const agentNode = nodes.current.find(n =>
              n.id.includes(d.guild!) || n.label.toLowerCase().includes(d.guild!.toLowerCase())
            );
            if (agentNode) {
              agentNode.activation = -0.8; // negative → displayColor fires red
            }
          } else if (d.guild && d.status === 'running') {
            const agentNode = nodes.current.find(n =>
              n.id.includes(d.guild!) || n.label.toLowerCase().includes(d.guild!.toLowerCase())
            );
            if (agentNode) {
              fireNode(agentNode.id, 0.9);
            }
          }
          break;
        }
        case 'memory_decay': {
          // A node lost weight — shrink it slightly and dim
          const d = ev.data as { id?: string; weight?: number };
          if (d.id) {
            const n = nodes.current.find(n => n.id === d.id);
            if (n) {
              n.weight = Math.max(0.05, d.weight ?? n.weight * 0.85);
              n.r      = 4 + n.weight * 14;
              n.activation = Math.max(-0.3, n.activation - 0.3); // brief dim flash
            }
          }
          break;
        }
      }
    }
  }, [events, fireNode]);

  // ── Animation loop ───────────────────────────────────────────────────────────
  const startLoop = useCallback(() => {
    cancelAnimationFrame(rafRef.current);

    const loop = (now: number) => {
      const canvas = canvasRef.current;
      const bg     = bgRef.current;
      if (!canvas || !bg) { rafRef.current = requestAnimationFrame(loop); return; }
      const ctx = canvas.getContext('2d');
      if (!ctx) return;

      const W = sizeRef.current.w, H = sizeRef.current.h;
      if (W < 10 || H < 10) { rafRef.current = requestAnimationFrame(loop); return; }

      const t0 = performance.now();
      const hormones     = hormoneRef.current;
      const noveltyFactor = 1 - hormones.novelty * 0.6;
      const driftBoost    = 1.0 + hormones.stress * 0.8;
      const hbSpeed       = hormones.stress > 0.5 ? 900 : 1800;
      const dynamicThreshold = ENERGY_STABLE * (1 + nodes.current.length / 200);
      const isWaiting = layoutBusyRef.current && !layoutReadyRef.current;
      const isStable  = stableRef.current || isWaiting;
      const energy = tick(nodes.current, edges.current, W, H, isStable, noveltyFactor, driftBoost);
      const justStabilized = !stableRef.current && energy < dynamicThreshold && nodes.current.length > 0 && !isWaiting;
      if (justStabilized) {
        stableRef.current = true;
        // Debounced layout save on convergence
        if (layoutSaveTimer.current) clearTimeout(layoutSaveTimer.current);
        layoutSaveTimer.current = setTimeout(() => saveLayout(nodes.current), LAYOUT_SAVE_DEBOUNCE_MS);
      }
      const tickMs = performance.now() - t0;

      particles.current = particles.current.filter(p => {
        p.t += p.speed;
        return p.t >= 0 && p.t <= 1;
      });

      const r0 = performance.now();
      render(ctx, bg, nodes.current, edges.current, particles.current,
             hoverRef.current, selectedIdRef.current, queryRef.current, Date.now(), W, H, camRef.current,
             pathNodeIdsRef.current, pathEdgeKeysRef.current, pathEndpointIdsRef.current, hbSpeed);
      const renderMs = performance.now() - r0;

      // FPS + perf snapshot (update every 30 frames)
      const perf = perfRef.current;
      perf.tickMs   = tickMs;
      perf.renderMs = renderMs;
      perf.frameCount++;
      if (perf.frameCount % 120 === 0) {
        const elapsed = now - perf.lastFpsTs;
        perf.fps = elapsed > 0 ? Math.round(120_000 / elapsed) : 0;
        perf.lastFpsTs = now;
        if (showPerfRef.current) {
          const n = nodes.current.length;
          setPerfSnap({
            tickMs:   Math.round(perf.tickMs * 10) / 10,
            renderMs: Math.round(perf.renderMs * 10) / 10,
            fps:      perf.fps,
            pairs:    Math.round(n * (n - 1) / 2),
            stable:   stableRef.current,
          });
        }
      }

      rafRef.current = requestAnimationFrame(loop);
    };

    rafRef.current = requestAnimationFrame(loop);
  }, []);

  useEffect(() => {
    if (!loading && nodes.current.length > 0) startLoop();
    return () => cancelAnimationFrame(rafRef.current);
  }, [startLoop, loading]);

  // ── Load graph ───────────────────────────────────────────────────────────────
  const loadGraph = useCallback(async (incremental = false) => {
    if (!incremental) setLoading(true);
    setError(null);

    try {
      const data = await bridge.getSilvaGraph(400, true); // cluster=true → Louvain community ids
      const raw  = data as unknown as { nodes?: GraphNode[]; edges?: unknown[]; links?: unknown[] };
      const rn   = raw.nodes ?? [];
      const re   = (raw.edges ?? raw.links ?? []) as
                   { source?: string; from?: string; target?: string; to?: string }[];

      let restoreIdx: Map<string, SimNode> | undefined;
      if (incremental) {
        const newRaw = rn.filter(n => !knownIds.current.has(n.id));
        if (newRaw.length === 0) return;

        const { nodes: newNodes, edges: newEdges } = buildSim(
          rn, re, size.w || 800, size.h || 500, new Map(nodes.current.map(n => [n.id, n])),
        );
        nodes.current = newNodes;
        edges.current = newEdges;
        newRaw.forEach(n => { knownIds.current.add(n.id); fireNode(n.id, 1.0); });
        stableRef.current = false;
      } else {
        // Partial-restore positions from localStorage: match known nodes, let new ones simulate freely
        const stored = loadStoredLayout();
        let existingIdx: Map<string, SimNode> | undefined;
        if (stored && stored.positions.length > 0) {
          const posMap = new Map(stored.positions.map(p => [p.id, p]));
          const matchCount = rn.filter(r => posMap.has(r.id)).length;
          // Only restore if at least half the nodes have saved positions (avoids ghost layouts)
          if (matchCount >= Math.min(stored.positions.length, rn.length) * 0.5) {
            existingIdx = new Map(stored.positions.map(p => [
              p.id,
              { id: p.id, x: p.x, y: p.y, vx: 0, vy: 0, r: 0, color: '', label: '',
                kind: '', weight: 0, birth: 0, activation: 0, nextFire: 0 } as SimNode,
            ]));
          }
        }
        const { nodes: n, edges: e } = buildSim(rn, re, size.w || 800, size.h || 500, existingIdx);
        nodes.current  = n;
        edges.current  = e;
        knownIds.current   = new Set(n.map(x => x.id));
        particles.current  = [];
        commitPathTrace(EMPTY_PATH_TRACE);
        // If restoring layout, start in stable mode to skip re-simulation
        if (existingIdx) { restoreIdx = existingIdx; stableRef.current = true; }
      }

      // Kick off worker-based layout unless restoring from cache
      if (!restoreIdx) requestLayout();
      setStats({ nodes: nodes.current.length, edges: edges.current.length });
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      if (!incremental) setLoading(false);
    }
  }, [bridge, size, fireNode, commitPathTrace]);

  useEffect(() => {
    loadGraph(false);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [reloadKey]);

  useEffect(() => {
    const id = setInterval(() => loadGraph(true), 20_000);
    return () => clearInterval(id);
  }, [loadGraph]);

  useEffect(() => {
    const onRefresh = () => loadGraph(false);
    window.addEventListener('silva_graph_refresh', onRefresh);
    return () => window.removeEventListener('silva_graph_refresh', onRefresh);
  }, [loadGraph]);

  // ── Canvas HiDPI setup ───────────────────────────────────────────────────────
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || size.w === 0) return;
    const dpr = Math.min(window.devicePixelRatio || 1, 1.5);
    canvas.width  = size.w * dpr;
    canvas.height = size.h * dpr;
    const ctx = canvas.getContext('2d');
    if (ctx) ctx.scale(dpr, dpr);
    bgRef.current = makeBackground(size.w, size.h, dpr);
    if (nodes.current.length > 0) {
      fitToGraph();
    }
  }, [size, fitToGraph]);

  const resetStoredLayout = useCallback(() => {
    localStorage.removeItem(LAYOUT_STORAGE_KEY);
    localStorage.removeItem('tylluan_hippocampus_v1');
    stableRef.current = false;
    resetCamera();
    loadGraph(false);
  }, [loadGraph, resetCamera]);

  // ── Mouse interaction ────────────────────────────────────────────────────────
  const getPos = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const r = canvasRef.current!.getBoundingClientRect();
    return { mx: e.clientX - r.left, my: e.clientY - r.top };
  };

  const onMouseMove = useCallback((e: React.MouseEvent<HTMLCanvasElement>) => {
    const { mx, my } = getPos(e);

    // Pan if dragging
    if (dragRef.current.active) {
      const dx = mx - dragRef.current.startX;
      const dy = my - dragRef.current.startY;
      if (Math.abs(dx) > 3 || Math.abs(dy) > 3) dragRef.current.moved = true;
      camRef.current.x = dragRef.current.camX + dx;
      camRef.current.y = dragRef.current.camY + dy;
      return;
    }

    const hit = hitTest(nodes.current, mx, my, camRef.current);
    const id  = hit?.id ?? null;
    if (id !== hoverRef.current) {
      hoverRef.current = id;
      canvasRef.current!.style.cursor = id ? 'pointer' : dragRef.current.active ? 'grabbing' : 'grab';
    }
  }, []);

  const onMouseDown = useCallback((e: React.MouseEvent<HTMLCanvasElement>) => {
    if (e.button !== 0) return;
    const { mx, my } = getPos(e);
    const hit = hitTest(nodes.current, mx, my, camRef.current);
    if (hit) return; // let onClick handle node hits

    dragRef.current = {
      active: true,
      moved: false,
      startX: mx, startY: my,
      camX: camRef.current.x, camY: camRef.current.y,
    };
    canvasRef.current!.style.cursor = 'grabbing';
  }, []);

  const onMouseUp = useCallback(() => {
    dragRef.current.active = false;
    canvasRef.current!.style.cursor = 'grab';
  }, []);

  const onMouseLeave = useCallback(() => {
    dragRef.current.active = false;
    dragRef.current.moved = false;
    hoverRef.current = null;
    if (canvasRef.current) canvasRef.current.style.cursor = 'grab';
  }, []);

  // Check if a screen point is inside the mini-map region
  const inMinimap = useCallback((sx: number, sy: number): boolean => {
    const ox = size.w - MM_MARGIN - MM_W;
    const oy = size.h - MM_MARGIN - MM_H;
    return sx >= ox && sx <= ox + MM_W && sy >= oy && sy <= oy + MM_H;
  }, [size]);

  const onClick = useCallback((e: React.MouseEvent<HTMLCanvasElement>) => {
    if (dragRef.current.active) return;
    if (dragRef.current.moved) {
      dragRef.current.moved = false;
      return;
    }
    const { mx, my } = getPos(e);

    // Mini-map click — pan camera to clicked world position
    if (inMinimap(mx, my) && nodes.current.length > 0) {
      const ns = nodes.current;
      let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
      for (const n of ns) {
        minX = Math.min(minX, n.x); minY = Math.min(minY, n.y);
        maxX = Math.max(maxX, n.x); maxY = Math.max(maxY, n.y);
      }
      const gw = Math.max(maxX - minX, 1), gh = Math.max(maxY - minY, 1);
      const scaleX = (MM_W - MM_PAD * 2) / gw;
      const scaleY = (MM_H - MM_PAD * 2) / gh;
      const mmScale = Math.min(scaleX, scaleY);
      const offX = MM_PAD + (MM_W - MM_PAD * 2 - gw * mmScale) / 2 - minX * mmScale;
      const offY = MM_PAD + (MM_H - MM_PAD * 2 - gh * mmScale) / 2 - minY * mmScale;
      const ox = size.w - MM_MARGIN - MM_W;
      const oy = size.h - MM_MARGIN - MM_H;
      // Convert click in mini-map to world coords, then center camera there
      const wx = (mx - ox - offX) / mmScale;
      const wy = (my - oy - offY) / mmScale;
      camRef.current.x = size.w / 2 - wx * camRef.current.scale;
      camRef.current.y = size.h / 2 - wy * camRef.current.scale;
      return;
    }
    const hit = hitTest(nodes.current, mx, my, camRef.current);
    if (hit) {
      const detail = { id: hit.id, label: hit.label, kind: hit.kind,
                       color: hit.color, weight: hit.weight,
                       content: hit.content, created: hit.created,
                       last_agent: hit.last_agent };
      setSelected(detail);

      if (pathMode) {
        const current = pathTraceRef.current.picks;
        const picks = current.length >= 2
          ? [hit.id]
          : (current[0] === hit.id ? [hit.id] : [...current, hit.id]);

        if (picks.length === 2) {
          const path = shortestVisiblePath(picks[0], picks[1], edges.current);
          if (path) {
            commitPathTrace({ picks, nodes: path.nodes, edges: path.edges, found: true, hops: path.hops });
            path.nodes.slice(0, 16).forEach(id => fireNode(id, id === hit.id ? 0.55 : 0.18));
          } else {
            commitPathTrace({ picks, nodes: picks, edges: [], found: false, hops: 0 });
          }
        } else {
          commitPathTrace({ picks, nodes: picks, edges: [], found: null, hops: 0 });
        }

        fireNode(hit.id, 0.6);
        onNodeClick?.({ id: hit.id, label: hit.label, node_type: hit.kind } as GraphNode);
        return;
      }

      fireNode(hit.id, 0.6);
      onNodeClick?.({ id: hit.id, label: hit.label, node_type: hit.kind } as GraphNode);
    } else {
      setSelected(null);
    }
  }, [commitPathTrace, fireNode, onNodeClick, pathMode]);

  // ─── Render ──────────────────────────────────────────────────────────────────
  return (
    <div className="flex flex-col flex-1 min-h-0 rounded-xl overflow-hidden border border-slate-800/40"
         style={{ background: BG_EDGE_COLOR }}>

      {/* Header */}
      <div className="flex items-center gap-3 px-4 py-2 border-b border-white/5 bg-white/[0.02] flex-shrink-0">
        <div className="flex items-center gap-2">
          <div className="relative flex items-center justify-center w-4 h-4">
            <div className="w-1.5 h-1.5 rounded-full bg-emerald-500 animate-pulse" />
            <div className="absolute w-3 h-3 rounded-full bg-emerald-500/20 animate-ping" />
          </div>
          <span className="text-[10px] text-slate-500 uppercase font-black tracking-widest">
            Hipocampo
          </span>
        </div>

        <div className="flex items-center gap-3 text-xs font-mono ml-1">
          <span className="text-emerald-400/80 font-semibold">
            {stats.nodes}
            <span className="text-slate-600 font-normal ml-1 text-[9px]">nodes</span>
          </span>
          <span className="text-blue-400/80 font-semibold">
            {stats.edges}
            <span className="text-slate-600 font-normal ml-1 text-[9px]">synapses</span>
          </span>
        </div>

        <div className="flex-1" />

        {/* Zoom controls */}
        <div className="flex items-center gap-1 border border-white/[0.06] rounded-lg px-1 py-0.5">
          <button type="button" aria-label="Zoom out"
            onClick={() => applyZoom(1 / 1.25)}
            className="p-1 text-slate-600 hover:text-slate-400 transition-colors">
            <ZoomOut className="w-3 h-3" />
          </button>
          <span className="text-[9px] font-mono text-slate-600 w-8 text-center select-none">
            {Math.round(camScale * 100)}%
          </span>
          <button type="button" aria-label="Zoom in"
            onClick={() => applyZoom(1.25)}
            className="p-1 text-slate-600 hover:text-slate-400 transition-colors">
            <ZoomIn className="w-3 h-3" />
          </button>
          <div className="w-px h-3 bg-white/[0.06] mx-0.5" />
          <button type="button" aria-label="Fit graph"
            onClick={fitToGraph}
            className="p-1 text-slate-600 hover:text-slate-400 transition-colors"
            title="Fit all nodes">
            <Maximize2 className="w-3 h-3" />
          </button>
          <button type="button" aria-label="Redistribute positions"
            onClick={resetStoredLayout}
            className="p-1 text-slate-600 hover:text-slate-400 transition-colors"
            title="Reset positions and clear layout cache">
            <RotateCcw className="w-3 h-3" />
          </button>
        </div>

        {/* Path tracing */}
        <div className={`flex items-center gap-1 rounded-lg px-1 py-0.5 border transition-colors ${
          pathMode || pathTrace.picks.length > 0
            ? 'border-cyan-400/30 bg-cyan-400/[0.06]'
            : 'border-white/[0.06]'
        }`}>
          <button type="button" aria-label="Trazar camino" title="Trazar camino"
            onClick={() => setPathMode(m => !m)}
            className={`p-1 transition-colors ${pathMode ? 'text-cyan-300' : 'text-slate-600 hover:text-slate-400'}`}>
            <Route className="w-3 h-3" />
          </button>
          {(pathMode || pathTrace.picks.length > 0) && (
            <span className={`text-[9px] font-mono w-12 text-center select-none ${
              pathTrace.found === false ? 'text-amber-400/80' : 'text-cyan-300/80'
            }`}>
              {pathTrace.found === true
                ? `${pathTrace.hops} saltos`
                : pathTrace.found === false
                  ? 'sin ruta'
                  : `${pathTrace.picks.length}/2`}
            </span>
          )}
          {pathTrace.picks.length > 0 && (
            <button type="button" aria-label="Clear trace"
              onClick={clearPathTrace}
              className="p-1 text-slate-600 hover:text-slate-400 transition-colors">
              <X className="w-3 h-3" />
            </button>
          )}
        </div>

        <div className="relative">
          <Search className="absolute left-2 top-1/2 -translate-y-1/2 w-3 h-3 text-slate-700 pointer-events-none" />
          <input
            type="text"
            value={query}
            onChange={e => { queryRef.current = e.target.value; setQuery(e.target.value); }}
            placeholder="Search neuron…"
            className="pl-6 pr-6 py-1 w-36 rounded-md bg-white/[0.04] border border-white/[0.06] text-xs text-slate-400 placeholder-slate-700 focus:outline-none focus:border-emerald-500/40 transition-colors"
          />
          {query && (
            <button type="button" aria-label="Clear"
              onClick={() => { queryRef.current = ''; setQuery(''); }}
              className="absolute right-1.5 top-1/2 -translate-y-1/2 text-slate-700 hover:text-slate-500">
              <X className="w-3 h-3" />
            </button>
          )}
        </div>

        <button type="button" aria-label="Métricas"
          onClick={() => setShowPerf(p => !p)}
          title="Show performance metrics"
          className={`p-1.5 rounded-md border transition-colors ${
            showPerf
              ? 'border-emerald-500/30 bg-emerald-500/[0.06] text-emerald-400'
              : 'border-white/[0.06] bg-white/[0.03] text-slate-600 hover:text-slate-400'
          }`}>
          <Activity className="w-3.5 h-3.5" />
        </button>

        <button type="button" aria-label="Recargar"
          onClick={() => { setSelected(null); clearPathTrace(); setReloadKey(k => k + 1); }}
          disabled={loading}
          className="p-1.5 rounded-md border border-white/[0.06] bg-white/[0.03] text-slate-600 hover:text-slate-400 transition-colors disabled:opacity-30">
          <RefreshCw className={`w-3.5 h-3.5 ${loading ? 'animate-spin' : ''}`} />
        </button>
      </div>

      {/* Canvas + panel */}
      <div className="flex flex-1 min-h-0">
        <div ref={wrapRef} className="flex-1 relative min-w-0 min-h-0">
          <canvas
            ref={canvasRef}
            style={{ position: 'absolute', inset: 0, width: '100%', height: '100%', cursor: 'grab' }}
            onMouseMove={onMouseMove}
            onMouseDown={onMouseDown}
            onMouseUp={onMouseUp}
            onMouseLeave={onMouseLeave}
            onClick={onClick}
          />

          {loading && (
            <div className="absolute inset-0 flex flex-col items-center justify-center gap-3"
                 style={{ background: BG_EDGE_COLOR }}>
              <RefreshCw className="w-5 h-5 animate-spin text-emerald-500/60" />
              <p className="text-[10px] text-slate-600 tracking-wider uppercase">Conectando sinapsis</p>
            </div>
          )}

          {error && (
            <div className="absolute inset-0 flex flex-col items-center justify-center gap-2 p-6">
              <p className="text-red-400/70 text-xs">{error}</p>
              <button type="button" onClick={() => setReloadKey(k => k + 1)}
                className="px-3 py-1 rounded text-[11px] text-slate-500 border border-slate-800 hover:text-slate-300 transition-colors">
                Reintentar
              </button>
            </div>
          )}

          {!loading && !error && stats.nodes === 0 && (
            <div className="absolute inset-0 flex flex-col items-center justify-center gap-1">
              <p className="text-[11px] text-slate-700">Empty hippocampus</p>
              <p className="text-[10px] text-slate-800">Ingest content to see memory grow</p>
            </div>
          )}

          {/* Perf overlay */}
          {showPerf && !loading && (
            <div className="absolute top-3 left-3 font-mono text-[8px] leading-relaxed text-slate-600 select-none pointer-events-none rounded bg-[rgba(1,4,9,0.75)] px-[7px] py-1 border border-white/[0.04]">
              <span className={perfSnap.fps < 30 ? 'text-amber-500' : 'text-emerald-500/80'}>{perfSnap.fps} fps</span>
              {'  ·  '}
              <span>tick {perfSnap.tickMs}ms</span>
              {'  ·  '}
              <span>render {perfSnap.renderMs}ms</span>
              {'  ·  '}
              <span>{perfSnap.pairs.toLocaleString()} pares</span>
              {'  ·  '}
              <span className={perfSnap.stable ? 'text-slate-700' : 'text-violet-400/70'}>{perfSnap.stable ? 'estable' : 'simulando'}</span>
            </div>
          )}

          {/* Legend */}
          {stats.nodes > 0 && !loading && (
            <div className="absolute bottom-3 left-3 flex flex-col gap-1 pointer-events-none select-none">
              {Object.entries(PALETTE)
                .filter(([k]) => nodes.current.some(n => n.kind === k))
                .map(([k, c]) => (
                  <div key={k} className="flex items-center gap-1.5">
                    <div className="w-1.5 h-1.5 rounded-full opacity-70 flex-shrink-0"
                         style={{ background: c }} />
                    <span className="text-[8px] text-slate-700 uppercase tracking-widest">{k}</span>
                  </div>
                ))}
            </div>
          )}
        </div>

        {/* Detail panel */}
        {selected && (
          <div className="w-52 flex-shrink-0 border-l border-white/[0.05] flex flex-col"
               style={{ background: 'rgba(3,13,30,0.95)' }}>

            <div className="p-4 border-b border-white/[0.05] flex items-center justify-between">
              <span className="px-1.5 py-0.5 rounded text-[9px] font-bold uppercase tracking-widest text-white/90"
                    style={{ background: selected.color + 'aa' }}>
                {selected.kind}
              </span>
              <button type="button" aria-label="Close"
                onClick={() => setSelected(null)}
                className="text-slate-700 hover:text-slate-500 transition-colors">
                <X className="w-3.5 h-3.5" />
              </button>
            </div>

            <div className="flex-1 overflow-y-auto p-4 space-y-4">
              <div>
                <p className="text-[8px] text-slate-700 uppercase font-bold tracking-widest mb-0.5">ID</p>
                <p className="text-[9px] font-mono text-violet-400/70 break-all leading-relaxed">{selected.id}</p>
              </div>

              {selected.last_agent && (
                <div>
                  <p className="text-[8px] text-slate-700 uppercase font-bold tracking-widest mb-0.5">Última Presencia</p>
                  <div className="flex items-center gap-1.5 mt-1">
                    <div className="w-1.5 h-1.5 rounded-full flex-shrink-0 animate-pulse"
                         style={{ background: getAgentColor(selected.last_agent) }} />
                    <span className="text-[10px] font-mono font-bold"
                          style={{ color: getAgentColor(selected.last_agent) }}>
                      {selected.last_agent}
                    </span>
                  </div>
                </div>
              )}

              <div>
                <p className="text-[8px] text-slate-700 uppercase font-bold tracking-widest mb-1.5">Peso sináptico</p>
                <div className="flex items-center gap-2">
                  <div className="flex-1 h-0.5 rounded-full overflow-hidden" style={{ background: 'rgba(255,255,255,0.05)' }}>
                    <div className="h-full rounded-full transition-all"
                         style={{ width: `${Math.min(100, selected.weight * 100)}%`, background: selected.color }} />
                  </div>
                  <span className="text-[9px] font-mono text-slate-600">{selected.weight.toFixed(2)}</span>
                </div>
              </div>

              {selected.created && (
                <div>
                  <p className="text-[8px] text-slate-700 uppercase font-bold tracking-widest mb-0.5">Formado</p>
                  <p className="text-[9px] text-slate-600 font-mono">{new Date(selected.created).toLocaleString()}</p>
                </div>
              )}

              {selected.content && (
                <div>
                  <p className="text-[8px] text-slate-700 uppercase font-bold tracking-widest mb-1">Traza mnémica</p>
                  <div className="rounded-lg p-2.5 max-h-44 overflow-y-auto"
                       style={{ background: 'rgba(255,255,255,0.02)', border: '1px solid rgba(255,255,255,0.04)' }}>
                    <p className="text-[9px] text-slate-500 leading-relaxed whitespace-pre-wrap">
                      {selected.content.slice(0, 500)}{selected.content.length > 500 && '…'}
                    </p>
                  </div>
                </div>
              )}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
