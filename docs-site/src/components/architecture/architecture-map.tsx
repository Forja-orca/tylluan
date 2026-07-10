'use client';

import { motion } from 'framer-motion';
import { ArrowMarker, GlowFilter, SvgNode, Connection, SectionLabel, PhaseBox, NODE_STYLES, COLORS } from './shared';

export function ArchitectureMap() {
  return (
    <div className="space-y-4">
      {/* Legend */}
      <div className="flex flex-wrap gap-4 text-xs font-mono text-muted-foreground">
        <span className="flex items-center gap-2">
          <span className="w-3 h-3 rounded-sm border border-teal-500/60 bg-teal-500/10" /> Core
        </span>
        <span className="flex items-center gap-2">
          <span className="w-3 h-3 rounded-sm border border-slate-500 bg-slate-800" /> Subsystem
        </span>
        <span className="flex items-center gap-2">
          <span className="w-3 h-3 rounded-sm border border-purple-500/60 bg-purple-500/10" /> Process
        </span>
        <span className="flex items-center gap-2">
          <span className="w-3 h-3 rounded-sm border border-amber-500/60 bg-amber-500/10" /> Data
        </span>
        <span className="flex items-center gap-2">
          <span className="w-2 h-0.5 bg-teal-500" /> Data flow
        </span>
        <span className="flex items-center gap-2">
          <span className="w-2 h-0.5 bg-slate-500 border-dashed" style={{ borderTop: '1px dashed #64748B' }} /> Control
        </span>
        <span className="flex items-center gap-2">
          <span className="w-2 h-0.5 bg-amber-500" /> Sync
        </span>
      </div>

      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: 0.6 }}
        className="w-full overflow-x-auto rounded-xl border border-border/50 bg-[#0A0F1A] p-2"
      >
        <svg viewBox="0 0 1200 880" className="w-full h-auto min-w-[900px]" style={{ fontFamily: 'system-ui, -apple-system, sans-serif' }}>
          <ArrowMarker id="arrow-data" color="#14B8A6" />
          <ArrowMarker id="arrow-control" color="#64748B" />
          <ArrowMarker id="arrow-sync" color="#F59E0B" />
          <ArrowMarker id="arrow-purple" color="#A855F7" />
          <GlowFilter id="glow-teal" color="#14B8A6" />

          {/* ═══════ LAYER 1: INTERFACES ═══════ */}
          <SectionLabel x={30} y={38} text="INTERFACES" color="#94A3B8" />

          <SvgNode x={120} y={22} width={130} height={38} label="tylluan-cli" sublabel="REPL / commands" style={NODE_STYLES.subsystem} />
          <SvgNode x={290} y={22} width={150} height={38} label="REST API" sublabel="tylluan serve" style={NODE_STYLES.subsystem} />
          <SvgNode x={480} y={22} width={180} height={38} label="Dashboard" sublabel="React + Vite + Tailwind" style={NODE_STYLES.subsystem} />
          <SvgNode x={700} y={22} width={140} height={38} label="Coloquio UI" sublabel="chat interface" style={NODE_STYLES.subsystem} />

          {/* ═══════ LAYER 2: ORCHESTRATION ═══════ */}
          <SectionLabel x={30} y={108} text="ORCHESTRATION" color="#A855F7" />

          {/* Complexity Cascade box */}
          <PhaseBox x={100} y={88} width={300} height={100} title="Complexity Cascade" color="#A855F7">
            <SvgNode x={120} y={126} width={120} height={34} label="Intent Parser" sublabel="classify intent" style={NODE_STYLES.process} />
            <SvgNode x={260} y={126} width={120} height={34} label="Score Router" sublabel="complexity level" style={NODE_STYLES.process} />
          </PhaseBox>

          {/* Dispatch Router box */}
          <PhaseBox x={440} y={88} width={320} height={100} title="DispatchRouter" color="#A855F7">
            <SvgNode x={460} y={126} width={130} height={34} label="Peer Scoring" sublabel="load · latency · GPU" style={NODE_STYLES.process} />
            <SvgNode x={610} y={126} width={130} height={34} label="Circuit Breaker" sublabel="fallback on fail" style={NODE_STYLES.process} />
          </PhaseBox>

          {/* Federation Controller */}
          <PhaseBox x={800} y={88} width={280} height={100} title="Federation" color="#F59E0B">
            <SvgNode x={820} y={126} width={115} height={34} label="Echo Loop" sublabel="safe / sync" style={{ fill: '#1A1510', stroke: '#F59E0B', strokeWidth: 1 }} />
            <SvgNode x={950} y={126} width={115} height={34} label="Conflict Resolver" sublabel="deterministic" style={{ fill: '#1A1510', stroke: '#F59E0B', strokeWidth: 1 }} />
          </PhaseBox>

          {/* ═══════ LAYER 3: CORE SUBSYSTEMS ═══════ */}
          <SectionLabel x={30} y={228} text="CORE SUBSYSTEMS" color="#14B8A6" />

          {/* LinearRAG */}
          <PhaseBox x={60} y={212} width={350} height={170} title="LinearRAG — Retrieval Engine" color="#14B8A6">
            <SvgNode x={80} y={250} width={100} height={34} label="BM25" sublabel="keyword search" style={NODE_STYLES.core} />
            <SvgNode x={195} y={250} width={100} height={34} label="BGE-M3" sublabel="vector search" style={NODE_STYLES.core} />
            <SvgNode x={310} y={250} width={80} height={34} label="PageRank" sublabel="graph score" style={NODE_STYLES.core} />

            {/* Future: HippoRAG-PPR */}
            <rect x={80} y={300} width={310} height={32} rx={4} fill="none" stroke="#F59E0B" strokeWidth={0.8} strokeDasharray="4 2" opacity={0.5} />
            <text x={235} y={320} textAnchor="middle" fill="#F59E0B" fontSize="10" fontFamily="ui-monospace, monospace" opacity={0.7}>
              FUTURE: HippoRAG-PPR (Personalized PageRank)
            </text>

            <SvgNode x={140} y={342} width={100} height={28} label="RRF Fusion" sublabel="rank merge" style={{ fill: '#0C1A1A', stroke: '#14B8A6', strokeWidth: 1.5 }} />
          </PhaseBox>

          {/* Coloquio */}
          <PhaseBox x={440} y={212} width={260} height={170} title="Coloquio — Episodic Memory" color="#3B82F6">
            <SvgNode x={460} y={250} width={100} height={34} label="Conversation" sublabel="turn buffer" style={{ fill: '#0F1520', stroke: '#3B82F6', strokeWidth: 1 }} />
            <SvgNode x={580} y={250} width={100} height={34} label="Extractor" sublabel="episodes → nodes" style={{ fill: '#0F1520', stroke: '#3B82F6', strokeWidth: 1 }} />
            <SvgNode x={460} y={300} width={100} height={34} label="Temporal" sublabel="time indexing" style={{ fill: '#0F1520', stroke: '#3B82F6', strokeWidth: 1 }} />
            <SvgNode x={580} y={300} width={100} height={34} label="Speaker ID" sublabel="who said what" style={{ fill: '#0F1520', stroke: '#3B82F6', strokeWidth: 1 }} />
            <SvgNode x={520} y={342} width={100} height={28} label="tldraw" sublabel="whiteboard" style={{ fill: '#0F1520', stroke: '#3B82F6', strokeWidth: 0.5, strokeDasharray: '3 2' } as any} />
          </PhaseBox>

          {/* Guilds */}
          <PhaseBox x={730} y={212} width={350} height={170} title="Guilds — Agent Capabilities" color="#A855F7">
            <SvgNode x={750} y={250} width={100} height={34} label="tylluan_do" sublabel="execute actions" style={NODE_STYLES.process} />
            <SvgNode x={870} y={250} width={100} height={34} label="tylluan_recall" sublabel="memory access" style={NODE_STYLES.process} />
            <SvgNode x={750} y={300} width={100} height={34} label="tylluan_store" sublabel="persist memory" style={NODE_STYLES.process} />
            <SvgNode x={870} y={300} width={100} height={34} label="tylluan_ask" sublabel="query LLM" style={NODE_STYLES.process} />

            <rect x={750} y={342} width={220} height={28} rx={4} fill="none" stroke="#F59E0B" strokeWidth={0.8} strokeDasharray="4 2" opacity={0.5} />
            <text x={860} y={360} textAnchor="middle" fill="#F59E0B" fontSize="10" fontFamily="ui-monospace, monospace" opacity={0.7}>
              FUTURE: Local tool-calling model (26M-1B)
            </text>
          </PhaseBox>

          {/* ═══════ LAYER 4: STORAGE ═══════ */}
          <SectionLabel x={30} y={418} text="STORAGE & MEMORY MODEL" color="#14B8A6" />

          {/* SilvaDB — Central Graph Store */}
          <PhaseBox x={60} y={402} width={520} height={130} title="SilvaDB — Graph Memory Store" color="#14B8A6">
            <SvgNode x={80} y={440} width={130} height={38} label="Node Store" sublabel="id · content · embedding" style={{ fill: '#0C1A1A', stroke: '#14B8A6', strokeWidth: 1.5 }} />
            <SvgNode x={230} y={440} width={130} height={38} label="Edge Store" sublabel="associations" style={{ fill: '#0C1A1A', stroke: '#14B8A6', strokeWidth: 1.5 }} />
            <SvgNode x={380} y={440} width={130} height={38} label="Meta Store" sublabel="timestamps · tags" style={{ fill: '#0C1A1A', stroke: '#14B8A6', strokeWidth: 1.5 }} />

            <SvgNode x={120} y={490} width={160} height={30} label="FSRS Memory Model" sublabel="stability · difficulty · retrievability" style={{ fill: '#0A1A15', stroke: '#10B981', strokeWidth: 1.5 }} highlight />
            <SvgNode x={300} y={490} width={130} height={30} label="Decay Engine" sublabel="2^(-Δt/stability)" style={{ fill: '#0A1A15', stroke: '#10B981', strokeWidth: 1 }} />
          </PhaseBox>

          {/* Query Cache */}
          <PhaseBox x={610} y={402} width={200} height={130} title="Query Cache" color="#64748B">
            <SvgNode x={630} y={440} width={160} height={34} label="LRU Cache" sublabel="TTL · 256 entries" style={NODE_STYLES.subsystem} />
            <SvgNode x={630} y={484} width={160} height={34} label="Embedding Cache" sublabel="BGE-M3 results" style={NODE_STYLES.subsystem} />
          </PhaseBox>

          {/* Profiles */}
          <PhaseBox x={840} y={402} width={240} height={130} title="Deployment Profiles" color="#64748B">
            <SvgNode x={860} y={440} width={60} height={30} label="📱" sublabel="" style={NODE_STYLES.subsystem} />
            <text x={890} y={452} textAnchor="middle" fill="#94A3B8" fontSize="9" fontFamily="ui-monospace, monospace">portable</text>
            <SvgNode x={930} y={440} width={60} height={30} label="🏥" sublabel="" style={NODE_STYLES.subsystem} />
            <text x={960} y={452} textAnchor="middle" fill="#94A3B8" fontSize="9" fontFamily="ui-monospace, monospace">clinic</text>
            <SvgNode x={1000} y={440} width={60} height={30} label="🖥" sublabel="" style={NODE_STYLES.subsystem} />
            <text x={1030} y={452} textAnchor="middle" fill="#94A3B8" fontSize="9" fontFamily="ui-monospace, monospace">server</text>

            <SvgNode x={860} y={484} width={100} height={30} label="Config Layer" sublabel="features flags" style={NODE_STYLES.subsystem} />
            <SvgNode x={970} y={484} width={90} height={30} label="mDNS" sublabel="peer discovery" style={NODE_STYLES.subsystem} />
          </PhaseBox>

          {/* ═══════ LAYER 5: INFRASTRUCTURE ═══════ */}
          <SectionLabel x={30} y="562" text="INFRASTRUCTURE" color="#64748B" />

          <SvgNode x={60} y={548} width={160} height={36} label="BGE-M3 Embeddings" sublabel="multilingual · 1024d" style={NODE_STYLES.subsystem} />
          <SvgNode x={250} y={548} width={140} height={36} label="HNSW Index" sublabel="vector search" style={NODE_STYLES.subsystem} />
          <SvgNode x={420} y={548} width={130} height={36} label="SQLite / WAL" sublabel="persistence" style={NODE_STYLES.subsystem} />
          <SvgNode x={580} y={548} width={140} height={36} label="Noise Protocol" sublabel="encrypted p2p" style={NODE_STYLES.subsystem} />
          <SvgNode x={750} y={548} width={150} height={36} label="ONNX Runtime" sublabel="model inference" style={NODE_STYLES.subsystem} />

          {/* ═══════ FUTURE: SLEEP CYCLE ═══════ */}
          <PhaseBox x={60} y="612" width={840} height={80} title="FUTURE: SleepCycle (SCM) — Memory Consolidation" color="#F59E0B">
            <SvgNode x={80} y={650} width={120} height={30} label="NREM Phase" sublabel="deduplicate · merge" style={{ fill: '#1A1510', stroke: '#F59E0B', strokeWidth: 1, strokeDasharray: '3 2' } as any} />
            <SvgNode x={220} y={650} width={120} height={30} label="REM Phase" sublabel="reactivate weak" style={{ fill: '#1A1510', stroke: '#F59E0B', strokeWidth: 1, strokeDasharray: '3 2' } as any} />
            <SvgNode x={360} y={650} width={140} height={30} label="Value Forgetting" sublabel="prune low-value" style={{ fill: '#1A1510', stroke: '#F59E0B', strokeWidth: 1, strokeDasharray: '3 2' } as any} />
            <SvgNode x={520} y={650} width={140} height={30} label="Self-Model" sublabel="introspection" style={{ fill: '#1A1510', stroke: '#F59E0B', strokeWidth: 1, strokeDasharray: '3 2' } as any} />
            <text x={720} y={668} textAnchor="middle" fill="#64748B" fontSize="10" fontFamily="ui-monospace, monospace">
              tylluan-cli sleep
            </text>
          </PhaseBox>

          {/* ═══════ DATA FLOW ARROWS ═══════ */}

          {/* Interfaces → Orchestration */}
          <Connection x1={185} y1={60} x2={185} y2={88} color="#64748B" markerId="arrow-control" />
          <Connection x1={365} y1={60} x2={365} y2={88} color="#64748B" markerId="arrow-control" />
          <Connection x1={570} y1={60} x2={570} y2={88} color="#64748B" markerId="arrow-control" />
          <Connection x1={770} y1={60} x2={860} y2={88} color="#64748B" markerId="arrow-control" />

          {/* Complexity Cascade → Core */}
          <Connection x1={250} y1={188} x2={235} y2={212} markerId="arrow-data" label="intent" />
          <Connection x1={350} y1={188} x2={440} y2={250} markerId="arrow-data" label="episodes" />
          <Connection x1={400} y1={188} x2={600} y2={188} color="#64748B" markerId="arrow-control" />

          {/* DispatchRouter → Guilds */}
          <Connection x1={600} y1={188} x2={810} y2={212} markerId="arrow-data" label="dispatch" />

          {/* Federation ↔ SilvaDB */}
          <Connection x1={940} y1={188} x2={580} y2={402} color="#F59E0B" markerId="arrow-sync" label="sync" dashed />
          <Connection x1={520} y1={402} x2={860} y2={188} color="#F59E0B" markerId="arrow-sync" label="conflicts" dashed />

          {/* Core → Storage */}
          <Connection x1={190} y1={382} x2={190} y2={402} markerId="arrow-data" label="read/write" />
          <Connection x1={570} y1={382} x2={320} y2={402} markerId="arrow-data" label="store episodes" />
          <Connection x1={900} y1={382} x2={860} y2={402} markerId="arrow-data" label="recall" />

          {/* Storage → Infrastructure */}
          <Connection x1={140} y1={532} x2={140} y2={548} color="#64748B" markerId="arrow-control" label="embed" />
          <Connection x1={320} y1={532} x2={320} y2={548} color="#64748B" markerId="arrow-control" label="index" />
          <Connection x1={480} y1={532} x2={480} y2={548} color="#64748B" markerId="arrow-control" />
          <Connection x1={825} y1={532} x2={825} y2={548} color="#64748B" markerId="arrow-control" label="inference" />

          {/* SleepCycle → SilvaDB */}
          <Connection x1={480} y1={612} x2={320} y2={532} color="#F59E0B" markerId="arrow-sync" label="consolidate" dashed />

          {/* Federation ↔ Peers (right side) */}
          <PhaseBox x={920} y={548} width={260} height={144} title="P2P Mesh Network" color="#F59E0B">
            {/* Peer nodes */}
            <circle cx={970} cy={598} r={18} fill="#1A1510" stroke="#F59E0B" strokeWidth={1} />
            <text x={970} y={602} textAnchor="middle" fill="#F59E0B" fontSize="10" fontFamily="ui-monospace, monospace">P1</text>

            <circle cx={1060} cy={575} r={18} fill="#1A1510" stroke="#F59E0B" strokeWidth={1} />
            <text x={1060} y={579} textAnchor="middle" fill="#F59E0B" fontSize="10" fontFamily="ui-monospace, monospace">P2</text>

            <circle cx={1060} cy={625} r={18} fill="#1A1510" stroke="#F59E0B" strokeWidth={1} />
            <text x={1060} y={629} textAnchor="middle" fill="#F59E0B" fontSize="10" fontFamily="ui-monospace, monospace">P3</text>

            <circle cx={1140} cy={598} r={18} fill="#1A1510" stroke="#F59E0B" strokeWidth={1} />
            <text x={1140} y={602} textAnchor="middle" fill="#F59E0B" fontSize="10" fontFamily="ui-monospace, monospace">P4</text>

            {/* Mesh connections */}
            <line x1={988} y1={590} x2={1042} y2={580} stroke="#F59E0B" strokeWidth={0.8} opacity={0.4} />
            <line x1={988} y1={606} x2={1042} y2={620} stroke="#F59E0B" strokeWidth={0.8} opacity={0.4} />
            <line x1={1078} y1={575} x2={1078} y2={607} stroke="#F59E0B" strokeWidth={0.8} opacity={0.4} />
            <line x1={1078} y1={590} x2={1122} y2={598} stroke="#F59E0B" strokeWidth={0.8} opacity={0.4} />
            <line x1={1078} y1={625} x2={1122} y2={604} stroke="#F59E0B" strokeWidth={0.8} opacity={0.4} />

            {/* KNEXA-FL note */}
            <text x={1060} y={668} textAnchor="middle" fill="#64748B" fontSize="9" fontFamily="ui-monospace, monospace">
              FUTURE: LinUCB matchmaking
            </text>
          </PhaseBox>

          {/* Federation → Mesh */}
          <Connection x1={940} y1={188} x2={1000} y2={548} color="#F59E0B" markerId="arrow-sync" dashed label="Noise Protocol" />

          {/* ═══════ KEY INSIGHT CALLOUTS ═══════ */}
          <g>
            <rect x={60} y="710" width={520} height={60} rx={6} fill="#0C1A1A" stroke="#10B981" strokeWidth={0.8} opacity={0.8} />
            <text x={76} y={730} fill="#10B981" fontSize="10" fontFamily="ui-monospace, monospace" fontWeight={600}>
              ✦ FSRS Integration (v13 — ACTIVE)
            </text>
            <text x={76} y={746} fill="#94A3B8" fontSize="10" fontFamily="system-ui, sans-serif">
              Replaced fixed half-life (T½=14d) with per-memory stability model.
            </text>
            <text x={76} y={760} fill="#64748B" fontSize="9" fontFamily="ui-monospace, monospace">
              R = 2^(-Δt/S)  ·  touch_node → review(Good)  ·  305 tests passing
            </text>
          </g>

          <g>
            <rect x={600} y="710" width={580} height={60} rx={6} fill="#1A1510" stroke="#F59E0B" strokeWidth={0.8} opacity={0.8} />
            <text x={616} y={730} fill="#F59E0B" fontSize="10" fontFamily="ui-monospace, monospace" fontWeight={600}>
              ⬡ Papers 2026 — Proposed Enhancements
            </text>
            <text x={616} y={746} fill="#94A3B8" fontSize="10" fontFamily="system-ui, sans-serif">
              HippoRAG-PPR (associative recall) · SCM SleepCycle · KNEXA-FL (bandit dispatch) ·
            </text>
            <text x={616} y={760} fill="#64748B" fontSize="9" fontFamily="ui-monospace, monospace">
              Deterministic freshness · Local tool-calling (26M-1B) · Mem0 benchmarking
            </text>
          </g>

          {/* Benchmark callout */}
          <g>
            <rect x={60} y="785" width={340} height={40} rx={4} fill="#0F1520" stroke="#3B82F6" strokeWidth={0.6} opacity={0.6} />
            <text x={76} y={804} fill="#3B82F6" fontSize="9" fontFamily="ui-monospace, monospace">
              Benchmark: LongMemEval R@5 = 82% (validated)
            </text>
            <text x={76} y={818} fill="#64748B" fontSize="9" fontFamily="ui-monospace, monospace">
              HNSW threshold: 12k nodes · Query Cache: LRU TTL 256
            </text>
          </g>

          {/* Sovereignty callout */}
          <g>
            <rect x={420} y="785" width={360} height={40} rx={4} fill="#0F172A" stroke="#64748B" strokeWidth={0.6} opacity={0.6} />
            <text x={436} y={804} fill="#94A3B8" fontSize="9" fontFamily="ui-monospace, monospace">
              Design Principle: Zero cloud dependency on critical path
            </text>
            <text x={436} y={818} fill="#64748B" fontSize="9" fontFamily="ui-monospace, monospace">
              All models run locally · ONNX runtime · Pi 4 compatible (portable)
            </text>
          </g>
        </svg>
      </motion.div>

      {/* Architecture summary cards */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-3 mt-2">
        <div className="rounded-lg border border-border/50 bg-surface/50 p-4">
          <h4 className="text-xs font-mono font-semibold text-teal-400 mb-2 uppercase tracking-wider">Capas del sistema</h4>
          <div className="space-y-1.5 text-xs text-muted-foreground">
            <div className="flex justify-between"><span>Interfaces</span><span className="font-mono text-slate-400">CLI · API · Dashboard · Coloquio</span></div>
            <div className="flex justify-between"><span>Orquestación</span><span className="font-mono text-slate-400">Cascade · Dispatch · Federation</span></div>
            <div className="flex justify-between"><span>Core</span><span className="font-mono text-slate-400">LinearRAG · Coloquio · Guilds</span></div>
            <div className="flex justify-between"><span>Storage</span><span className="font-mono text-slate-400">SilvaDB · FSRS · Cache</span></div>
            <div className="flex justify-between"><span>Infra</span><span className="font-mono text-slate-400">BGE-M3 · HNSW · SQLite · ONNX</span></div>
          </div>
        </div>
        <div className="rounded-lg border border-border/50 bg-surface/50 p-4">
          <h4 className="text-xs font-mono font-semibold text-emerald-400 mb-2 uppercase tracking-wider">Primitivos activos</h4>
          <div className="space-y-1.5 text-xs text-muted-foreground">
            <div className="flex justify-between"><span>Decay model</span><span className="font-mono text-emerald-500/80">FSRS (per-memory stability)</span></div>
            <div className="flex justify-between"><span>Retrieval</span><span className="font-mono text-emerald-500/80">BM25 + BGE-M3 + PageRank + RRF</span></div>
            <div className="flex justify-between"><span>Graph</span><span className="font-mono text-emerald-500/80">Node + Edge store</span></div>
            <div className="flex justify-between"><span>Federation</span><span className="font-mono text-emerald-500/80">Echo loop (safe mode)</span></div>
            <div className="flex justify-between"><span>Mesh</span><span className="font-mono text-emerald-500/80">mDNS discovery · Noise Protocol</span></div>
          </div>
        </div>
        <div className="rounded-lg border border-border/50 bg-surface/50 p-4">
          <h4 className="text-xs font-mono font-semibold text-amber-400 mb-2 uppercase tracking-wider">Eje de mejora (papers 2026)</h4>
          <div className="space-y-1.5 text-xs text-muted-foreground">
            <div className="flex justify-between"><span>Heurísticas → Algoritmos</span><span className="font-mono text-amber-500/80">3 puntos de cambio</span></div>
            <div className="flex justify-between"><span>Consolidación offline</span><span className="font-mono text-amber-500/80">SleepCycle (SCM)</span></div>
            <div className="flex justify-between"><span>Recuperación asociativa</span><span className="font-mono text-amber-500/80">HippoRAG-PPR</span></div>
            <div className="flex justify-between"><span>Soberanía total</span><span className="font-mono text-amber-500/80">Tool-calling 26M-1B</span></div>
            <div className="flex justify-between"><span>Citabilidad</span><span className="font-mono text-amber-500/80">Mem0 benchmark + taxonomy</span></div>
          </div>
        </div>
      </div>
    </div>
  );
}