'use client';

import { motion } from 'framer-motion';
import { ArrowMarker, SvgNode, Connection, PhaseBox, SectionLabel, NODE_STYLES } from './shared';

export function SleepCycle() {
  const styles = {
    fsrsNode: { fill: '#0A1A15' as const, stroke: '#10B981' as const, strokeWidth: 1 },
    processNode: { fill: '#1A1010' as const, stroke: '#F59E0B' as const, strokeWidth: 1 },
    redNode: { fill: '#1A1010' as const, stroke: '#EF4444' as const, strokeWidth: 1 },
  };

  return (
    <div className="space-y-4">
      {/* Status banner */}
      <div className="rounded-lg border border-emerald-500/30 bg-emerald-500/5 p-3 flex items-start gap-3">
        <span className="text-emerald-400 text-lg">🌙</span>
        <div>
          <div className="text-xs font-mono font-semibold text-emerald-400 uppercase tracking-wider">
            Active — NightConsolidation + DreamCycle
          </div>
          <div className="text-xs text-muted-foreground mt-1">
            DreamCycle corre cada hora en NightConsolidation: dedup, decay por saliencia, detecci&#243;n de contradicciones.
            ConsensusEngine resuelve conflictos cognitivos. consolidate_episodes fusiona episodios similares.
          </div>
        </div>
      </div>

      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: 0.6 }}
        className="w-full overflow-x-auto rounded-xl border border-border/50 bg-[#0A0F1A] p-4"
      >
        <svg viewBox="0 0 1100 820" className="w-full h-auto min-w-[800px]">
          <ArrowMarker id="sleep-arrow" color="#14B8A6" />
          <ArrowMarker id="sleep-arrow-amber" color="#F59E0B" />
          <ArrowMarker id="sleep-arrow-red" color="#EF4444" />
          <ArrowMarker id="sleep-arrow-gray" color="#64748B" />
          <ArrowMarker id="sleep-arrow-blue" color="#3B82F6" />

          {/* TRIGGER */}
          <SectionLabel x={30} y={30} text="TRIGGER" color="#94A3B8" />

          <SvgNode x={60} y={18} width={180} height={34} label="Manual" sublabel="tylluan-cli sleep" style={NODE_STYLES.subsystem} />
          <SvgNode x={270} y={18} width={180} height={34} label="Scheduled" sublabel="cron / idle detect" style={NODE_STYLES.subsystem} />
          <SvgNode x={480} y={18} width={180} height={34} label="Threshold" sublabel="N new episodic mems" style={NODE_STYLES.subsystem} />

          <line x1={240} y1={35} x2={270} y2={35} stroke="#64748B" strokeWidth={1} />
          <line x1={450} y1={35} x2={480} y2={35} stroke="#64748B" strokeWidth={1} />
          <line x1={570} y1={52} x2={570} y2={70} stroke="#64748B" strokeWidth={1} markerEnd="url(#sleep-arrow-gray)" />

          {/* NREM PHASE */}
          <PhaseBox x={40} y={70} width={1020} height={270} title="NREM Phase — Deduplication &amp; Compression" color="#14B8A6">
            <SectionLabel x={60} y={102} text="INPUT: Coloquio episodes" color="#3B82F6" />

            <g>
              {[0, 1, 2, 3, 4, 5, 6, 7].map((i) => (
                <g key={i}>
                  <rect
                    x={70 + i * 65}
                    y={112}
                    width={55}
                    height={26}
                    rx={4}
                    fill="#0F1520"
                    stroke="#3B82F6"
                    strokeWidth={0.8}
                    opacity={0.6 + Math.random() * 0.4}
                  />
                  <text
                    x={97 + i * 65}
                    y={128}
                    textAnchor="middle"
                    fill="#3B82F6"
                    fontSize="8"
                    fontFamily="ui-monospace, monospace"
                  >
                    {`ep_${i + 1}`}
                  </text>
                </g>
              ))}
            </g>

            <Connection x1={530} y1={138} x2={530} y2={155} markerId="sleep-arrow" label="batch embed" strokeWidth={0.8} />

            <PhaseBox x={100} y={155} width={420} height={100} title="Step 1: Embedding Comparison" color="#14B8A6">
              <SvgNode x={120} y={190} width={130} height={28} label="BGE-M3 Encode" sublabel="all episodes" style={NODE_STYLES.core} />
              <SvgNode x={270} y={190} width={130} height={28} label="Pairwise Cosine" sublabel="O(n^2) compare" style={NODE_STYLES.core} />
              <Connection x1={250} y1={204} x2={270} y2={204} markerId="sleep-arrow" strokeWidth={0.8} />

              <rect x={420} y={185} width={85} height={32} rx={4} fill="#0C1A1A" stroke="#F59E0B" strokeWidth={0.8} />
              <text x={462} y={198} textAnchor="middle" fill="#F59E0B" fontSize="9" fontFamily="ui-monospace, monospace">
                theta &gt; 0.95
              </text>
              <text x={462} y={210} textAnchor="middle" fill="#64748B" fontSize="8" fontFamily="ui-monospace, monospace">
                merge?
              </text>
            </PhaseBox>

            <PhaseBox x={540} y={155} width={500} height={100} title="Step 2: Semantic Clustering" color="#14B8A6">
              <g>
                <circle cx={580} cy={195} r={10} fill="#3B82F6" opacity={0.5} />
                <circle cx={610} cy={190} r={10} fill="#3B82F6" opacity={0.6} />
                <circle cx={600} cy={215} r={10} fill="#3B82F6" opacity={0.4} />
                <circle cx={630} cy={205} r={10} fill="#3B82F6" opacity={0.5} />
                <text x={605} y={240} textAnchor="middle" fill="#64748B" fontSize="8" fontFamily="ui-monospace">
                  episodic
                </text>
              </g>

              <text x={680} y={205} textAnchor="middle" fill="#10B981" fontSize="18" fontFamily="ui-monospace">
                &#x2192;
              </text>
              <text x={680} y={220} textAnchor="middle" fill="#64748B" fontSize="7" fontFamily="ui-monospace">
                cluster
              </text>

              <g>
                <rect x={720} y={180} width={60} height={40} rx={6} fill="#0C1A1A" stroke="#10B981" strokeWidth={1.5} />
                <text x={750} y={198} textAnchor="middle" fill="#10B981" fontSize="9" fontFamily="ui-monospace" fontWeight={600}>
                  sem_1
                </text>
                <text x={750} y={212} textAnchor="middle" fill="#64748B" fontSize="7" fontFamily="ui-monospace">
                  4 merged
                </text>
              </g>

              <text x={850} y={192} fill="#94A3B8" fontSize="9" fontFamily="ui-monospace, monospace">
                Output:
              </text>
              <text x={850} y={206} fill="#10B981" fontSize="9" fontFamily="ui-monospace, monospace">
                Semantic nodes
              </text>
              <text x={850} y={220} fill="#64748B" fontSize="8" fontFamily="ui-monospace, monospace">
                (deduplicated)
              </text>
              <text x={850} y={236} fill="#64748B" fontSize="8" fontFamily="ui-monospace, monospace">
                + graph edges
              </text>
            </PhaseBox>

            <Connection x1={530} y1={255} x2={530} y2={272} markerId="sleep-arrow" strokeWidth={0.8} />
            <PhaseBox x={300} y={272} width={460} height={50} title="Step 3: Write to SilvaDB" color="#10B981">
              <text x={530} y={300} textAnchor="middle" fill="#10B981" fontSize="10" fontFamily="ui-monospace, monospace">
                Episodes &#x2192; Semantic Nodes &#183; Duplicates removed &#183; Graph edges created
              </text>
            </PhaseBox>

            <rect x={60} y={280} width={220} height={42} rx={4} fill="#0A0F1A" stroke="#14B8A6" strokeWidth={0.5} />
            <text x={75} y={297} fill="#14B8A6" fontSize="9" fontFamily="ui-monospace, monospace" fontWeight={600}>
              NREM Benefit: ~90% of total
            </text>
            <text x={75} y={311} fill="#64748B" fontSize="8" fontFamily="ui-monospace, monospace">
              90.9% noise reduction &#183; &lt;1ms latency
            </text>
          </PhaseBox>

          {/* REM PHASE */}
          <PhaseBox x={40} y={360} width={1020} height={160} title="REM Phase — Reactivation &amp; Reinforcement" color="#A855F7">
            <PhaseBox x={60} y={395} width={300} height={105} title="FSRS-Guided Reactivation" color="#10B981">
              <SvgNode x={80} y={430} width={120} height={26} label="Scan: low R" sublabel="retrievability" style={styles.fsrsNode} />
              <SvgNode x={80} y={462} width={120} height={26} label="Filter: high S" sublabel="stability" style={styles.fsrsNode} />

              <text x={230} y={438} fill="#94A3B8" fontSize="9" fontFamily="ui-monospace, monospace">
                R &lt; 0.5 AND
              </text>
              <text x={230} y={454} fill="#94A3B8" fontSize="9" fontFamily="ui-monospace, monospace">
                S &gt; 30d &#x2192; valuable
              </text>
              <text x={230} y={470} fill="#64748B" fontSize="8" fontFamily="ui-monospace, monospace">
                but forgetting
              </text>
            </PhaseBox>

            <PhaseBox x={380} y={395} width={300} height={105} title="Reinforce via review()" color="#A855F7">
              <SvgNode x={400} y={430} width={130} height={26} label="review(Rating::Good)" sublabel="simulated access" style={NODE_STYLES.process} />
              <text x={560} y={438} fill="#94A3B8" fontSize="9" fontFamily="ui-monospace, monospace">
                S_new = S * growth_factor
              </text>
              <text x={560} y={454} fill="#94A3B8" fontSize="9" fontFamily="ui-monospace, monospace">
                R resets &#x2192; 1.0
              </text>
              <text x={560} y={470} fill="#64748B" fontSize="8" fontFamily="ui-monospace, monospace">
                No real query needed
              </text>
            </PhaseBox>

            <PhaseBox x={700} y={395} width={340} height={105} title="Result" color="#F59E0B">
              <text x={720} y={425} fill="#F59E0B" fontSize="10" fontFamily="ui-monospace, monospace">
                Weak-but-valuable memories
              </text>
              <text x={720} y={442} fill="#F59E0B" fontSize="10" fontFamily="ui-monospace, monospace">
                get reinforced without user
              </text>
              <text x={720} y={459} fill="#F59E0B" fontSize="10" fontFamily="ui-monospace, monospace">
                intervention. Like dream replay.
              </text>
              <text x={720} y={482} fill="#64748B" fontSize="8" fontFamily="ui-monospace, monospace">
                &quot;The brain replays while sleeping&quot;
              </text>
            </PhaseBox>

            <Connection x1={360} y1={447} x2={380} y2={447} markerId="sleep-arrow" strokeWidth={0.8} />
            <Connection x1={680} y1={447} x2={700} y2={447} markerId="sleep-arrow-amber" strokeWidth={0.8} />
          </PhaseBox>

          {/* VALUE-BASED FORGETTING */}
          <PhaseBox x={40} y={540} width={1020} height={120} title="Value-Based Forgetting" color="#EF4444">
            <SvgNode x={60} y={575} width={160} height={30} label="Score: value * R" sublabel="combined metric" style={styles.redNode} />
            <SvgNode x={250} y={575} width={130} height={30} label="Threshold Check" sublabel="below cutoff?" style={styles.redNode} />
            <SvgNode x={420} y={575} width={130} height={30} label="Tombstone" sublabel="N-day recoverable" style={styles.processNode} />
            <SvgNode x={580} y={575} width={130} height={30} label="Permanent Delete" sublabel="after N days" style={styles.redNode} />

            <Connection x1={220} y1={590} x2={250} y2={590} markerId="sleep-arrow-red" strokeWidth={0.8} />
            <Connection x1={380} y1={590} x2={420} y2={590} markerId="sleep-arrow-red" strokeWidth={0.8} />
            <Connection x1={550} y1={590} x2={580} y2={590} markerId="sleep-arrow-red" strokeWidth={0.8} />

            <text x={60} y={625} fill="#64748B" fontSize="9" fontFamily="ui-monospace, monospace">
              Safety: --dry-run shows what would be deleted &#183; Tombstones recoverable during grace period &#183; Frees space on Pi 4
            </text>
            <text x={60} y={642} fill="#64748B" fontSize="9" fontFamily="ui-monospace, monospace">
              Criteria: value_score LOW AND retrievability LOW AND no recent access &#x2192; candidate for forgetting
            </text>
          </PhaseBox>

          {/* SELF-MODEL */}
          <PhaseBox x={40} y={680} width={1020} height={70} title="Self-Model Introspection (SCM Component 5)" color="#64748B">
            <text x={60} y={712} fill="#94A3B8" fontSize="10" fontFamily="system-ui, sans-serif">
              After each sleep cycle, the system updates its self-model.
            </text>
            <text x={60} y={730} fill="#64748B" fontSize="9" fontFamily="ui-monospace, monospace">
              This metadata drives dashboard visualization (salience heatmap) and proactive re-exposure scheduling.
            </text>
          </PhaseBox>

          {/* PHASE TRANSITIONS */}
          <Connection x1={550} y1={340} x2={550} y2={360} markerId="sleep-arrow" label="NREM complete" strokeWidth={0.8} />
          <Connection x1={550} y1={520} x2={550} y2={540} markerId="sleep-arrow-amber" label="REM complete" strokeWidth={0.8} />
          <Connection x1={550} y1={660} x2={550} y2={680} markerId="sleep-arrow-gray" label="forgetting done" strokeWidth={0.8} />
        </svg>
      </motion.div>

      {/* Implementation notes */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <div className="rounded-lg border border-border/50 bg-surface/50 p-4">
          <h4 className="text-xs font-mono font-semibold text-teal-400 mb-2">Fase 1 recomendada</h4>
          <div className="text-xs text-muted-foreground space-y-1.5">
            <p>Solo <strong className="text-slate-300">NREM</strong> (deduplicaci&#243;n + fusi&#243;n). Da el ~90% del beneficio con menor riesgo.</p>
            <p>Comando: <code className="text-teal-400">tylluan-cli sleep --nrem-only</code></p>
            <p>Esfuerzo: medio-alto</p>
          </div>
        </div>
        <div className="rounded-lg border border-border/50 bg-surface/50 p-4">
          <h4 className="text-xs font-mono font-semibold text-purple-400 mb-2">Fase 2</h4>
          <div className="text-xs text-muted-foreground space-y-1.5">
            <p>A&#241;adir <strong className="text-slate-300">REM</strong> (reactivation FSRS). Requiere que FSRS lleve weeks en prod con datos reales.</p>
            <p>Depende de: FSRS maturity</p>
            <p>Esfuerzo: medio</p>
          </div>
        </div>
        <div className="rounded-lg border border-border/50 bg-surface/50 p-4">
          <h4 className="text-xs font-mono font-semibold text-red-400 mb-2">Fase 3</h4>
          <div className="text-xs text-muted-foreground space-y-1.5">
            <p><strong className="text-slate-300">Value-based forgetting</strong> con tombstones. El m&#225;s sensible — necesita --dry-run y grace period.</p>
            <p>Riesgo: informaci&#243;n irrecuperable si tombstone expira</p>
            <p>Esfuerzo: bajo (una vez que 1+2 existen)</p>
          </div>
        </div>
      </div>
    </div>
  );
}