/**
 * KnowledgeCortex2D — the memory graph, 2D and light.
 *
 * Replaces KnowledgeCortex3D (react-force-graph-3d + three.js/WebGL) after
 * direct user feedback: the 3D cortex was heavy ("gran consumo de recursos
 * para el UI") and, worse, not USEFUL — ambient camera drift and per-node
 * emissive breathing are decoration, not information. This renderer keeps
 * exactly what carries meaning and drops what doesn't:
 *
 *  - KEPT (real utility): real SilvaDB data via bridge.getSilvaGraph
 *    (/api/v1/graph/viz), semantic node colors by type (palette.ts),
 *    Louvain cluster rings when community data exists, connected-link
 *    highlighting on hover/select, live refresh on memory_added/updated
 *    events, zoom-to-fit once the layout settles, native hover tooltip with
 *    the node's real content.
 *  - DROPPED (pure cost): three.js/WebGL entirely, the idle "breathing"
 *    rAF loop (per-frame material mutation forever), idle camera drift,
 *    emissive glow, pause button (there is no ambient life left to pause —
 *    the d3 physics self-cools to ZERO cpu once the layout settles and the
 *    canvas only repaints on interaction).
 *
 * Same library family as the 3D one (react-force-graph-2d, canvas 2D instead
 * of WebGL) so the data flow and the force tuning learned here (the
 * "Thomson atom" collapse fix: flat link strength decoupled from node degree
 * at ~22 edges/node density) carry over 1:1.
 */
import { useEffect, useMemo, useRef, useState, useCallback } from 'react';
import ForceGraph2D from 'react-force-graph-2d';
import { RefreshCw, Maximize2 } from 'lucide-react';
import type { NexusBridge, GraphNode, NexusEvent } from '../../lib/nexus-bridge';
import { nodeTypeColor, clusterRingColor, CORTEX_BACKGROUND } from './palette';

interface CortexNode extends GraphNode {
  cluster_id?: number | string;
  stigmergy_heat?: number;
  x?: number; y?: number;
}
interface CortexLink { source: string; target: string; }
interface CortexData { nodes: CortexNode[]; links: CortexLink[]; }

interface Props {
  bridge: NexusBridge | null;
  events?: NexusEvent[];
  onNodeClick?: (node: GraphNode) => void;
}

const REFRESH_DEBOUNCE_MS = 1500;
const COOLDOWN_MS = 6000; // physics budget per layout settle — then idle at 0 cpu

function normalizeLinks(raw: any[]): CortexLink[] {
  return raw
    .map((l) => ({
      source: String(l.source ?? l.from ?? l.s ?? ''),
      target: String(l.target ?? l.to ?? l.t ?? ''),
    }))
    .filter((l) => l.source && l.target);
}

export function KnowledgeCortex2D({ bridge, events, onNodeClick }: Props) {
  const fgRef = useRef<any>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [dims, setDims] = useState({ w: 800, h: 500 });
  const [data, setData] = useState<CortexData>({ nodes: [], links: [] });
  const [loading, setLoading] = useState(true);
  const [hoverNode, setHoverNode] = useState<any>(null);
  const [selectedNode, setSelectedNode] = useState<any>(null);
  const seenEventTsRef = useRef(0);
  const refreshTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const hasAutoFramedRef = useRef(false);

  // Highlight links that connect to whatever the user is looking at.
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

  // ── Data loading (real SilvaDB data, no mocks) ─────────────────────────────
  const load = useCallback(async () => {
    if (!bridge) return;
    try {
      const g = await bridge.getSilvaGraph(500, true);
      setData({
        nodes: (g.nodes as CortexNode[]) || [],
        links: normalizeLinks(g.edges || []),
      });
    } catch {
      // Transient — the panel keeps showing the last good graph rather than blanking.
    } finally {
      setLoading(false);
    }
  }, [bridge]);

  useEffect(() => { load(); }, [load]);

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
        // Learned from the 3D cortex (see its history): at 500 nodes / ~11k
        // edges, d3's default link.strength() (scales with degree) collapses
        // the graph into a tight ball. Flat constant strength decouples the
        // aggregate spring pull from node degree. Center force untouched —
        // cranking it fights charge repulsion and re-collapses the graph.
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
    refreshTimerRef.current = setTimeout(() => load(), REFRESH_DEBOUNCE_MS);
    return () => { if (refreshTimerRef.current) clearTimeout(refreshTimerRef.current); };
  }, [events, load]);

  // ── Node painter: type fill + cluster ring + label only where it matters ───
  // Full replacement of the default painter (nodeCanvasObjectMode replace):
  // one circle + optional one stroke per node is the whole cost. Text is the
  // expensive part at graph scale, so labels render only on hover/select or
  // when the user has zoomed in enough to read them anyway.
  const paintNode = useCallback((node: any, ctx: CanvasRenderingContext2D, globalScale: number) => {
    const n = node as CortexNode;
    const weight = Number.isFinite(n.weight) ? Math.min(1, Number(n.weight)) : 0.4;
    const r = 3 + weight * 5;
    const x = n.x ?? 0;
    const y = n.y ?? 0;

    ctx.beginPath();
    ctx.arc(x, y, r, 0, 2 * Math.PI, false);
    ctx.fillStyle = nodeTypeColor(n.node_type || (n as any).type);
    ctx.fill();

    const isActive = hoverNode === node || selectedNode === node;
    if (isActive) {
      ctx.lineWidth = 1.5 / globalScale;
      ctx.strokeStyle = '#e2e8f0';
      ctx.stroke();
    } else {
      const ring = clusterRingColor(n.cluster_id === undefined ? undefined : Number(n.cluster_id));
      if (ring) {
        ctx.lineWidth = 0.9 / globalScale;
        ctx.strokeStyle = ring;
        ctx.stroke();
      }
    }

    if (isActive || globalScale >= 1.8) {
      const label = String(n.content || (n as any).label || n.id || '').slice(0, 42);
      // Escalado inverso al zoom: 10px CONSTANTES en pantalla. Un floor en
      // unidades de mundo (p.ej. 3) hace que a zoom alto el label explote
      // (3px de mundo × 8 de zoom = 24px visuales y creciendo) — cazado en la
      // prueba visual en vivo con zoom de rueda.
      const fontSize = 10 / globalScale;
      ctx.font = `${fontSize}px ui-monospace, SFMono-Regular, Menlo, monospace`;
      ctx.textAlign = 'center';
      ctx.textBaseline = 'top';
      ctx.fillStyle = 'rgba(226, 232, 240, 0.92)';
      ctx.fillText(label, x, y + r + 1.5 / globalScale);
    }
  }, [hoverNode, selectedNode]);

  // Hit area must match the custom painter or hover/click detection breaks.
  const paintNodeHitArea = useCallback((node: any, color: string, ctx: CanvasRenderingContext2D) => {
    const n = node as CortexNode;
    ctx.beginPath();
    ctx.arc(n.x ?? 0, n.y ?? 0, 8, 0, 2 * Math.PI, false);
    ctx.fillStyle = color;
    ctx.fill();
  }, []);

  const handleNodeClick = useCallback((node: any) => {
    setSelectedNode(node);
    onNodeClick?.(node as GraphNode);
    const n = node as CortexNode;
    fgRef.current?.centerAt?.(n.x ?? 0, n.y ?? 0, 500);
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

      <div className="absolute top-3 right-3 z-10">
        <button
          type="button"
          onClick={() => fgRef.current?.zoomToFit?.(600, 40)}
          title="Fit to view"
          className="p-1.5 rounded bg-background/40 backdrop-blur border border-slate-800/60 text-slate-400 hover:text-amber-300 hover:border-amber-500/40 transition-colors cursor-pointer"
        >
          <Maximize2 className="w-3.5 h-3.5" />
        </button>
      </div>

      <ForceGraph2D
        ref={fgRef}
        graphData={useMemo(() => ({ nodes: data.nodes as any[], links: data.links as any[] }), [data])}
        width={dims.w}
        height={dims.h}
        backgroundColor={CORTEX_BACKGROUND}
        nodeCanvasObject={paintNode}
        nodePointerAreaPaint={paintNodeHitArea}
        nodeLabel={(n: any) => `${n.node_type || n.type || 'node'} · ${(n.content || n.label || n.id || '').toString().slice(0, 160)}`}
        linkColor={(link: any) => {
          const isConnected = isLinkConnectedToActiveNode(link);
          if (hoverNode || selectedNode) {
            return isConnected ? 'rgba(34, 211, 238, 0.85)' : 'rgba(148, 163, 184, 0.04)';
          }
          return 'rgba(148, 163, 184, 0.16)';
        }}
        linkWidth={(link: any) => (isLinkConnectedToActiveNode(link) && (hoverNode || selectedNode) ? 1.4 : 0.5)}
        cooldownTime={COOLDOWN_MS}
        d3VelocityDecay={0.35}
        minZoom={0.15}
        maxZoom={8}
        onNodeHover={(node: any) => setHoverNode(node)}
        onNodeClick={handleNodeClick}
        onEngineStop={() => {
          // Layout settled (or hit the time budget) — physics idle from here
          // until a refetch reheats it. Frame the whole graph once so a dense
          // 500-node graph doesn't open stuck on the default viewport.
          if (!hasAutoFramedRef.current) {
            hasAutoFramedRef.current = true;
            fgRef.current?.zoomToFit?.(800, 60);
          }
        }}
      />
    </div>
  );
}
