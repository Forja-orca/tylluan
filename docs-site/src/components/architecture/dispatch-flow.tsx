'use client';

import { motion } from 'framer-motion';
import { ArrowMarker, SvgNode, Connection, PhaseBox, SectionLabel, NODE_STYLES } from './shared';

export function DispatchFlow() {
  return (
    <div className="space-y-4">
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: 0.6 }}
        className="w-full overflow-x-auto rounded-xl border border-border/50 bg-[#0A0F1A] p-4"
      >
        <svg viewBox="0 0 1100 780" className="w-full h-auto min-w-[800px]">
          <ArrowMarker id="disp-arrow" color="#14B8A6" />
          <ArrowMarker id="disp-arrow-purple" color="#A855F7" />
          <ArrowMarker id="disp-arrow-amber" color="#F59E0B" />
          <ArrowMarker id="disp-arrow-red" color="#EF4444" />
          <ArrowMarker id="disp-arrow-gray" color="#64748B" />

          {/* ═══════ STEP 1: INTENT ARRIVES ═══════ */}
          <SectionLabel x={30} y={30} text="STEP 1: Intent Arrival" color="#94A3B8" />

          <SvgNode x={200} y={18} width={240} height={38} label="User Input" sublabel="natural language / command" style={NODE_STYLES.subsystem} />
            <SvgNode x={500} y={18} width={200} height={38} label="Guild Call" sublabel="tylluan_do / recall / remember" style={NODE_STYLES.process} />

          <Connection x1={440} y1={37} x2={500} y2={37} markerId="disp-arrow-gray" strokeWidth={0.8} />

          {/* ═══════ STEP 2: COMPLEXITY CASCADE ═══════ */}
          <SectionLabel x={30} y={78} text="STEP 2: Complexity Cascade" color="#A855F7" />

          <PhaseBox x={150} y={88} width={800} height={140} title="Complexity Cascade — Intent Classification" color="#A855F7">
            {/* Intent Parser */}
            <SvgNode x={180} y={125} width={150} height={34} label="Intent Parser" sublabel="classify intent type" style={NODE_STYLES.process} />
            <SvgNode x={360} y={125} width={150} height={34} label="Complexity Scorer" sublabel="0.0 → 1.0" style={NODE_STYLES.process} />

            <Connection x1={330} y1={142} x2={360} y2={142} markerId="disp-arrow-purple" label="features" strokeWidth={0.8} />

            {/* Score visualization */}
            <g>
              <rect x={560} y={115} width={360} height={52} rx={4} fill="#111827" />
              <text x={575} y={132} fill="#64748B" fontSize="8" fontFamily="ui-monospace, monospace">
                Score inputs: input length · entity count · nested intents · guild history
              </text>

              {/* Score bar */}
              <rect x={575} y={140} width={330} height={8} rx={4} fill="#1E293B" />
              <rect x={575} y={140} width={165} height={8} rx={4} fill="#A855F7" opacity={0.6} />
              <text x={575} y={162} fill="#A855F7" fontSize="8" fontFamily="ui-monospace, monospace">
                0.0
              </text>
              <text x={740} y={162} fill="#F59E0B" fontSize="8" fontFamily="ui-monospace, monospace">
                0.5
              </text>
              <text x={890} y={162} fill="#EF4444" fontSize="8" fontFamily="ui-monospace, monospace">
                1.0
              </text>

              {/* Markers */}
              <line x1={640} y1={138} x2={640} y2={150} stroke="#10B981" strokeWidth={2} />
              <text x={640} y={136} textAnchor="middle" fill="#10B981" fontSize="7" fontFamily="ui-monospace">local</text>
              <line x1={810} y1={138} x2={810} y2={150} stroke="#F59E0B" strokeWidth={2} />
              <text x={810} y={136} textAnchor="middle" fill="#F59E0B" fontSize="7" fontFamily="ui-monospace">router</text>
            </g>

            {/* Decision outputs */}
            <text x={180} y={200} fill="#10B981" fontSize="9" fontFamily="ui-monospace, monospace">
              score &lt; 0.3 → LOCAL (embeddings only)
            </text>
            <text x={180} y={214} fill="#F59E0B" fontSize="9" fontFamily="ui-monospace, monospace">
              score ≥ 0.3 → ROUTER (full dispatch)
            </text>
          </PhaseBox>

          {/* ═══════ STEP 3A: LOCAL PATH (low complexity) ═══════ */}
          <SectionLabel x={30} y={250} text="STEP 3A: Local Resolution (score &lt; 0.3)" color="#10B981" />

          <PhaseBox x={40} y={262} width={500} height={110} title="Local Path — Embedding Similarity" color="#10B981">
            <SvgNode x={60} y={300} width={150} height={30} label="BGE-M3 Encode Query" sublabel="1024d" style={NODE_STYLES.core} />
            <SvgNode x={230} y={300} width={150} height={30} label="Cosine to Guild Descs" sublabel="similarity ranking" style={NODE_STYLES.core} />
            <SvgNode x={400} y={300} width={120} height={30} label="Select Guild" sublabel="top-1" style={{ fill: '#0A1A15', stroke: '#10B981', strokeWidth: 1.5 }} />

            <Connection x1={210} y1={315} x2={230} y2={315} markerId="disp-arrow" strokeWidth={0.8} />
            <Connection x1={380} y1={315} x2={400} y2={315} markerId="disp-arrow" strokeWidth={0.8} />

            <text x={60} y={355} fill="#64748B" fontSize="9" fontFamily="ui-monospace, monospace">
              Portable profile: CPU-only, no LLM needed for routing
            </text>
          </PhaseBox>

          {/* ═══════ STEP 3B: FULL ROUTER (high complexity) ═══════ */}
          <SectionLabel x={570} y={250} text="STEP 3B: Full Dispatch (score ≥ 0.3)" color="#F59E0B" />

          <PhaseBox x={560} y={262} width={510} height={110} title="Full Router — DispatchRouter + Model" color="#F59E0B">
            {/* Current heuristic */}
            <SvgNode x={580} y={295} width={130} height={28} label="Peer Scoring" sublabel="load·lat·GPU" style={NODE_STYLES.process} />
            <text x={720} y={308} fill="#F59E0B" fontSize="8" fontFamily="ui-monospace, monospace">
              CURRENT (heuristic)
            </text>

            {/* Future bandit */}
            <rect x={580} y={332} width={460} height={28} rx={4} fill="none" stroke="#F59E0B" strokeWidth={0.8} strokeDasharray="4 2" opacity={0.5} />
            <text x={810} y={350} textAnchor="middle" fill="#F59E0B" fontSize="9" fontFamily="ui-monospace, monospace" opacity={0.7}>
              FUTURE: LinUCB bandit (KNEXA-FL) · Local tool-calling model 26M-1B (ONNX)
            </text>

            <Connection x1={710} y1={309} x2={710} y2={332} markerId="disp-arrow-amber" strokeWidth={0.8} label="select peer" />
          </PhaseBox>

          {/* ═══════ STEP 4: GUILD EXECUTION ═══════ */}
          <SectionLabel x={30} y={395} text="STEP 4: Guild Execution" color="#14B8A6" />

          <PhaseBox x={40} y={407} width={1030} height={140} title="Guild Execution & Response" color="#14B8A6">
            {/* Guild types */}
            <SvgNode x={60} y={440} width={110} height={34} label="tylluan_do" sublabel="execute action" style={NODE_STYLES.process} />
            <SvgNode x={190} y={440} width={110} height={34} label="tylluan_recall" sublabel="memory search" style={NODE_STYLES.process} />
            <SvgNode x={320} y={440} width={130} height={34} label="tylluan_remember" sublabel="persist memory" style={NODE_STYLES.process} />
            <SvgNode x={470} y={440} width={110} height={34} label="tylluan_graph" sublabel="knowledge graph" style={NODE_STYLES.process} />
            <SvgNode x={600} y={440} width={110} height={34} label="tylluan_think" sublabel="reason + reflect" style={NODE_STYLES.process} />

            {/* Execution details */}
            <text x={600} y={448} fill="#94A3B8" fontSize="9" fontFamily="ui-monospace, monospace">
              Each guild:
            </text>
            <text x={600} y={464} fill="#64748B" fontSize="9" fontFamily="ui-monospace, monospace">
              1. Receives args from router
            </text>
            <text x={600} y={478} fill="#64748B" fontSize="9" fontFamily="ui-monospace, monospace">
              2. May call LinearRAG (recall)
            </text>
            <text x={600} y={492} fill="#64748B" fontSize="9" fontFamily="ui-monospace, monospace">
              3. Executes with timeout
            </text>
            <text x={600} y={506} fill="#64748B" fontSize="9" fontFamily="ui-monospace, monospace">
              4. Returns result + metadata
            </text>
            <text x={600} y={520} fill="#64748B" fontSize="9" fontFamily="ui-monospace, monospace">
              5. touch_node() on accessed memories
            </text>

            {/* Circuit Breaker */}
            <rect x={60} y={490} width={490} height={40} rx={4} fill="#1A1010" stroke="#EF4444" strokeWidth={0.6} />
            <text x={75} y={508} fill="#EF4444" fontSize="9" fontFamily="ui-monospace, monospace" fontWeight={600}>
              Circuit Breaker
            </text>
            <text x={75} y={522} fill="#64748B" fontSize="8" fontFamily="ui-monospace, monospace">
              Timeout / error → open circuit → fallback to local execution → half-open after cooldown
            </text>
          </PhaseBox>

          {/* ═══════ STEP 5: RESPONSE ═══════ */}
          <SectionLabel x={30} y={565} text="STEP 5: Response" color="#10B981" />

          <PhaseBox x={120} y={577} width={840} height={65} title="Response Assembly" color="#10B981">
            <SvgNode x={140} y={607} width={120} height={26} label="Guild Result" sublabel="stdout/stderr" style={NODE_STYLES.core} />
            <SvgNode x={280} y={607} width={120} height={26} label="Recall Context" sublabel="memories used" style={NODE_STYLES.core} />
            <SvgNode x={420} y={607} width={120} height={26} label="FSRS Update" sublabel="touch accessed" style={{ fill: '#0A1A15', stroke: '#10B981', strokeWidth: 1 }} />
            <SvgNode x={560} y={607} width={120} height={26} label="DreamCycle" sublabel="consolidate episode" style={{ fill: '#0A1A15', stroke: '#10B981', strokeWidth: 1 }} />
            <SvgNode x={700} y={607} width={120} height={26} label="Audit Log" sublabel="guild_call entry" style={NODE_STYLES.subsystem} />

            <Connection x1={350} y1={620} x2={370} y2={620} markerId="disp-arrow" strokeWidth={0.8} />
            <Connection x1={500} y1={620} x2={520} y2={620} markerId="disp-arrow" strokeWidth={0.8} />
            <Connection x1={650} y1={620} x2={670} y2={620} markerId="disp-arrow" strokeWidth={0.8} />
          </PhaseBox>

          {/* ═══════ FLOW ARROWS ═══════ */}
          <Connection x1={600} y1={56} x2={600} y2={88} markerId="disp-arrow-gray" strokeWidth={0.8} />

          {/* Branch from complexity cascade */}
          <line x1={350} y1={228} x2={350} y2={245} stroke="#10B981" strokeWidth={1.5} markerEnd="url(#disp-arrow)" />
          <text x={310} y={240} fill="#10B981" fontSize="8" fontFamily="ui-monospace">&lt; 0.3</text>

          <line x1={700} y1={228} x2={700} y2={245} stroke="#F59E0B" strokeWidth={1.5} markerEnd="url(#disp-arrow-amber)" />
          <text x={710} y={240} fill="#F59E0B" fontSize="8" fontFamily="ui-monospace">≥ 0.3</text>

          {/* Both paths merge to guild execution */}
          <Connection x1={290} y1={372} x2={290} y2={407} markerId="disp-arrow" label="local guild" strokeWidth={0.8} />
          <Connection x1={810} y1={372} x2={810} y2={407} markerId="disp-arrow-amber" label="remote guild" strokeWidth={0.8} />

          {/* To response */}
          <Connection x1={550} y1={547} x2={550} y2={577} markerId="disp-arrow" strokeWidth={0.8} />

          {/* ═══════ SOVEREIGNTY NOTE ═══════ */}
          <rect x={40} y={660} width={1020} height={100} rx={6} fill="#0F1520" stroke="#3B82F6" strokeWidth={0.8} opacity={0.8} />
          <text x={60} y={685} fill="#3B82F6" fontSize="11" fontFamily="ui-monospace, monospace" fontWeight={600}>
            Sovereignty Analysis: The Dispatch Path
          </text>
          <div className="font-mono text-xs" style={{ fill: '#94A3B8' }}>
            <text x={60} y={705} fontFamily="ui-monospace, monospace" fontSize="10">
              Current: CLI → Complexity Cascade (local) → DispatchRouter (local scoring) → Guild (may be remote peer) → Response
            </text>
            <text x={60} y={722} fontFamily="ui-monospace, monospace" fontSize="10">
              Gap: DispatchRouter scoring is heuristic (fixed weights). Peer selection doesn&apos;t learn from outcomes.
            </text>
            <text x={60} y={739} fontFamily="ui-monospace, monospace" fontSize="10">
              Fix (KNEXA-FL): Replace heuristic weights with LinUCB contextual bandit. Each node learns locally.
            </text>
          </div>
        </svg>
      </motion.div>
    </div>
  );
}