/**
 * KnowledgeCortex3D — the living memory graph, in three dimensions.
 *
 * Replaces the hand-rolled Canvas2D/Barnes-Hut engine (HippocampusGraph +
 * graph/simulation.ts, ~1600 lines, zero library dependencies) with
 * react-force-graph's ForceGraph3D — a proven WebGL/three.js engine used
 * across the open-source graph-viz ecosystem (vasturiano/3d-force-graph).
 *
 * Design goals (per direct instruction, not a generic dashboard widget):
 *  - "Alive" without burning resources: physics cools down and stops once
 *    the layout settles (cooldownTicks), a short reheat burst runs only when
 *    new nodes actually arrive, and the idle pulse/rotation loop pauses
 *    entirely when the tab isn't visible or the panel isn't mounted.
 *  - Semantic color language preserved from the old engine (node type fill,
 *    Louvain cluster ring) — see ./palette.ts — but re-skinned to the owl
 *    branding (deep navy background, cyan/violet/amber accents) instead of
 *    the generic emerald dashboard palette.
 *  - Idle camera drift + per-node emissive "breathing" reads as alive at a
 *    glance without a continuous physics recompute driving it.
 */
import { useEffect, useMemo, useRef, useState, useCallback } from 'react';
import * as THREE from 'three';
import ForceGraph3D from 'react-force-graph-3d';
import { RefreshCw, Maximize2, Pause, Play } from 'lucide-react';
import type { NexusBridge, GraphNode, NexusEvent } from '../../lib/nexus-bridge';
import { nodeTypeColor, clusterRingColor, CORTEX_BACKGROUND } from './palette';

interface CortexNode extends GraphNode {
  cluster_id?: number;
  stigmergy_heat?: number;
  x?: number; y?: number; z?: number;
}
interface CortexLink { source: string; target: string; }
interface CortexData { nodes: CortexNode[]; links: CortexLink[]; }

interface Props {
  bridge: NexusBridge | null;
  events?: NexusEvent[];
  onNodeClick?: (node: GraphNode) => void;
}

const IDLE_ROTATE_DELAY_MS = 4000;
const PULSE_FRAME_MS = 90; // ~11fps for the breathing loop — plenty for a slow glow, cheap on the GPU
const REFRESH_DEBOUNCE_MS = 1500;

function normalizeLinks(raw: any[]): CortexLink[] {
  return raw
    .map((l) => ({
      source: String(l.source ?? l.from ?? l.s ?? ''),
      target: String(l.target ?? l.to ?? l.t ?? ''),
    }))
    .filter((l) => l.source && l.target);
}

export function KnowledgeCortex3D({ bridge, events, onNodeClick }: Props) {
  const fgRef = useRef<any>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [dims, setDims] = useState({ w: 800, h: 500 });
  const [data, setData] = useState<CortexData>({ nodes: [], links: [] });
  const [loading, setLoading] = useState(true);
  const [paused, setPaused] = useState(false);
  const [hoverNode, setHoverNode] = useState<any>(null);
  const [selectedNode, setSelectedNode] = useState<any>(null);
  const lastInteractionRef = useRef(Date.now());
  const seenEventTsRef = useRef(0);
  const refreshTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const hasAutoFramedRef = useRef(false);

  // Helper to check if a link is connected to the active focus node
  const isLinkConnectedToActiveNode = useCallback((link: any) => {
    if (!hoverNode && !selectedNode) return true;
    const activeId = hoverNode?.id || selectedNode?.id;
    const sourceId = typeof link.source === 'object' ? link.source.id : link.source;
    const targetId = typeof link.target === 'object' ? link.target.id : link.target;
    return sourceId === activeId || targetId === activeId;
  }, [hoverNode, selectedNode]);

  // ── Sizing ──────────────────────────────────────────────────────────────────
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const ro = new ResizeObserver(([entry]) => {
      const { width, height } = entry.contentRect;
      if (width > 10 && height > 10) setDims({ w: width, h: height });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // ── Resource disposal logic to prevent GPU memory leaks (Three.js WebGL) ────
  const disposeThreeResources = useCallback(() => {
    const scene = fgRef.current?.scene?.();
    if (!scene) return;

    scene.traverse((obj: any) => {
      if (obj.isMesh && obj.userData?.__cortexMaterial) {
        if (obj.geometry) {
          obj.geometry.dispose();
        }
        if (obj.material) {
          if (Array.isArray(obj.material)) {
            obj.material.forEach((m: any) => m.dispose());
          } else {
            obj.material.dispose();
          }
        }
      }
    });
  }, []);

  // ── Data loading ────────────────────────────────────────────────────────────
  const load = useCallback(async (reheat: boolean) => {
    if (!bridge) return;
    try {
      // Free GPU memory from prior nodes before React replaces them
      disposeThreeResources();

      const g = await bridge.getSilvaGraph(500, true);
      setData({
        nodes: (g.nodes as CortexNode[]) || [],
        links: normalizeLinks(g.edges || []),
      });
      if (reheat) {
        // A brief re-settle burst for the new arrivals — not a continuous simulation.
        requestAnimationFrame(() => fgRef.current?.d3ReheatSimulation?.());
      }
    } catch {
      // Transient — the panel keeps showing the last good graph rather than blanking.
    } finally {
      setLoading(false);
    }
  }, [bridge, disposeThreeResources]);

  useEffect(() => { load(false); }, [load]);

  // Unmount cleanup
  useEffect(() => {
    return () => {
      disposeThreeResources();
    };
  }, [disposeThreeResources]);

  // Adapt d3-force parameters once data is loaded using a retry-interval
  // to avoid asynchrony race conditions before simulation mounts.
  useEffect(() => {
    if (data.nodes.length === 0) return;
    const graph = fgRef.current;
    if (!graph) return;

    let attempts = 0;
    const initForces = () => {
      const charge = graph.d3Force('charge');
      const link = graph.d3Force('link');

      if (charge && link) {
        // This is a genuinely dense graph (500 nodes, ~11k edges from
        // next_chunk ingest chains -- ~22 edges/node average). d3-force's
        // default link.strength() scales with node degree, so at this
        // density the aggregate "spring" pull from thousands of edges
        // overwhelms any reasonable charge repulsion and the whole graph
        // contracts into a tight ball ("Thomson atom"), no matter how
        // strong the repulsion is cranked. Fix: decouple link strength
        // from degree (flat constant instead of the default formula).
        //
        // We do NOT touch the `center` force here. A previous attempt
        // boosted center.strength() to hold disconnected orphans near the
        // cluster -- but forceCenter recenters the WHOLE graph's centroid
        // every tick, not just orphans, so cranking it up was actively
        // fighting the charge repulsion and re-collapsing everything back
        // toward the origin. It has a sane low default; leave it alone.
        charge.strength(-220);
        link.distance(60);
        link.strength(0.04);

        graph.d3ReheatSimulation();
      } else if (attempts < 15) {
        attempts++;
        setTimeout(initForces, 80);
      }
    };

    initForces();
  }, [data.nodes.length]);

  // Live updates: memory_added/memory_updated events only carry {id, node_type},
  // not the full node — refetch (debounced) rather than guess the shape.
  useEffect(() => {
    if (!events?.length) return;
    const relevant = events.filter(
      (e) => (e.type === 'memory_added' || e.type === 'memory_updated') && e.ts > seenEventTsRef.current
    );
    if (relevant.length === 0) return;
    seenEventTsRef.current = events[events.length - 1].ts;
    if (refreshTimerRef.current) clearTimeout(refreshTimerRef.current);
    refreshTimerRef.current = setTimeout(() => load(true), REFRESH_DEBOUNCE_MS);
    return () => { if (refreshTimerRef.current) clearTimeout(refreshTimerRef.current); };
  }, [events, load]);

  // ── Node visuals: sphere fill by type, translucent ring by cluster ──────────
  const nodeThreeObject = useCallback((node: any) => {
    const n = node as CortexNode;
    const weight = Number.isFinite(n.weight) ? Math.min(1, Number(n.weight)) : 0.4;
    const radius = Math.max(2.4, weight * 5.5);
    const color = nodeTypeColor(n.node_type || n.type);

    const group = new THREE.Group();
    const geom = new THREE.SphereGeometry(radius, 16, 16);
    const mat = new THREE.MeshStandardMaterial({
      color,
      emissive: color,
      emissiveIntensity: 0.35,
      roughness: 0.45,
      metalness: 0.15,
    });
    const mesh = new THREE.Mesh(geom, mat);
    mesh.userData.__cortexMaterial = true;
    mesh.userData.__baseEmissive = 0.35;
    mesh.userData.__heat = n.stigmergy_heat ?? 0;
    group.add(mesh);

    const ringColor = clusterRingColor(n.cluster_id);
    if (ringColor) {
      const ringGeom = new THREE.RingGeometry(radius * 1.6, radius * 1.85, 24);
      const ringMat = new THREE.MeshBasicMaterial({ color: ringColor, transparent: true, opacity: 0.55, side: THREE.DoubleSide });
      const ring = new THREE.Mesh(ringGeom, ringMat);
      ring.userData.__cortexMaterial = true;
      ring.rotation.x = Math.PI / 2.4;
      group.add(ring);
    }
    return group;
  }, []);

  // ── Idle breathing pulse — cheap, gated, no physics involved ────────────────
  useEffect(() => {
    let raf = 0;
    let lastTick = 0;
    let stopped = false;

    const tick = (t: number) => {
      if (stopped) return;
      raf = requestAnimationFrame(tick);
      if (document.visibilityState !== 'visible' || paused) return;
      if (t - lastTick < PULSE_FRAME_MS) return;
      lastTick = t;

      const scene = fgRef.current?.scene?.();
      if (!scene) return;
      const now = Date.now();
      scene.traverse((obj: any) => {
        if (!obj.userData?.__cortexMaterial) return;
        const heat = obj.userData.__heat || 0;
        const base = obj.userData.__baseEmissive ?? 0.35;
        const speed = 0.0016 + Math.min(heat, 5) * 0.0006; // busier nodes breathe faster
        const wave = (Math.sin(now * speed + obj.id) + 1) / 2; // 0..1
        obj.material.emissiveIntensity = base + wave * 0.35;
      });
    };
    raf = requestAnimationFrame(tick);
    return () => { stopped = true; cancelAnimationFrame(raf); };
  }, [paused]);

  // ── Idle camera drift — pauses immediately on interaction ───────────────────
  useEffect(() => {
    const markInteraction = () => { lastInteractionRef.current = Date.now(); };
    const el = containerRef.current;
    el?.addEventListener('pointerdown', markInteraction);
    el?.addEventListener('wheel', markInteraction, { passive: true });

    const interval = setInterval(() => {
      const controls = fgRef.current?.controls?.();
      if (!controls) return;
      const idle = Date.now() - lastInteractionRef.current > IDLE_ROTATE_DELAY_MS;
      controls.autoRotate = idle && !paused && document.visibilityState === 'visible';
      controls.autoRotateSpeed = 0.35;
    }, 500); // cheap poll, not per-frame

    return () => {
      el?.removeEventListener('pointerdown', markInteraction);
      el?.removeEventListener('wheel', markInteraction);
      clearInterval(interval);
    };
  }, [paused]);

  const handleNodeClick = useCallback((node: any) => {
    lastInteractionRef.current = Date.now();
    onNodeClick?.(node as GraphNode);
    const distance = 90;
    const distRatio = 1 + distance / Math.hypot(node.x || 1, node.y || 1, node.z || 1);
    fgRef.current?.cameraPosition?.(
      { x: (node.x || 0) * distRatio, y: (node.y || 0) * distRatio, z: (node.z || 0) * distRatio },
      node,
      800,
    );
  }, [onNodeClick]);

  const nodeCount = data.nodes.length;
  const linkCount = data.links.length;

  return (
    <div ref={containerRef} className="relative flex-1 min-h-0 rounded-xl border border-slate-800/80 overflow-hidden" style={{ background: CORTEX_BACKGROUND }}>
      {loading && (
        <div className="absolute inset-0 flex items-center justify-center gap-2 text-xs text-slate-500 font-mono z-10">
          <RefreshCw className="w-4 h-4 animate-spin" /> Querying SilvaDB...
        </div>
      )}

      <div className="absolute top-3 left-3 z-10 flex items-center gap-2 font-mono text-[10px] text-slate-400">
        <span className="px-2 py-1 rounded bg-background/40 backdrop-blur border border-slate-800/60">
          {nodeCount} nodes · {linkCount} edges
        </span>
      </div>

      <div className="absolute top-3 right-3 z-10 flex items-center gap-1.5">
        <button
          type="button"
          onClick={() => setPaused((p) => !p)}
          title={paused ? 'Resume ambient life' : 'Pause ambient life'}
          className="p-1.5 rounded bg-background/40 backdrop-blur border border-slate-800/60 text-slate-400 hover:text-amber-300 hover:border-amber-500/40 transition-colors cursor-pointer"
        >
          {paused ? <Play className="w-3.5 h-3.5" /> : <Pause className="w-3.5 h-3.5" />}
        </button>
        <button
          type="button"
          onClick={() => fgRef.current?.zoomToFit?.(600, 40)}
          title="Fit to view"
          className="p-1.5 rounded bg-background/40 backdrop-blur border border-slate-800/60 text-slate-400 hover:text-amber-300 hover:border-amber-500/40 transition-colors cursor-pointer"
        >
          <Maximize2 className="w-3.5 h-3.5" />
        </button>
      </div>

      <ForceGraph3D
        ref={fgRef}
        graphData={useMemo(() => ({ nodes: data.nodes as any[], links: data.links as any[] }), [data])}
        width={dims.w}
        height={dims.h}
        backgroundColor={CORTEX_BACKGROUND}
        nodeThreeObject={nodeThreeObject}
        nodeThreeObjectExtend={false}
        nodeLabel={(n: any) => `${n.node_type || n.type || 'node'} · ${(n.content || n.label || n.id || '').toString().slice(0, 80)}`}
        linkColor={(link: any) => {
          const isConnected = isLinkConnectedToActiveNode(link);
          if (hoverNode || selectedNode) {
            return isConnected ? 'rgba(34, 211, 238, 0.85)' : 'rgba(148, 163, 184, 0.02)';
          }
          return 'rgba(148, 163, 184, 0.16)';
        }}
        linkWidth={(link: any) => {
          const isConnected = isLinkConnectedToActiveNode(link);
          if (hoverNode || selectedNode) {
            return isConnected ? 1.25 : 0.08;
          }
          return 0.45;
        }}
        linkDirectionalParticles={0}
        cooldownTime={8000}
        d3VelocityDecay={0.35}
        onNodeHover={(node) => setHoverNode(node)}
        onNodeClick={(node) => {
          setSelectedNode(node);
          handleNodeClick(node);
        }}
        onEngineStop={() => {
          // Layout has settled (or hit the 8s time budget) — physics idle from
          // here until reheated by a new arrival. Frame the whole graph once,
          // the first time it stops, so a dense 500-node graph doesn't open
          // stuck on whatever the default camera distance happened to be.
          if (!hasAutoFramedRef.current) {
            hasAutoFramedRef.current = true;
            fgRef.current?.zoomToFit?.(800, 60);
          }
        }}
      />
    </div>
  );
}
