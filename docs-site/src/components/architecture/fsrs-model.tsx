'use client';

import { motion } from 'framer-motion';
import { ArrowMarker, GlowFilter, SvgNode, Connection, PhaseBox, NODE_STYLES } from './shared';

export function FsrsModel() {
  return (
    <div className="space-y-4">
      {/* Formula highlight */}
      <div className="rounded-lg border border-emerald-500/30 bg-emerald-500/5 p-4">
        <div className="flex items-center gap-3 mb-2">
          <span className="text-xs font-mono font-semibold text-emerald-400 uppercase tracking-wider">Core Formula</span>
          <span className="text-xs font-mono text-muted-foreground">— replaces fixed T½ = 14d</span>
        </div>
        <div className="font-mono text-lg text-emerald-300 tracking-wide">
          R(t) = 2<sup className="text-emerald-400">-(Δt / S)</sup>
        </div>
        <div className="flex flex-wrap gap-4 mt-2 text-xs text-muted-foreground">
          <span><strong className="text-slate-300">R</strong> = retrievability [0, 1]</span>
          <span><strong className="text-slate-300">Δt</strong> = time since last review (days)</span>
          <span><strong className="text-slate-300">S</strong> = stability (days, per-memory)</span>
        </div>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        {/* LEFT: Review Cycle Flow */}
        <motion.div
          initial={{ opacity: 0, x: -20 }}
          animate={{ opacity: 1, x: 0 }}
          transition={{ duration: 0.5 }}
          className="rounded-xl border border-border/50 bg-[#0A0F1A] p-4"
        >
          <h3 className="text-sm font-mono font-semibold text-slate-300 mb-3">Ciclo de Review FSRS</h3>
          <svg viewBox="0 0 560 620" className="w-full h-auto">
            <ArrowMarker id="fsrs-arrow" color="#14B8A6" />
            <ArrowMarker id="fsrs-arrow-green" color="#10B981" />
            <ArrowMarker id="fsrs-arrow-amber" color="#F59E0B" />
            <GlowFilter id="fsrs-glow" color="#10B981" />

            {/* Step 1: Memory Creation */}
            <PhaseBox x={20} y={10} width={520} height={80} title="1. Memory Creation" color="#3B82F6">
              <SvgNode x={40} y={48} width={120} height={30} label="tylluan_remember" sublabel="guild call" style={NODE_STYLES.process} />
                <text x={200} y={58} fill="#94A3B8" fontSize="10" fontFamily="ui-monospace, monospace">
                  S₀ = 14d
                </text>
                <text x={200} y={72} fill="#94A3B8" fontSize="10" fontFamily="ui-monospace, monospace">
                  D₀ = 0.3, R computed
                </text>
              <text x={360} y={58} fill="#64748B" fontSize="9" fontFamily="ui-monospace, monospace">
                (migration default)
              </text>
            </PhaseBox>

            {/* Step 2: Time Passes */}
            <PhaseBox x={20} y={110} width={520} height={100} title="2. Passive Decay (apply_decay)" color="#F59E0B">
              {/* Decay visualization */}
              <g>
                {/* Time arrow */}
                <line x1={60} y1={170} x2={500} y2={170} stroke="#334155" strokeWidth={1} />
                <text x={280} y={162} textAnchor="middle" fill="#F59E0B" fontSize="10" fontFamily="ui-monospace, monospace">
                  R decays as 2^(-Δt/S)
                </text>

                {/* Memory nodes at different retrievability */}
                <circle cx={80} cy={170} r={14} fill="#10B981" opacity={0.9} />
                <text x={80} y={174} textAnchor="middle" fill="white" fontSize="8" fontFamily="ui-monospace">R=.95</text>

                <circle cx={180} cy={170} r={12} fill="#10B981" opacity={0.7} />
                <text x={180} y={174} textAnchor="middle" fill="white" fontSize="8" fontFamily="ui-monospace">R=.85</text>

                <circle cx={280} cy={170} r={10} fill="#F59E0B" opacity={0.7} />
                <text x={280} y={174} textAnchor="middle" fill="white" fontSize="8" fontFamily="ui-monospace">R=.60</text>

                <circle cx={380} cy={170} r={8} fill="#F59E0B" opacity={0.5} />
                <text x={380} y={174} textAnchor="middle" fill="white" fontSize="7" fontFamily="ui-monospace">R=.35</text>

                <circle cx={470} cy={170} r={6} fill="#EF4444" opacity={0.5} />
                <text x={470} y={174} textAnchor="middle" fill="white" fontSize="7" fontFamily="ui-monospace">R=.10</text>

                <text x={60} y={198} fill="#64748B" fontSize="8" fontFamily="ui-monospace">t₀</text>
                <text x={460} y={198} fill="#64748B" fontSize="8" fontFamily="ui-monospace">t₀ + S</text>
              </g>
            </PhaseBox>

            {/* Step 3: Access triggers review */}
            <PhaseBox x={20} y={230} width={520} height={90} title="3. Access → touch_node" color="#10B981">
              <SvgNode x={40} y={268} width={120} height={30} label="tylluan_recall" sublabel="query triggers" style={{ fill: '#0A1A15', stroke: '#10B981', strokeWidth: 1.5 }} />
              <text x={200} y={278} fill="#10B981" fontSize="11" fontFamily="ui-monospace, monospace" fontWeight={600}>
                → FsrsItem::review(Rating::Good)
              </text>
              <text x={200} y={296} fill="#94A3B8" fontSize="10" fontFamily="ui-monospace, monospace">
                S_new = S × (1 + e^(b × (1-D) - a))
              </text>
            </PhaseBox>

            {/* Step 4: Stability increases */}
            <PhaseBox x={20} y={340} width={520} height={110} title="4. Stability Update" color="#14B8A6">
              <g>
                {/* Before/After comparison */}
                <text x={80} y={376} textAnchor="middle" fill="#F59E0B" fontSize="10" fontFamily="ui-monospace, monospace">Before</text>
                <rect x={40} y={385} width={80} height={12} rx={3} fill="#F59E0B" opacity={0.3} />
                <rect x={40} y={385} width={40} height={12} rx={3} fill="#F59E0B" opacity={0.6} />
                <text x={80} y={394} textAnchor="middle" fill="white" fontSize="7" fontFamily="ui-monospace">S=14d</text>

                {/* Arrow */}
                <text x={170} y={394} textAnchor="middle" fill="#10B981" fontSize="16">→</text>
                <text x={170} y={376} textAnchor="middle" fill="#10B981" fontSize="8" fontFamily="ui-monospace">review</text>

                <text x={280} y={376} textAnchor="middle" fill="#10B981" fontSize="10" fontFamily="ui-monospace, monospace">After</text>
                <rect x={200} y={385} width={160} height={12} rx={3} fill="#10B981" opacity={0.3} />
                <rect x={200} y={385} width={100} height={12} rx={3} fill="#10B981" opacity={0.6} />
                <text x={280} y={394} textAnchor="middle" fill="white" fontSize="7" fontFamily="ui-monospace">S=28d</text>

                {/* Formula details */}
                <text x={440} y={376} fill="#64748B" fontSize="9" fontFamily="ui-monospace, monospace">
                  R resets to 1.0
                </text>
                <text x={440} y={392} fill="#64748B" fontSize="9" fontFamily="ui-monospace, monospace">
                  D adjusts by
                </text>
                <text x={440} y={406} fill="#64748B" fontSize="9" fontFamily="ui-monospace, monospace">
                  rating quality
                </text>
              </g>

              {/* Cycle arrow */}
              <path d="M 480 430 C 520 430, 530 280, 520 160 C 510 100, 540 50, 540 50" fill="none" stroke="#14B8A6" strokeWidth={1} strokeDasharray="4 2" markerEnd="url(#fsrs-arrow)" opacity={0.5} />
              <text x={530} y={240} fill="#14B8A6" fontSize="8" fontFamily="ui-monospace, monospace" transform="rotate(90, 530, 240)" textAnchor="middle">
                cycle repeats
              </text>
            </PhaseBox>

            {/* Step 5: Weight in retrieval */}
            <PhaseBox x={20} y={470} width={520} height={70} title="5. Retrieval Weight" color="#A855F7">
              <text x={40} y={508} fill="#A855F7" fontSize="11" fontFamily="ui-monospace, monospace">
                weight = R(node) × RRF_score
              </text>
              <text x={40} y={526} fill="#94A3B8" fontSize="10" fontFamily="system-ui, sans-serif">
                Low-retrievability memories sink in ranked results.
              </text>
            </PhaseBox>

            {/* Step 6: Decay_node for individual nodes */}
            <PhaseBox x={20} y={560} width={520} height={50} title="6. decay_node (single)" color="#64748B">
              <text x={40} y={590} fill="#94A3B8" fontSize="10" fontFamily="ui-monospace, monospace">
                Same formula R = 2^(-Δt/S)  ·  No more hardcoded 14d  ·  Per-memory half-life
              </text>
            </PhaseBox>
          </svg>
        </motion.div>

        {/* RIGHT: Comparison and Data Model */}
        <div className="space-y-4">
          {/* Old vs New comparison */}
          <motion.div
            initial={{ opacity: 0, x: 20 }}
            animate={{ opacity: 1, x: 0 }}
            transition={{ duration: 0.5, delay: 0.2 }}
            className="rounded-xl border border-border/50 bg-[#0A0F1A] p-4"
          >
            <h3 className="text-sm font-mono font-semibold text-slate-300 mb-3">Antes / Después</h3>
            <div className="space-y-3">
              {/* Old */}
              <div className="rounded-lg border border-red-500/20 bg-red-500/5 p-3">
                <div className="text-xs font-mono font-semibold text-red-400 mb-2">❌ Antes — Fixed Half-Life</div>
                <div className="font-mono text-sm text-red-300/80 mb-2">
                  weight = 2<sup>-(days / 14)</sup>
                </div>
                <div className="text-xs text-muted-foreground space-y-1">
                  <p>• Todas las memorias decaen a la misma tasa</p>
                  <p>• Una memoria consultada 1000× decae igual que una jamás accedida</p>
                  <p>• T½ = 14d es un número mágico sin base empírica</p>
                  <p>• <code className="text-red-400">salience_decay</code> = campo escalar</p>
                </div>
              </div>

              {/* New */}
              <div className="rounded-lg border border-emerald-500/20 bg-emerald-500/5 p-3">
                <div className="text-xs font-mono font-semibold text-emerald-400 mb-2">✓ Después — Per-Memory FSRS</div>
                <div className="font-mono text-sm text-emerald-300/80 mb-2">
                  weight = 2<sup>-(Δt / S<sub>i</sub>)</sup>
                </div>
                <div className="text-xs text-muted-foreground space-y-1">
                  <p>• Cada memoria tiene su propia curva de olvido</p>
                  <p>• El uso reforza la estabilidad (review → S aumenta)</p>
                  <p>• S calibrado por cómo el usuario accede la memoria</p>
                  <p>• <code className="text-emerald-400">stability · difficulty · retrievability</code></p>
                </div>
              </div>
            </div>
          </motion.div>

          {/* Database schema change */}
          <motion.div
            initial={{ opacity: 0, x: 20 }}
            animate={{ opacity: 1, x: 0 }}
            transition={{ duration: 0.5, delay: 0.3 }}
            className="rounded-xl border border-border/50 bg-[#0A0F1A] p-4"
          >
            <h3 className="text-sm font-mono font-semibold text-slate-300 mb-3">Schema Migration</h3>
            <div className="font-mono text-xs space-y-1">
              <div className="text-red-400/70">
                <span className="text-red-400">-</span> salience_decay REAL DEFAULT 14.0
              </div>
              <div className="text-emerald-400/80">
                <span className="text-emerald-400">+</span> stability REAL NOT NULL DEFAULT 14.0
              </div>
              <div className="text-emerald-400/80">
                <span className="text-emerald-400">+</span> difficulty REAL NOT NULL DEFAULT 0.5
              </div>
              <div className="text-amber-400/80">
                <span className="text-amber-400">~</span> retrievability (COMPUTED: 2^(-Δt / stability), no almacenado)
              </div>
              <div className="text-emerald-400/80">
                <span className="text-emerald-400">+</span> fsrs_last_review INTEGER DEFAULT 0
              </div>
            </div>
            <div className="mt-3 pt-3 border-t border-border/50 text-xs text-muted-foreground">
              <p>Backfill: memorias existentes inicializan con <code className="text-emerald-400">S=14, D=0.5, R=1.0</code> → comportamiento idéntico al anterior hasta primer acceso.</p>
            </div>
          </motion.div>

          {/* Key metrics */}
          <motion.div
            initial={{ opacity: 0, x: 20 }}
            animate={{ opacity: 1, x: 0 }}
            transition={{ duration: 0.5, delay: 0.4 }}
            className="rounded-xl border border-border/50 bg-[#0A0F1A] p-4"
          >
            <h3 className="text-sm font-mono font-semibold text-slate-300 mb-3">Métricas del Modelo</h3>
            <div className="grid grid-cols-3 gap-3">
              <div className="text-center p-2 rounded-lg bg-surface/50">
                <div className="text-lg font-mono font-bold text-teal-400">3</div>
                <div className="text-[10px] text-muted-foreground">variables por memoria</div>
              </div>
              <div className="text-center p-2 rounded-lg bg-surface/50">
                <div className="text-lg font-mono font-bold text-teal-400">~200</div>
                <div className="text-[10px] text-muted-foreground">líneas de Rust (algo)</div>
              </div>
              <div className="text-center p-2 rounded-lg bg-surface/50">
                <div className="text-lg font-mono font-bold text-teal-400">305</div>
                <div className="text-[10px] text-muted-foreground">tests passing</div>
              </div>
            </div>
            <div className="mt-3 space-y-1.5 text-xs text-muted-foreground">
              <div className="flex justify-between">
                <span>Entrenado sobre</span>
                <span className="font-mono text-slate-400">~220M revisiones (Anki)</span>
              </div>
              <div className="flex justify-between">
                <span>Mejora retención 90d</span>
                <span className="font-mono text-emerald-500/80">~30% menos revisiones vs SM-2</span>
              </div>
              <div className="flex justify-between">
                <span>Riesgo de migración</span>
                <span className="font-mono text-emerald-500/80">Mínimo (zero-regression)</span>
              </div>
              <div className="flex justify-between">
                <span>Compatibilidad</span>
                <span className="font-mono text-emerald-500/80">portable · clinic · server</span>
              </div>
            </div>
          </motion.div>
        </div>
      </div>

      {/* Decay curves visualization */}
      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.5, delay: 0.5 }}
        className="rounded-xl border border-border/50 bg-[#0A0F1A] p-4"
      >
        <h3 className="text-sm font-mono font-semibold text-slate-300 mb-3">Curvas de Decaimiento por Perfil de Memoria</h3>
        <svg viewBox="0 0 800 220" className="w-full h-auto">
          {/* Grid */}
          {[0, 50, 100, 150, 200].map((v, i) => (
            <g key={i}>
              <line x1={80} y1={20 + (200 - v)} x2={780} y2={20 + (200 - v)} stroke="#1E293B" strokeWidth={0.5} />
              <text x={70} y={24 + (200 - v)} textAnchor="end" fill="#475569" fontSize="9" fontFamily="ui-monospace">{v}%</text>
            </g>
          ))}
          {[0, 7, 14, 28, 56, 90].map((d, i) => {
            const x = 80 + (d / 90) * 700;
            return (
              <g key={i}>
                <line x1={x} y1={20} x2={x} y2={220} stroke="#1E293B" strokeWidth={0.5} />
                <text x={x} y={236} textAnchor="middle" fill="#475569" fontSize="9" fontFamily="ui-monospace">{d}d</text>
              </g>
            );
          })}

          {/* Y axis label */}
          <text x={15} y={120} textAnchor="middle" fill="#64748B" fontSize="9" fontFamily="ui-monospace" transform="rotate(-90, 15, 120)">
            Retrievability (R)
          </text>
          <text x={430} y={255} textAnchor="middle" fill="#64748B" fontSize="9" fontFamily="ui-monospace">
            Días desde último review (Δt)
          </text>

          {/* Old fixed half-life (T½=14d) */}
          <path
            d={Array.from({ length: 91 }, (_, d) => {
              const r = Math.pow(2, -d / 14);
              const x = 80 + (d / 90) * 700;
              const y = 220 - r * 200;
              return `${d === 0 ? 'M' : 'L'} ${x} ${y}`;
            }).join(' ')}
            fill="none"
            stroke="#EF4444"
            strokeWidth={1.5}
            opacity={0.6}
            strokeDasharray="4 2"
          />

          {/* Stable memory (S=50d) */}
          <path
            d={Array.from({ length: 91 }, (_, d) => {
              const r = Math.pow(2, -d / 50);
              const x = 80 + (d / 90) * 700;
              const y = 220 - r * 200;
              return `${d === 0 ? 'M' : 'L'} ${x} ${y}`;
            }).join(' ')}
            fill="none"
            stroke="#10B981"
            strokeWidth={2}
          />

          {/* Average memory (S=14d, default) */}
          <path
            d={Array.from({ length: 91 }, (_, d) => {
              const r = Math.pow(2, -d / 14);
              const x = 80 + (d / 90) * 700;
              const y = 220 - r * 200;
              return `${d === 0 ? 'M' : 'L'} ${x} ${y}`;
            }).join(' ')}
            fill="none"
            stroke="#14B8A6"
            strokeWidth={2}
          />

          {/* Fragile memory (S=5d) */}
          <path
            d={Array.from({ length: 91 }, (_, d) => {
              const r = Math.pow(2, -d / 5);
              const x = 80 + (d / 90) * 700;
              const y = 220 - r * 200;
              return `${d === 0 ? 'M' : 'L'} ${x} ${y}`;
            }).join(' ')}
            fill="none"
            stroke="#F59E0B"
            strokeWidth={2}
          />

          {/* Critical memory (S=2d) */}
          <path
            d={Array.from({ length: 91 }, (_, d) => {
              const r = Math.pow(2, -d / 2);
              const x = 80 + (d / 90) * 700;
              const y = 220 - r * 200;
              return `${d === 0 ? 'M' : 'L'} ${x} ${y}`;
            }).join(' ')}
            fill="none"
            stroke="#EF4444"
            strokeWidth={2}
          />

          {/* Legend */}
          <g transform="translate(580, 30)">
            <rect x={0} y={0} width={190} height={110} rx={4} fill="#0A0F1A" stroke="#1E293B" strokeWidth={0.5} />
            <line x1={10} y1={18} x2={30} y2={18} stroke="#10B981" strokeWidth={2} />
            <text x={36} y={22} fill="#94A3B8" fontSize="9" fontFamily="ui-monospace">S=50d (muy estable)</text>

            <line x1={10} y1={36} x2={30} y2={36} stroke="#14B8A6" strokeWidth={2} />
            <text x={36} y={40} fill="#94A3B8" fontSize="9" fontFamily="ui-monospace">S=14d (default)</text>

            <line x1={10} y1={54} x2={30} y2={54} stroke="#F59E0B" strokeWidth={2} />
            <text x={36} y={58} fill="#94A3B8" fontSize="9" fontFamily="ui-monospace">S=5d (frágil)</text>

            <line x1={10} y1={72} x2={30} y2={72} stroke="#EF4444" strokeWidth={2} />
            <text x={36} y={76} fill="#94A3B8" fontSize="9" fontFamily="ui-monospace">S=2d (crítica)</text>

            <line x1={10} y1={90} x2={30} y2={90} stroke="#EF4444" strokeWidth={1.5} strokeDasharray="4 2" opacity={0.6} />
            <text x={36} y={94} fill="#94A3B8" fontSize="9" fontFamily="ui-monospace">Antes: T½=14d fijo</text>
          </g>
        </svg>
      </motion.div>
    </div>
  );
}