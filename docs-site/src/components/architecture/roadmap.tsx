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
    description: 'Resolver conflictos de federación con reglas deterministas (SH-conflict + CAR), sin LLM',
    source: '"Don\'t Ask the LLM to Track Freshness" (Reddy & Challaram)',
    effort: 'low',
    risk: 'low',
    phase: 1,
    status: 'proposed',
    impact: 'Federación semánticamente determinista, validado en LongMemEval',
  },
  {
    id: 3,
    title: 'Dashboard Redesign (3-pane)',
    description: 'Tres paneles: cerebro (grafo), actividad (timeline), mesh (topología). Eliminar ruido.',
    source: 'Informe de diagnóstico',
    effort: 'medium',
    risk: 'low',
    phase: 1,
    status: 'proposed',
    impact: 'De "genérico" a "ventana al cerebro del agente"',
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
    title: 'SleepCycle NREM Phase',
    description: 'Consolidación offline: deduplicar episodios → memorias semánticas (θ > 0.95)',
    source: 'SCM (Shinde, arXiv:2604.20943)',
    effort: 'medium-high',
    risk: 'medium',
    phase: 2,
    status: 'proposed',
    impact: '"La memoria se cura mientras duerme"',
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
    description: 'Re-benchmark contra Mem0 10-way comparison. Posicionar en la taxonomía de la encuesta.',
    source: 'Mem0 (ECAI 2025) + Survey (47 authors)',
    effort: 'medium',
    risk: 'minimal',
    phase: 3,
    status: 'future',
    impact: 'Proyecto citable y comparable en la literatura',
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
        <h4 className="text-xs font-mono font-semibold text-teal-400 mb-2 uppercase tracking-wider">Execution Summary</h4>
        <div className="text-xs text-muted-foreground space-y-1">
          <p>• Los <strong className="text-slate-300">3 primeros</strong> (FSRS + Freshness + Dashboard) son alcanzables en <strong className="text-slate-300">1 mes</strong> de trabajo concentrado.</p>
          <p>• Los <strong className="text-slate-300">5 siguientes</strong> (items 4-8), en un <strong className="text-slate-300">trimestre</strong>.</p>
          <p>• <strong className="text-slate-300">Ninguno</strong> requiere cambiar la filosofía del proyecto — todos la refuerzan.</p>
          <p>• Ninguno toca federación ni guilds en el caso de FSRS (cambio local y aislado).</p>
        </div>
      </div>

      {/* Critical path visualization */}
      <div className="rounded-xl border border-border/50 bg-[#0A0F1A] p-4">
        <h3 className="text-sm font-mono font-semibold text-slate-300 mb-3">Dependency Graph</h3>
        <svg viewBox="0 0 1000 250" className="w-full h-auto">
          {/* Phase 1 items */}
          {[1, 2, 3, 4].map((id, i) => {
            const x = 60 + i * 230;
            const isActive = id === 1;
            return (
              <g key={id}>
                <rect x={x} y={30} width={200} height={44} rx={6}
                  fill={isActive ? '#0C1A1A' : '#111827'}
                  stroke={isActive ? '#10B981' : '#334155'}
                  strokeWidth={isActive ? 1.5 : 0.8}
                />
                <text x={x + 100} y={48} textAnchor="middle" fill={isActive ? '#10B981' : '#94A3B8'} fontSize="10" fontFamily="ui-monospace, monospace" fontWeight={600}>
                  #{id} {ROADMAP_ITEMS[id - 1].title.split(' ').slice(0, 2).join(' ')}
                </text>
                <text x={x + 100} y={64} textAnchor="middle" fill="#64748B" fontSize="8" fontFamily="ui-monospace, monospace">
                  {ROADMAP_ITEMS[id - 1].effort} · {ROADMAP_ITEMS[id - 1].risk} risk
                </text>
              </g>
            );
          })}

          {/* Phase 1 → Phase 2 arrows */}
          <text x={500} y={105} textAnchor="middle" fill="#14B8A6" fontSize="9" fontFamily="ui-monospace, monospace">
            Phase 1 complete →
          </text>

          {/* Phase 2 items */}
          {[5, 6, 7].map((id, i) => {
            const x = 100 + i * 300;
            return (
              <g key={id}>
                <rect x={x} y={115} width={260} height={44} rx={6}
                  fill="#111827"
                  stroke="#334155"
                  strokeWidth={0.8}
                />
                <text x={x + 130} y={133} textAnchor="middle" fill="#A855F7" fontSize="10" fontFamily="ui-monospace, monospace" fontWeight={600}>
                  #{id} {ROADMAP_ITEMS[id - 1].title.split(' ').slice(0, 3).join(' ')}
                </text>
                <text x={x + 130} y={149} textAnchor="middle" fill="#64748B" fontSize="8" fontFamily="ui-monospace, monospace">
                  {ROADMAP_ITEMS[id - 1].effort} · {ROADMAP_ITEMS[id - 1].risk} risk · deps: {ROADMAP_ITEMS[id - 1].deps?.length || 0}
                </text>
              </g>
            );
          })}

          {/* Dependency arrows */}
          {/* FSRS (#1) → HippoRAG (#5) */}
          <path d="M 160 74 L 160 90 L 230 90 L 230 115" fill="none" stroke="#10B981" strokeWidth={0.8} strokeDasharray="3 2" markerEnd="url(#disp-arrow)" opacity={0.5} />
          {/* FSRS (#1) → SleepCycle (#6) */}
          <path d="M 160 74 L 160 85 L 410 85 L 410 115" fill="none" stroke="#10B981" strokeWidth={0.8} strokeDasharray="3 2" markerEnd="url(#disp-arrow)" opacity={0.5} />

          {/* Phase 2 → Phase 3 */}
          <text x={500} y={195} textAnchor="middle" fill="#64748B" fontSize="9" fontFamily="ui-monospace, monospace">
            Phase 2 complete →
          </text>

          {/* Phase 3 */}
          <g>
            <rect x={300} y={205} width={400} height={36} rx={6} fill="#111827" stroke="#334155" strokeWidth={0.8} />
            <text x={500} y={224} textAnchor="middle" fill="#64748B" fontSize="10" fontFamily="ui-monospace, monospace" fontWeight={600}>
              #8 Mem0 Benchmark + Taxonomy
            </text>
            <text x={500} y={236} textAnchor="middle" fill="#475569" fontSize="8" fontFamily="ui-monospace, monospace">
              medium · minimal risk
            </text>
          </g>

          {/* Arrow markers */}
          <defs>
            <marker id="roadmap-arrow" viewBox="0 0 10 7" refX="10" refY="3.5" markerWidth="6" markerHeight="4" orient="auto">
              <polygon points="0 0, 10 3.5, 0 7" fill="#64748B" />
            </marker>
          </defs>
        </svg>
      </div>
    </div>
  );
}