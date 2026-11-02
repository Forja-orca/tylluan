import React, { useState, useEffect, useMemo, useRef, useCallback } from 'react';
import { Network, RefreshCw, Zap, AlertCircle, Info, ArrowRight } from 'lucide-react';
import type { NexusBridge } from '../../lib/nexus-bridge';
import { cn } from '../../lib/utils';
import { nodeTypeColor } from './palette';

export interface RadialNode {
  id: string;
  label: string;
  content?: string;
  lifecycle_state?: string;
  stigmergy_heat?: number;
  node_type?: string;
  x: number;
  y: number;
  ring: number;
}

export interface RadialLink {
  source: string;
  target: string;
  weight?: number;
  type?: string;
}

export interface RadialSilvaGraphProps {
  bridge: NexusBridge | null;
  initialNodeId?: string | null;
  onSelectNode?: (nodeId: string) => void;
  className?: string;
}

/**
 * RadialSilvaGraph (AG-30b)
 * Visor de grafo SilvaDB de consumo de cómputo mínimo:
 * 1. Disposición radial calculada una sola vez al elegir el nodo (sin simulación física ni RAF).
 * 2. Máximo 40 vecinos, sin librerías externas (SVG nativo).
 * 3. Una sola petición cancelable (AbortController) a /api/v1/silva/graph al elegir nodo.
 * 4. Solo se renderiza y pide cuando el componente está montado.
 * 5. <8 KB bundle footprint, respeta prefers-reduced-motion.
 */
export function RadialSilvaGraph({
  bridge,
  initialNodeId = null,
  onSelectNode,
  className,
}: RadialSilvaGraphProps) {
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(initialNodeId);
  const [loading, setLoading] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const [rawNodes, setRawNodes] = useState<Array<Record<string, unknown>>>([]);
  const [rawLinks, setRawLinks] = useState<Array<{ source: string; target: string; weight?: number; type?: string }>>([]);
  const [activeInspectNode, setActiveInspectNode] = useState<RadialNode | null>(null);

  const abortControllerRef = useRef<AbortController | null>(null);

  // Dimensiones del lienzo SVG
  const width = 640;
  const height = 480;
  const cx = width / 2;
  const cy = height / 2;

  // Cancelable fetch: una sola petición a /api/v1/silva/graph al elegir nodo (o montar si hay initialNodeId)
  const fetchGraphData = useCallback(async (nodeIdToLoad?: string | null) => {
    if (!bridge) return;

    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
    }
    const controller = new AbortController();
    abortControllerRef.current = controller;

    setLoading(true);
    setError(null);

    try {
      // Máximo 40 vecinos según especificación AG-30b
      const rawData = await bridge.fetchRaw('/api/v1/silva/graph?limit=40', { signal: controller.signal });
      const data = rawData as {
        nodes?: Array<Record<string, unknown>>;
        links?: Array<{ source: string; target: string; weight?: number; type?: string }>;
      };

      const nodes = Array.isArray(data?.nodes) ? data.nodes : [];
      const links = Array.isArray(data?.links) ? data.links : [];

      setRawNodes(nodes);
      setRawLinks(links);

      // Si no había nodo seleccionado, seleccionar el primero o el más representativo
      if (!nodeIdToLoad && nodes.length > 0) {
        const firstId = String(nodes[0].id || '');
        setSelectedNodeId(firstId);
      }
    } catch (err: unknown) {
      if (err instanceof Error && err.name === 'AbortError') {
        return; // Petición cancelada intencionalmente
      }
      const msg = err instanceof Error ? err.message : String(err || 'Error al consultar el grafo SilvaDB');
      setError(msg);
    } finally {
      setLoading(false);
    }
  }, [bridge]);

  // Al montar o cuando cambia selectedNodeId explícitamente
  useEffect(() => {
    void fetchGraphData(selectedNodeId);
    return () => {
      if (abortControllerRef.current) {
        abortControllerRef.current.abort();
      }
    };
  }, [fetchGraphData, selectedNodeId]);

  // Disposición radial calculada una sola vez con useMemo (cero física, cero RAF)
  const { centerNode, layoutNodes, visibleLinks } = useMemo(() => {
    if (!rawNodes.length) {
      return { centerNode: null, layoutNodes: [], visibleLinks: [] };
    }

    const currentCentralId = selectedNodeId || String(rawNodes[0]?.id || '');
    const centerRaw = rawNodes.find((n) => String(n.id) === currentCentralId) || rawNodes[0];

    const cNode: RadialNode = {
      id: String(centerRaw.id || 'central'),
      label: String(centerRaw.label || centerRaw.title || centerRaw.id || 'Nodo Central'),
      content: String(centerRaw.content || centerRaw.text || ''),
      lifecycle_state: String(centerRaw.lifecycle_state || 'active'),
      stigmergy_heat: Number(centerRaw.stigmergy_heat || 0),
      node_type: String(centerRaw.node_type || centerRaw.type || 'memory'),
      x: cx,
      y: cy,
      ring: 0,
    };

    // Filtrar los vecinos directos o hasta un máximo de 40 nodos restantes
    const neighborsRaw = rawNodes
      .filter((n) => String(n.id) !== cNode.id)
      .slice(0, 40); // Criterio estricto AG-30b: máximo 40 vecinos

    const totalNeighbors = neighborsRaw.length;
    const computedNodes: RadialNode[] = [];

    // Estrategia de anillos concéntricos:
    // Si total <= 14: 1 solo anillo (R = 155)
    // Si total > 14: Anillo interior (primeros 12 a R = 115) y anillo exterior (hasta 28 a R = 195)
    neighborsRaw.forEach((n, idx) => {
      let r: number;
      let angle: number;
      let ring: number;

      if (totalNeighbors <= 14) {
        angle = (idx / totalNeighbors) * 2 * Math.PI - Math.PI / 2;
        r = 155;
        ring = 1;
      } else if (idx < 12) {
        angle = (idx / 12) * 2 * Math.PI - Math.PI / 2;
        r = 115;
        ring = 1;
      } else {
        const outerTotal = totalNeighbors - 12;
        const outerIdx = idx - 12;
        angle = (outerIdx / outerTotal) * 2 * Math.PI - Math.PI / 2;
        r = 195;
        ring = 2;
      }

      computedNodes.push({
        id: String(n.id),
        label: String(n.label || n.title || n.id),
        content: String(n.content || n.text || ''),
        lifecycle_state: String(n.lifecycle_state || 'active'),
        stigmergy_heat: Number(n.stigmergy_heat || 0),
        node_type: String(n.node_type || n.type || 'memory'),
        x: Math.round(cx + r * Math.cos(angle)),
        y: Math.round(cy + r * Math.sin(angle)),
        ring,
      });
    });

    // Mapear enlaces visibles conectados al centro o entre vecinos computados
    const nodePositionMap = new Map<string, { x: number; y: number }>();
    nodePositionMap.set(cNode.id, { x: cNode.x, y: cNode.y });
    for (const cn of computedNodes) {
      nodePositionMap.set(cn.id, { x: cn.x, y: cn.y });
    }

    const filteredLinks = rawLinks.filter((l) => {
      return nodePositionMap.has(String(l.source)) && nodePositionMap.has(String(l.target));
    });

    return {
      centerNode: cNode,
      layoutNodes: computedNodes,
      visibleLinks: filteredLinks,
    };
  }, [rawNodes, rawLinks, selectedNodeId, cx, cy]);

  const handleSelect = (nodeId: string) => {
    setSelectedNodeId(nodeId);
    if (onSelectNode) onSelectNode(nodeId);
    const target = layoutNodes.find((n) => n.id === nodeId);
    if (target) setActiveInspectNode(target);
  };

  return (
    <div
      className={cn(
        'flex-1 min-h-[380px] rounded-lg bg-card border border-border shadow-sm flex flex-col overflow-hidden font-sans',
        className
      )}
    >
      {/* Header bar con métricas instantáneas y estado */}
      <div className="flex items-center justify-between px-4 py-2.5 border-b border-border bg-muted/30">
        <div className="flex items-center gap-2">
          <Network className="w-4 h-4 text-primary" aria-hidden="true" />
          <span className="text-xs font-bold text-foreground">
            Constelación Radial SilvaDB (AG-30b)
          </span>
          {layoutNodes.length > 0 && (
            <span className="text-[11px] font-mono px-2 py-0.5 rounded bg-muted text-muted-foreground border border-border">
              {layoutNodes.length} vecinos proyectados (máx 40)
            </span>
          )}
        </div>

        <div className="flex items-center gap-2">
          {loading && (
            <span className="inline-flex items-center gap-1.5 text-xs text-muted-foreground font-mono">
              <RefreshCw className="w-3.5 h-3.5 animate-spin" aria-hidden="true" />
              Calculando radio...
            </span>
          )}
          <button
            type="button"
            onClick={() => void fetchGraphData(selectedNodeId)}
            disabled={loading}
            aria-label="Actualizar constelación radial"
            className="p-1.5 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none motion-reduce:transition-none"
            title="Recalcular proyección radial"
          >
            <RefreshCw className={cn('w-3.5 h-3.5', loading && 'animate-spin')} aria-hidden="true" />
          </button>
        </div>
      </div>

      {/* Área Principal de Gráfico SVG y Panel Lateral */}
      <div className="flex-1 relative flex flex-col md:flex-row min-h-0 overflow-hidden">
        {error ? (
          <div className="flex-1 flex flex-col items-center justify-center p-6 text-center text-rose-500">
            <AlertCircle className="w-8 h-8 mb-2" aria-hidden="true" />
            <p className="text-xs font-semibold">{error}</p>
            <button
              type="button"
              onClick={() => void fetchGraphData(selectedNodeId)}
              className="mt-3 px-3 py-1 bg-muted hover:bg-muted/80 text-foreground text-xs rounded border border-border font-medium cursor-pointer"
            >
              Reintentar
            </button>
          </div>
        ) : !centerNode ? (
          <div className="flex-1 flex flex-col items-center justify-center p-6 text-center text-muted-foreground">
            <Network className="w-10 h-10 mb-2 text-muted-foreground/40" aria-hidden="true" />
            <p className="text-xs">No hay recuerdos cargados en SilvaDB.</p>
          </div>
        ) : (
          <div className="flex-1 relative flex items-center justify-center p-2 min-h-[320px]">
            {/* SVG Canvas determinista calculado una sola vez */}
            <svg
              viewBox={`0 0 ${width} ${height}`}
              className="w-full h-full max-h-[460px] select-none"
              role="img"
              aria-label={`Grafo radial con centro en ${centerNode.label} y ${layoutNodes.length} vecinos`}
            >
              {/* Círculos de referencia para anillos concéntricos */}
              <circle
                cx={cx}
                cy={cy}
                r={115}
                className="stroke-border/40 fill-none stroke-dashed"
                strokeDasharray="3 3"
              />
              {layoutNodes.length > 12 && (
                <circle
                  cx={cx}
                  cy={cy}
                  r={195}
                  className="stroke-border/25 fill-none stroke-dashed"
                  strokeDasharray="4 4"
                />
              )}

              {/* Enlaces radiales (Centro -> Vecinos) */}
              <g className="links" stroke="currentColor" strokeOpacity="0.2">
                {layoutNodes.map((n) => (
                  <line
                    key={`center-to-${n.id}`}
                    x1={cx}
                    y1={cy}
                    x2={n.x}
                    y2={n.y}
                    className="stroke-border hover:stroke-primary transition-colors motion-reduce:transition-none"
                    strokeWidth="1.2"
                  />
                ))}

                {/* Enlaces inter-vecinos (si existen en el payload) */}
                {visibleLinks.map((link, idx) => {
                  const sNode = link.source === centerNode.id ? centerNode : layoutNodes.find((n) => n.id === link.source);
                  const tNode = link.target === centerNode.id ? centerNode : layoutNodes.find((n) => n.id === link.target);
                  if (!sNode || !tNode || (sNode === centerNode && tNode !== centerNode)) return null;
                  return (
                    <line
                      key={`link-${idx}`}
                      x1={sNode.x}
                      y1={sNode.y}
                      x2={tNode.x}
                      y2={tNode.y}
                      stroke="currentColor"
                      strokeOpacity="0.12"
                      strokeWidth="0.8"
                    />
                  );
                })}
              </g>

              {/* Nodos Vecinos */}
              <g className="neighbors">
                {layoutNodes.map((node) => {
                  const isHovered = activeInspectNode?.id === node.id;
                  return (
                    <g
                      key={node.id}
                      tabIndex={0}
                      role="button"
                      aria-label={`Nodo vecino ${node.label}, pulsa Enter para enfocar como centro`}
                      onClick={() => handleSelect(node.id)}
                      onMouseEnter={() => setActiveInspectNode(node)}
                      onKeyDown={(e) => {
                        if (e.key === 'Enter' || e.key === ' ') {
                          e.preventDefault();
                          handleSelect(node.id);
                        }
                      }}
                      className="cursor-pointer focus-visible:outline-none group"
                    >
                      <circle
                        cx={node.x}
                        cy={node.y}
                        r={isHovered ? 14 : 10}
                        fill={nodeTypeColor(node.node_type)}
                        stroke={isHovered ? 'currentColor' : 'hsl(var(--background))'}
                        className={cn(
                          'transition-all motion-reduce:transition-none duration-150 stroke-2',
                          isHovered && 'stroke-foreground'
                        )}
                      />
                      {/* Etiqueta recortada */}
                      <text
                        x={node.x}
                        y={node.y + 18}
                        textAnchor="middle"
                        className="text-[10px] font-sans fill-muted-foreground group-hover:fill-foreground select-none pointer-events-none transition-colors motion-reduce:transition-none"
                      >
                        {node.label.length > 14 ? `${node.label.slice(0, 12)}…` : node.label}
                      </text>
                    </g>
                  );
                })}
              </g>

              {/* Nodo Central Dominante */}
              <g
                tabIndex={0}
                role="button"
                aria-label={`Nodo central ${centerNode.label}`}
                onClick={() => setActiveInspectNode(centerNode)}
                className="cursor-pointer focus-visible:outline-none"
              >
                <circle
                  cx={centerNode.x}
                  cy={centerNode.y}
                  r={26}
                  fill={nodeTypeColor(centerNode.node_type)}
                  className="stroke-background stroke-2 shadow-md hover:opacity-90 transition-all motion-reduce:transition-none"
                />
                <circle
                  cx={centerNode.x}
                  cy={centerNode.y}
                  r={30}
                  className="fill-none stroke-primary/30 stroke-1"
                />
                <Zap
                  x={centerNode.x - 8}
                  y={centerNode.y - 8}
                  className="w-4 h-4 text-primary-foreground pointer-events-none"
                  aria-hidden="true"
                />
                <text
                  x={centerNode.x}
                  y={centerNode.y + 44}
                  textAnchor="middle"
                  className="text-xs font-bold font-sans fill-foreground select-none pointer-events-none"
                >
                  {centerNode.label.length > 18 ? `${centerNode.label.slice(0, 16)}…` : centerNode.label}
                </text>
              </g>
            </svg>
          </div>
        )}

        {/* Panel lateral colapsable / inspector del nodo activo */}
        {activeInspectNode && (
          <div className="w-full md:w-64 border-t md:border-t-0 md:border-l border-border bg-card p-3.5 flex flex-col justify-between text-xs font-sans">
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <span className="text-[10px] uppercase font-bold text-muted-foreground tracking-wider flex items-center gap-1">
                  <Info className="w-3 h-3 text-primary" aria-hidden="true" />
                  Detalle del Nodo
                </span>
                <span
                  className={cn(
                    'px-1.5 py-0.5 rounded text-[10px] font-mono border',
                    activeInspectNode.lifecycle_state === 'active'
                      ? 'bg-emerald-500/10 text-emerald-500 border-emerald-500/20'
                      : 'bg-muted text-muted-foreground border-border'
                  )}
                >
                  {activeInspectNode.lifecycle_state || 'activo'}
                </span>
              </div>

              <h5 className="font-bold text-foreground text-sm leading-snug line-clamp-2">
                {activeInspectNode.label}
              </h5>

              {activeInspectNode.content && (
                <p className="text-muted-foreground text-[11px] leading-relaxed line-clamp-4 bg-muted/40 p-2 rounded border border-border/50">
                  {activeInspectNode.content}
                </p>
              )}

              <div className="grid grid-cols-2 gap-1.5 pt-1 font-mono text-[10px] text-muted-foreground">
                <div>
                  <span className="block text-[9px] uppercase text-muted-foreground/70">Tipo</span>
                  <span className="text-foreground font-semibold">{activeInspectNode.node_type}</span>
                </div>
                <div>
                  <span className="block text-[9px] uppercase text-muted-foreground/70">Stigmergy</span>
                  <span className="text-foreground font-semibold">
                    {activeInspectNode.stigmergy_heat?.toFixed(2) || '0.00'}
                  </span>
                </div>
              </div>
            </div>

            {activeInspectNode.id !== centerNode?.id && (
              <button
                type="button"
                onClick={() => handleSelect(activeInspectNode.id)}
                className="mt-3 w-full py-1.5 px-2 bg-primary/10 hover:bg-primary/20 text-primary border border-primary/30 rounded font-semibold text-xs flex items-center justify-center gap-1.5 transition-colors cursor-pointer motion-reduce:transition-none focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
              >
                <span>Enfocar como centro</span>
                <ArrowRight className="w-3.5 h-3.5" aria-hidden="true" />
              </button>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
