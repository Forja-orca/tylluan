'use client';

import { motion } from 'framer-motion';

interface RoadmapItem {
  id: number;
  title: string;
  description: string;
  source: string;
  effort: 'low' | 'medium' | 'medium-high' | 'high';
  risk: 'minimal' | 'low' | 'medium';
  phase: 1 | 2 | 3;
  status: 'active' | 'proposed' | 'future';
  impact: string;
  deps?: string[];
}

const ROADMAP_ITEMS: RoadmapItem[] = [
  {
    id: 1,
    title: 'FSRS Memory Model',
    description: 'Reemplazar half-life fijo (T½=14d) por modelo por memoria con stability, difficulty, retrievability',
    source: 'FSRS (Ye et al.)',
    effort: 'low',
    risk: 'minimal',
    phase: 1,
    status: 'active',
    impact: 'Cada memoria tiene su curva de olvido personalizada',
  },
  {
    id: 2,
    title: 'Deterministic Freshness Resolution',
    description: 'Resolver conflictos de federación con reglas deterministas (SH-conflict, protected, peer priority, timestamp, tiebreak). Implementado en consensus.rs + wired en 4 paths federation sync',
    source: '"Don\'t Ask the LLM to Track Freshness" (Reddy & Challaram)',
    effort: 'low',
    risk: 'minimal',
    phase: 1,
    status: 'active',
    impact: 'Federación semánticamente determinista · 9 tests de resolución · Zero LLM en critical path',
  },
  {
    id: 3,
    title: 'Dashboard Modular + M26 Canvas',
    description: 'Dashboard modular con 4 tabs (PREVIEW/DOCS/WHITEBOARD/KNOWLEDGE). ColoquioCanvasWorkspace con Tldraw, sigma.js para grafo >500 nodos, FleetTab, FederationTab, GuildsTab.',
    source: 'M26 implementación real',
    effort: 'medium',
    risk: 'low',
    phase: 1,
    status: 'active',
    impact: 'Dashboard funcional con whiteboard Tldraw, grafo interactivo, federación y métricas',
  },
  {
    id: 4,
    title: 'Local Tool-Calling Router',
    description: 'Modelo 26M-1B (ONNX) para routing de guilds 100% local. Elimina dependencia cloud.',
    source: 'Needle 26M, Granite 350M, Arch-Function 1B',
    effort: 'medium',
    risk: 'low',
    phase: 1,
    status: 'proposed',
    impact: 'Soberanía total: "sin nube" sin asterisco',
    deps: ['Complexity Cascade exists'],
  },
  {
    id: 5,
    title: 'HippoRAG-PPR Retrieval',
    description: 'Añadir Personalized PageRank como tercer brazo del retrieval (asociativo, no similitud)',
    source: 'HippoRAG 2 (OSU-NLP)',
    effort: 'medium',
    risk: 'low',
    phase: 2,
    status: 'proposed',
    impact: '"Recupero lo asociado" — diferenciador vs Mem0/Letta',
    deps: ['FSRS stability data mature'],
  },
  {
    id: 6,
    title: 'DreamCycle (SleepCycle NREM)',
    description: 'DreamCycle implementado: deduplicación + decay por saliencia + detección de contradicciones. Corre en NightConsolidation horaria. Consolidación episódica vía consolidate_episodes.',
    source: 'SCM (Shinde) + implementación propia',
    effort: 'medium',
    risk: 'low',
    phase: 2,
    status: 'active',
    impact: 'Deduplicación automática, decay de nodos low-weight, detección de contradicciones cada hora',
    deps: ['FSRS in production'],
  },
  {
    id: 7,
    title: 'KNEXA-FL LinUCB Dispatch',
    description: 'Reemplazar heuristic weights por bandit contextual que aprende de outcomes',
    source: 'KNEXA-FL (Singh et al., AAAI 2026)',
    effort: 'medium',
    risk: 'medium',
    phase: 2,
    status: 'future',
    impact: 'Mesh de "demo técnica" a "infraestructura adaptativa"',
    deps: ['Multiple active peers', 'Dispatch logging'],
  },
  {
    id: 8,
    title: 'Mem0 Benchmark + Taxonomy',
    description: 'Taxonomía documentada en SPEC.md. Benchmark contra Mem0/Letta cerrado en M17/M15-P3. Re-benchmark contra Mem0 10-way comparison.',
    source: 'Mem0 (ECAI 2025) + Survey (47 authors)',
    effort: 'medium',
    risk: 'minimal',
    phase: 3,
    status: 'active',
    impact: 'Proyecto citable · taxonomía publicada · M17/M15-P3 cerrados',
    deps: ['Reproducible eval harness'],
  },
];

const EFFORT_COLORS = {
  low: 'text-emerald-400 bg-emerald-400/10 border-emerald-400/20',
  medium: 'text-amber-400 bg-amber-400/10 border-amber-400/20',
  'medium-high': 'text-orange-400 bg-orange-400/10 border-orange-400/20',
  high: 'text-red-400 bg-red-400/10 border-red-400/20',
};

const RISK_COLORS = {
  minimal: 'text-emerald-400',
  low: 'text-amber-400',
  medium: 'text-red-400',
};

const STATUS_STYLES = {
  active: { label: '✓ ACTIVE', color: 'text-emerald-400 border-emerald-400/30 bg-emerald-400/5' },
  proposed: { label: '→ PROPOSED', color: 'text-amber-400 border-amber-400/30 bg-amber-400/5' },
  future: { label: '◎ FUTURE', color: 'text-slate-400 border-slate-400/30 bg-slate-400/5' },
};

const PHASE_COLORS = {
  1: { bg: 'bg-teal-500/10', border: 'border-teal-500/30', text: 'text-teal-400', label: 'PHASE 1 — 1 mes' },
  2: { bg: 'bg-purple-500/10', border: 'border-purple-500/30', text: 'text-purple-400', label: 'PHASE 2 — 1 trimestre' },
  3: { bg: 'bg-slate-500/10', border: 'border-slate-500/30', text: 'text-slate-400', label: 'PHASE 3 — posterior' },
};

export function Roadmap() {
  return (
    <div className="space-y-6">
      {/* Timeline header */}
      <div className="grid grid-cols-3 gap-3">
        {([1, 2, 3] as const).map((phase) => {
          const pc = PHASE_COLORS[phase];
          const items = ROADMAP_ITEMS.filter((i) => i.phase === phase);
          const activeCount = items.filter((i) => i.status === 'active').length;
          return (
            <div key={phase} className={`rounded-lg border p-3 ${pc.bg} ${pc.border}`}>
              <div className={`text-xs font-mono font-semibold ${pc.text} uppercase tracking-wider`}>{pc.label}</div>
              <div className="text-xs text-muted-foreground mt-1">
                {items.length} items · {activeCount} active
              </div>
            </div>
          );
        })}
      </div>

      {/* Items */}
      <div className="space-y-3">
        {ROADMAP_ITEMS.map((item, index) => {
          const effortColor = EFFORT_COLORS[item.effort];
          const riskColor = RISK_COLORS[item.risk];
          const statusStyle = STATUS_STYLES[item.status];
          const phaseColor = PHASE_COLORS[item.phase];

          return (
            <motion.div
              key={item.id}
              initial={{ opacity: 0, y: 10 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.3, delay: index * 0.05 }}
              className={`rounded-xl border border-border/50 bg-surface/30 overflow-hidden ${
                item.status === 'active' ? 'ring-1 ring-emerald-500/20' : ''
              }`}
            >
              <div className="p-4">
                {/* Header */}
                <div className="flex items-start justify-between gap-4 mb-2">
                  <div className="flex items-center gap-2">
                    <span className="text-sm font-mono font-bold text-muted-foreground">#{item.id}</span>
                    <h3 className="text-sm font-semibold text-slate-200">{item.title}</h3>
                  </div>
                  <div className="flex items-center gap-2 shrink-0">
                    <span className={`text-[10px] font-mono px-2 py-0.5 rounded border ${statusStyle.color}`}>
                      {statusStyle.label}
                    </span>
                    <span className={`text-[10px] font-mono px-2 py-0.5 rounded border ${phaseColor.text} ${phaseColor.bg} ${phaseColor.border}`}>
                      P{item.phase}
                    </span>
                  </div>
                </div>

                {/* Description */}
                <p className="text-xs text-muted-foreground mb-3">{item.description}</p>

                {/* Meta row */}
                <div className="flex flex-wrap items-center gap-3 text-[10px] font-mono">
                  <span className="text-slate-500">Source:</span>
                  <span className="text-slate-300">{item.source}</span>

                  <span className="text-slate-500 ml-2">Effort:</span>
                  <span className={`px-1.5 py-0.5 rounded border ${effortColor}`}>{item.effort}</span>

                  <span className="text-slate-500 ml-2">Risk:</span>
                  <span className={riskColor}>{item.risk}</span>
                </div>

                {/* Impact */}
                <div className="mt-2 text-xs text-muted-foreground italic">
                  ↳ {item.impact}
                </div>

                {/* Dependencies */}
                {item.deps && (
                  <div className="mt-1 text-[10px] text-slate-500 font-mono">
                    Depends on: {item.deps.join(', ')}
                  </div>
                )}
              </div>
            </motion.div>
          );
        })}
      </div>

      {/* Summary */}
      <div className="rounded-lg border border-teal-500/20 bg-teal-500/5 p-4">
        <h4 className="text-xs font-mono font-semibold text-teal-400 mb-2 uppercase tracking-wider">Estado Real (Jul 2026)</h4>
        <div className="text-xs text-muted-foreground space-y-1">
          <p>• <strong className="text-emerald-400">5 activos</strong>: FSRS · Freshness · Dashboard M26 · DreamCycle · Mem0 Benchmark — todos implementados y verificados.</p>
          <p>• <strong className="text-amber-400">2 propuestos</strong>: Local Tool-Calling (26M-1B) · HippoRAG-PPR — requieren desarrollo nuevo.</p>
          <p>• <strong className="text-slate-400">1 futuro</strong>: KNEXA-FL LinUCB — requiere madurez del mesh multi-peer.</p>
          <p>• <strong className="text-slate-300">383 tests</strong> · 310 kernel + 61 tylluan-link + 12 FSRS · 0 cloud dependencies.</p>
        </div>
      </div>

      {/* Critical path visualization */}
      <div className="rounded-xl border border-border/50 bg-[#0A0F1A] p-4">
        <h3 className="text-sm font-mono font-semibold text-slate-300 mb-3">Current State — Activos vs Pendientes</h3>
        <svg viewBox="0 0 1000 250" className="w-full h-auto">
          <defs>
            <marker id="rm-arrow" viewBox="0 0 10 7" refX="10" refY="3.5" markerWidth="6" markerHeight="4" orient="auto">
              <polygon points="0 0, 10 3.5, 0 7" fill="#64748B" />
            </marker>
          </defs>

          {/* ACTIVE items: 1, 2, 3, 6, 8 */}
          {[1, 2, 3].map((id, i) => {
            const x = 60 + i * 230;
            return (
              <g key={id}>
                <rect x={x} y={20} width={200} height={38} rx={6}
                  fill="#0C1A1A" stroke="#10B981" strokeWidth={1.5}
                />
                <text x={x + 100} y={38} textAnchor="middle" fill="#10B981" fontSize="10" fontFamily="ui-monospace, monospace" fontWeight={600}>
                  #{id} {ROADMAP_ITEMS[id - 1].title.split(' ').slice(0, 2).join(' ')}
                </text>
                <text x={x + 100} y={51} textAnchor="middle" fill="#64748B" fontSize="8" fontFamily="ui-monospace, monospace">
                  ACTIVE
                </text>
              </g>
            );
          })}

          <text x={60} y={85} fill="#10B981" fontSize="8" fontFamily="ui-monospace, monospace">
            Phase 1: 3/3 activos
          </text>

          {/* Phase 2: items 5, 6, 7 */}
          {[5, 6, 7].map((id, i) => {
            const x = 100 + i * 260;
            const isActive = id === 6;
            return (
              <g key={id}>
                <rect x={x} y={100} width={230} height={38} rx={6}
                  fill={isActive ? '#0C1A1A' : '#111827'}
                  stroke={isActive ? '#10B981' : '#334155'}
                  strokeWidth={isActive ? 1.5 : 0.8}
                />
                <text x={x + 115} y={118} textAnchor="middle" fill={isActive ? '#10B981' : '#A855F7'} fontSize="10" fontFamily="ui-monospace, monospace" fontWeight={600}>
                  #{id} {ROADMAP_ITEMS[id - 1].title.split(' ').slice(0, 3).join(' ')}
                </text>
                <text x={x + 115} y={132} textAnchor="middle" fill="#64748B" fontSize="8" fontFamily="ui-monospace, monospace">
                  {isActive ? 'ACTIVE' : (ROADMAP_ITEMS[id - 1].status === 'future' ? 'FUTURE' : 'PROPOSED')}
                </text>
              </g>
            );
          })}

          <text x={60} y={163} fill={ROADMAP_ITEMS[5].status === 'active' ? '#10B981' : '#64748B'} fontSize="8" fontFamily="ui-monospace, monospace">
            Phase 2: 1/3 activo (DreamCycle) · 2 pendientes
          </text>

          {/* Phase 3: item 8 */}
          <g>
            <rect x={300} y={178} width={400} height={38} rx={6}
              fill="#0C1A1A" stroke="#10B981" strokeWidth={1.5}
            />
            <text x={500} y={196} textAnchor="middle" fill="#10B981" fontSize="10" fontFamily="ui-monospace, monospace" fontWeight={600}>
              #8 Mem0 Benchmark + Taxonomy
            </text>
            <text x={500} y={210} textAnchor="middle" fill="#64748B" fontSize="8" fontFamily="ui-monospace, monospace">
              ACTIVE · M17/M15-P3 cerrados
            </text>
          </g>

          {/* Dependencies */}
          <path d="M 160 58 L 160 75 L 230 75 L 230 100" fill="none" stroke="#10B981" strokeWidth={0.8} strokeDasharray="3 2" opacity={0.5} />
          <path d="M 160 58 L 160 75 L 410 75 L 410 100" fill="none" stroke="#10B981" strokeWidth={0.8} strokeDasharray="3 2" opacity={0.5} />
        </svg>
      </div>
    </div>
  );
}