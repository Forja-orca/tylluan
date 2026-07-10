'use client';

import { motion } from 'framer-motion';
import { ArrowMarker, SvgNode, Connection, PhaseBox, SectionLabel, NODE_STYLES } from './shared';

export function FederationMesh() {
  return (
    <div className="space-y-4">
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: 0.6 }}
        className="w-full overflow-x-auto rounded-xl border border-border/50 bg-[#0A0F1A] p-4"
      >
        <svg viewBox="0 0 1100 750" className="w-full h-auto min-w-[800px]">
          <ArrowMarker id="fed-arrow" color="#14B8A6" />
          <ArrowMarker id="fed-arrow-amber" color="#F59E0B" />
          <ArrowMarker id="fed-arrow-red" color="#EF4444" />
          <ArrowMarker id="fed-arrow-gray" color="#64748B" />

          {/* ═══════ SINGLE PEER DETAIL (Left) ═══════ */}
          <SectionLabel x={20} y={32} text="PEER NODE (local)" color="#14B8A6" />

          {/* Peer box */}
          <PhaseBox x={20} y={45} width={500} height={380} title="Tylluan Peer" color="#14B8A6">
            {/* Local SilvaDB */}
            <PhaseBox x={40} y={82} width={220} height={130} title="SilvaDB (local)" color="#14B8A6">
              <SvgNode x={55} y={115} width={90} height={26} label="Nodes" sublabel="memories" style={NODE_STYLES.core} />
              <SvgNode x={155} y={115} width={90} height={26} label="Edges" sublabel="associations" style={NODE_STYLES.core} />
              <SvgNode x={55} y={152} width={90} height={26} label="FSRS" sublabel="S · D · R" style={{ fill: '#0A1A15', stroke: '#10B981', strokeWidth: 1 }} />
              <SvgNode x={155} y={152} width={90} height={26} label="HNSW" sublabel="index" style={NODE_STYLES.core} />
            </PhaseBox>

            {/* Federation module */}
            <PhaseBox x={280} y={82} width={220} height={130} title="Federation Module" color="#F59E0B">
              <SvgNode x={295} y={115} width={90} height={26} label="Echo Loop" sublabel="safe mode" style={{ fill: '#1A1510', stroke: '#F59E0B', strokeWidth: 1 }} />
              <SvgNode x={395} y={115} width={90} height={26} label="Signer" sublabel="Noise Proto" style={{ fill: '#1A1510', stroke: '#F59E0B', strokeWidth: 1 }} />
              <SvgNode x={295} y={152} width={90} height={26} label="Resolver" sublabel="deterministic" style={{ fill: '#1A1510', stroke: '#F59E0B', strokeWidth: 1 }} />
              <SvgNode x={395} y={152} width={90} height={26} label="Ledger" sublabel="sync log" style={{ fill: '#1A1510', stroke: '#F59E0B', strokeWidth: 1 }} />
            </PhaseBox>

            {/* Internal flows */}
            <Connection x1={260} y1={147} x2={280} y2={147} markerId="fed-arrow-amber" label="changes" strokeWidth={0.8} />

            {/* Discovery */}
            <SvgNode x={40} y={230} width={130} height={30} label="mDNS Discovery" sublabel="local peers" style={NODE_STYLES.subsystem} />
            <SvgNode x={190} y={230} width={130} height={30} label="DispatchRouter" sublabel="score peers" style={NODE_STYLES.process} />

            {/* Complexity Cascade */}
            <PhaseBox x={340} y={225} width={160} height={90} title="Complexity Cascade" color="#A855F7">
              <SvgNode x={355} y={257} width={125} height={24} label="Intent → Score" sublabel="route decision" style={NODE_STYLES.process} />
              <SvgNode x={355} y={285} width={125} height={20} label="Local vs Remote" sublabel="" style={NODE_STYLES.process} />
            </PhaseBox>

            <Connection x1={105} y1={260} x2={190} y2={245} markerId="fed-arrow-gray" strokeWidth={0.8} />
            <Connection x1={320} y1={245} x2={340} y2={250} markerId="fed-arrow-gray" label="score" strokeWidth={0.8} />

            {/* FSRS note */}
            <rect x={40} y={275} width={280} height={50} rx={4} fill="#0A1A15" stroke="#10B981" strokeWidth={0.6} />
            <text x={55} y={293} fill="#10B981" fontSize="9" fontFamily="ui-monospace, monospace" fontWeight={600}>
              FSRS: cada peer mantiene su propio estado
            </text>
            <text x={55} y={307} fill="#64748B" fontSize="9" fontFamily="ui-monospace, monospace">
              La memoria se sincroniza como contenido,
            </text>
            <text x={55} y={319} fill="#64748B" fontSize="9" fontFamily="ui-monospace, monospace">
              pero el modelo de olvido es LOCAL (soberanía)
            </text>

            {/* Guilds */}
            <SvgNode x={40} y={340} width={100} height={28} label="tylluan_do" sublabel="execute" style={NODE_STYLES.process} />
            <SvgNode x={150} y={340} width={100} height={28} label="tylluan_recall" sublabel="search" style={NODE_STYLES.process} />
            <SvgNode x={260} y={340} width={100} height={28} label="tylluan_store" sublabel="persist" style={NODE_STYLES.process} />
            <SvgNode x={370} y={340} width={110} height={28} label="tylluan_ask" sublabel="LLM query" style={NODE_STYLES.process} />

            <text x={40} y={390} fill="#475569" fontSize="8" fontFamily="ui-monospace, monospace">
              Profiles: portable (Pi 4) · clinic (local server) · server (full mesh)
            </text>
          </PhaseBox>

          {/* ═══════ SYNC PROTOCOL FLOW (Right) ═══════ */}
          <SectionLabel x={550} y={32} text="SYNC PROTOCOL" color="#F59E0B" />

          {/* Step 1: Local Change */}
          <PhaseBox x={550} y={45} width={520} height={80} title="Step 1 — Local Mutation" color="#14B8A6">
            <SvgNode x={570} y={78} width={120} height={28} label="tylluan_store" sublabel="guild call" style={NODE_STYLES.process} />
            <text x={720} y={88} fill="#94A3B8" fontSize="9" fontFamily="ui-monospace, monospace">
              → New memory or update
            </text>
            <text x={720} y={104} fill="#64748B" fontSize="9" fontFamily="ui-monospace, monospace">
              → FSRS: stability, difficulty, retrievability updated locally
            </text>
          </PhaseBox>

          {/* Step 2: Sign & Propagate */}
          <PhaseBox x={550} y={140} width={520} height={70} title="Step 2 — Sign & Propagate (echo-loop safe)" color="#F59E0B">
            <SvgNode x={570} y={168} width={100} height={26} label="Sign" sublabel="Noise Proto" style={{ fill: '#1A1510', stroke: '#F59E0B', strokeWidth: 1 }} />
            <SvgNode x={690} y={168} width={100} height={26} label="Compress" sublabel="delta encode" style={NODE_STYLES.subsystem} />
            <SvgNode x={810} y={168} width={100} height={26} label="Propagate" sublabel="to N peers" style={{ fill: '#1A1510', stroke: '#F59E0B', strokeWidth: 1 }} />
            <Connection x1={670} y1={181} x2={690} y2={181} markerId="fed-arrow-amber" strokeWidth={0.8} />
            <Connection x1={790} y1={181} x2={810} y2={181} markerId="fed-arrow-amber" strokeWidth={0.8} />
          </PhaseBox>

          {/* Step 3: Conflict Detection */}
          <PhaseBox x={550} y={225} width={520} height={90} title="Step 3 — Conflict Detection & Resolution" color="#EF4444">
            <SvgNode x={570} y={260} width={140} height={26} label="SH-Conflict" sublabel="same-hash detect" style={{ fill: '#1A1010', stroke: '#EF4444', strokeWidth: 1 }} />
            <SvgNode x={730} y={260} width={140} height={26} label="CAR Resolution" sublabel="chain-aware rule" style={{ fill: '#1A1010', stroke: '#EF4444', strokeWidth: 1 }} />
            <text x={570} y={300} fill="#64748B" fontSize="9" fontFamily="ui-monospace, monospace">
              DETERMINISTIC (no LLM) · Validado en LongMemEval · Paper: &quot;Don&apos;t Ask the LLM to Track Freshness&quot; (2026)
            </text>
            <Connection x1={710} y1={273} x2={730} y2={273} markerId="fed-arrow-red" strokeWidth={0.8} />
          </PhaseBox>

          {/* Step 4: Apply */}
          <PhaseBox x={550} y="330" width={520} height={60} title="Step 4 — Apply & Log" color="#10B981">
            <SvgNode x={570} y={360} width={130} height={24} label="Merge to SilvaDB" sublabel="" style={NODE_STYLES.core} />
            <SvgNode x={720} y={360} width={130} height={24} label="Log to Ledger" sublabel="sync record" style={NODE_STYLES.subsystem} />
            <text x={880} y={372} fill="#10B981" fontSize="9" fontFamily="ui-monospace, monospace">
              ✓ resolved
            </text>
          </PhaseBox>

          {/* Flow arrows between steps */}
          <Connection x1={810} y1={125} x2={810} y2={140} markerId="fed-arrow-amber" strokeWidth={0.8} />
          <Connection x1={810} y1={210} x2={810} y2={225} markerId="fed-arrow-red" strokeWidth={0.8} />
          <Connection x1={810} y1={315} x2={810} y2={330} markerId="fed-arrow" strokeWidth={0.8} />

          {/* ═══════ MESH TOPOLOGY (Bottom) ═══════ */}
          <SectionLabel x={20} y={445} text="MESH TOPOLOGY" color="#F59E0B" />

          <PhaseBox x={20} y={458} width={1060} height={280} title="P2P Mesh Network — Noise Protocol Encrypted" color="#F59E0B">
            {/* Peer nodes in a mesh */}
            {/* Peer A (local - highlighted) */}
            <g>
              <circle cx={160} cy={570} r={40} fill="#0C1A1A" stroke="#14B8A6" strokeWidth={2} />
              <text x={160} y={565} textAnchor="middle" fill="#14B8A6" fontSize="12" fontFamily="ui-monospace, monospace" fontWeight={700}>A</text>
              <text x={160} y={580} textAnchor="middle" fill="#94A3B8" fontSize="8" fontFamily="ui-monospace, monospace">LOCAL</text>
              {/* Inner components */}
              <text x={160} y={596} textAnchor="middle" fill="#64748B" fontSize="7" fontFamily="ui-monospace">SilvaDB·FSRS</text>
            </g>

            {/* Peer B */}
            <g>
              <circle cx={400} cy={510} r={35} fill="#111827" stroke="#F59E0B" strokeWidth={1.5} />
              <text x={400} y={506} textAnchor="middle" fill="#F59E0B" fontSize="11" fontFamily="ui-monospace, monospace" fontWeight={600}>B</text>
              <text x={400} y={520} textAnchor="middle" fill="#64748B" fontSize="8" fontFamily="ui-monospace, monospace">clinic</text>
            </g>

            {/* Peer C */}
            <g>
              <circle cx={400} cy={640} r={35} fill="#111827" stroke="#F59E0B" strokeWidth={1.5} />
              <text x={400} y={636} textAnchor="middle" fill="#F59E0B" fontSize="11" fontFamily="ui-monospace, monospace" fontWeight={600}>C</text>
              <text x={400} y={650} textAnchor="middle" fill="#64748B" fontSize="8" fontFamily="ui-monospace, monospace">portable</text>
            </g>

            {/* Peer D */}
            <g>
              <circle cx={650} cy={510} r={35} fill="#111827" stroke="#F59E0B" strokeWidth={1.5} />
              <text x={650} y={506} textAnchor="middle" fill="#F59E0B" fontSize="11" fontFamily="ui-monospace, monospace" fontWeight={600}>D</text>
              <text x={650} y={520} textAnchor="middle" fill="#64748B" fontSize="8" fontFamily="ui-monospace, monospace">server</text>
            </g>

            {/* Peer E */}
            <g>
              <circle cx={650} cy={640} r={35} fill="#111827" stroke="#F59E0B" strokeWidth={1.5} />
              <text x={650} y={636} textAnchor="middle" fill="#F59E0B" fontSize="11" fontFamily="ui-monospace, monospace" fontWeight={600}>E</text>
              <text x={650} y={650} textAnchor="middle" fill="#64748B" fontSize="8" fontFamily="ui-monospace, monospace">portable</text>
            </g>

            {/* Peer F */}
            <g>
              <circle cx={880} cy={570} r={35} fill="#111827" stroke="#F59E0B" strokeWidth={1.5} />
              <text x={880} y={566} textAnchor="middle" fill="#F59E0B" fontSize="11" fontFamily="ui-monospace, monospace" fontWeight={600}>F</text>
              <text x={880} y={580} textAnchor="middle" fill="#64748B" fontSize="8" fontFamily="ui-monospace, monospace">server</text>
            </g>

            {/* Mesh connections */}
            <line x1={197} y1={555} x2={365} y2={515} stroke="#334155" strokeWidth={1} />
            <line x1={197} y1={585} x2={365} y2={635} stroke="#334155" strokeWidth={1} />
            <line x1={435} y1={500} x2={615} y2={500} stroke="#334155" strokeWidth={1} />
            <line x1={435} y1={520} x2={615} y2={640} stroke="#334155" strokeWidth={1} />
            <line x1={435} y1={650} x2={615} y2={650} stroke="#334155" strokeWidth={1} />
            <line x1={685} y1={500} x2={845} y2={560} stroke="#334155" strokeWidth={1} />
            <line x1={685} y1={640} x2={845} y2={580} stroke="#334155" strokeWidth={1} />
            <line x1={400} y1={545} x2={400} y2={605} stroke="#334155" strokeWidth={1} />
            <line x1={650} y1={545} x2={650} y2={605} stroke="#334155" strokeWidth={1} />

            {/* Active sync flow (highlighted) */}
            <line x1={197} y1={558} x2={365} y2={512} stroke="#14B8A6" strokeWidth={2} opacity={0.8} />
            <circle cx={280} cy={535} r={3} fill="#14B8A6">
              <animate attributeName="opacity" values="0.3;1;0.3" dur="2s" repeatCount="indefinite" />
            </circle>
            <text x={280} y={525} textAnchor="middle" fill="#14B8A6" fontSize="8" fontFamily="ui-monospace, monospace">
              sync: 3 mems
            </text>

            {/* Conflict happening */}
            <line x1={197} y1={582} x2={365} y2={638} stroke="#EF4444" strokeWidth={2} opacity={0.6} strokeDasharray="4 2" />
            <circle cx={280} cy={610} r={3} fill="#EF4444">
              <animate attributeName="opacity" values="0.3;1;0.3" dur="1.5s" repeatCount="indefinite" />
            </circle>
            <text x={280} y={625} textAnchor="middle" fill="#EF4444" fontSize="8" fontFamily="ui-monospace, monospace">
              conflict: SH-resolve
            </text>

            {/* Discovery label */}
            <text x={100} y={720} fill="#64748B" fontSize="9" fontFamily="ui-monospace, monospace">
              Discovery: mDNS (LAN) · DHT (WAN)
            </text>
            <text x={350} y={720} fill="#64748B" fontSize="9" fontFamily="ui-monospace, monospace">
              Encryption: Noise Protocol (authenticated, encrypted, zero-knowledge)
            </text>
            <text x={680} y={720} fill="#64748B" fontSize="9" fontFamily="ui-monospace, monospace">
              Future: KNEXA-FL LinUCB matchmaking
            </text>
          </PhaseBox>
        </svg>
      </motion.div>
    </div>
  );
}