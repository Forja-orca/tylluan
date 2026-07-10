'use client';

import { motion } from 'framer-motion';
import { ArrowMarker, SvgNode, Connection, PhaseBox, SectionLabel, NODE_STYLES } from './shared';

export function RetrievalPipeline() {
  return (
    <div className="space-y-4">
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: 0.6 }}
        className="w-full overflow-x-auto rounded-xl border border-border/50 bg-[#0A0F1A] p-4"
      >
        <svg viewBox="0 0 1100 680" className="w-full h-auto min-w-[800px]">
          <ArrowMarker id="rp-arrow" color="#14B8A6" />
          <ArrowMarker id="rp-arrow-blue" color="#3B82F6" />
          <ArrowMarker id="rp-arrow-purple" color="#A855F7" />
          <ArrowMarker id="rp-arrow-amber" color="#F59E0B" />
          <ArrowMarker id="rp-arrow-gray" color="#64748B" />

          {/* ═══════ INPUT ═══════ */}
          <SectionLabel x={30} y={32} text="INPUT" color="#94A3B8" />
          <SvgNode x={100} y={20} width={200} height={42} label="User Query" sublabel="natural language" style={NODE_STYLES.subsystem} />

          {/* Embedding step */}
          <Connection x1={200} y1={62} x2={200} y2={85} markerId="rp-arrow-gray" label="embed" />
          <SvgNode x={120} y={85} width={160} height={32} label="BGE-M3 Encode" sublabel="1024d embedding" style={{ fill: '#0F1520', stroke: '#3B82F6', strokeWidth: 1 }} />

          {/* ═══════ THREE PARALLEL RETRIEVAL PATHS ═══════ */}
          <SectionLabel x={30} y={142} text="PARALLEL RETRIEVAL (3 paths)" color="#14B8A6" />

          {/* Path 1: BM25 (Lexical) */}
          <PhaseBox x={20} y={155} width={330} height={130} title="Path 1 — BM25 (Lexical)" color="#14B8A6">
            <SvgNode x={40} y={195} width={120} height={30} label="Tokenizer" sublabel="language-aware" style={NODE_STYLES.core} />
            <SvgNode x={40} y={235} width={120} height={30} label="BM25 Score" sublabel="TF-IDF variant" style={NODE_STYLES.core} />
            <Connection x1={100} y1={225} x2={100} y2={235} markerId="rp-arrow" strokeWidth={0.8} />
            <text x={200} y={200} fill="#64748B" fontSize="9" fontFamily="ui-monospace, monospace">
              Keyword matching
            </text>
            <text x={200} y={214} fill="#64748B" fontSize="9" fontFamily="ui-monospace, monospace">
              Good for: proper nouns,
            </text>
            <text x={200} y={228} fill="#64748B" fontSize="9" fontFamily="ui-monospace, monospace">
              IDs, exact terms
            </text>
            <text x={200} y={254} fill="#475569" fontSize="8" fontFamily="ui-monospace, monospace">
              Returns: ranked list [node_id, score]
            </text>
          </PhaseBox>

          {/* Path 2: BGE-M3 (Semantic) */}
          <PhaseBox x={380} y={155} width={330} height={130} title="Path 2 — BGE-M3 (Semantic)" color="#3B82F6">
            <SvgNode x={400} y={195} width={120} height={30} label="HNSW Search" sublabel="cosine similarity" style={{ fill: '#0F1520', stroke: '#3B82F6', strokeWidth: 1 }} />
            <SvgNode x={400} y={235} width={120} height={30} label="Re-score" sublabel="normalize" style={{ fill: '#0F1520', stroke: '#3B82F6', strokeWidth: 1 }} />
            <Connection x1={460} y1={225} x2={460} y2={235} markerId="rp-arrow-blue" strokeWidth={0.8} />
            <text x={560} y={200} fill="#64748B" fontSize="9" fontFamily="ui-monospace, monospace">
              Dense vector search
            </text>
            <text x={560} y={214} fill="#64748B" fontSize="9" fontFamily="ui-monospace, monospace">
              Good for: meaning,
            </text>
            <text x={560} y={228} fill="#64748B" fontSize="9" fontFamily="ui-monospace, monospace">
              concepts, paraphrases
            </text>
            <text x={560} y={254} fill="#475569" fontSize="8" fontFamily="ui-monospace, monospace">
              Returns: ranked list [node_id, score]
            </text>
          </PhaseBox>

          {/* Path 3: Graph PageRank (Relational) */}
          <PhaseBox x={740} y={155} width={330} height={130} title="Path 3 — PageRank (Graph)" color="#A855F7">
            <SvgNode x={760} y={195} width={130} height={30} label="Graph Walk" sublabel="edge traversal" style={NODE_STYLES.process} />
            <SvgNode x={760} y={235} width={130} height={30} label="PageRank Score" sublabel="degree penalty" style={NODE_STYLES.process} />
            <Connection x1={825} y1={225} x2={825} y2={235} markerId="rp-arrow-purple" strokeWidth={0.8} />
            <text x={920} y={200} fill="#64748B" fontSize="9" fontFamily="ui-monospace, monospace">
              Graph structure
            </text>
            <text x={920} y={214} fill="#64748B" fontSize="9" fontFamily="ui-monospace, monospace">
              Good for: related
            </text>
            <text x={920} y={228} fill="#64748B" fontSize="9" fontFamily="ui-monospace, monospace">
              concepts, context
            </text>
            <text x={920} y={254} fill="#475569" fontSize="8" fontFamily="ui-monospace, monospace">
              Returns: ranked list [node_id, score]
            </text>
          </PhaseBox>

          {/* Arrows from embedding to each path */}
          <Connection x1={160} y1={117} x2={185} y2={155} markerId="rp-arrow" label="tokens" strokeWidth={0.8} />
          <Connection x1={200} y1={117} x2={545} y2={155} markerId="rp-arrow-blue" label="query vec" strokeWidth={0.8} />
          <Connection x1={240} y1={117} x2={905} y2={155} markerId="rp-arrow-purple" label="seed nodes" strokeWidth={0.8} />

          {/* ═══════ FUTURE: HippoRAG-PPR ═══════ */}
          <PhaseBox x={740} y={300} width={330} height={90} title="FUTURE: Path 4 — HippoRAG-PPR (Associative)" color="#F59E0B">
            <SvgNode x={760} y={335} width={140} height={30} label="Personalized PageRank" sublabel="query-seeded diffusion" style={{ fill: '#1A1510', stroke: '#F59E0B', strokeWidth: 1, strokeDasharray: '3 2' } as any } />
            <text x={920} y={340} fill="#F59E0B" fontSize="9" fontFamily="ui-monospace, monospace" opacity={0.7}>
              Recupera por
            </text>
            <text x={920} y={354} fill="#F59E0B" fontSize="9" fontFamily="ui-monospace, monospace" opacity={0.7}>
              asociación, no similitud
            </text>
          </PhaseBox>

          {/* ═══════ FUSION ═══════ */}
          <SectionLabel x={30} y={410} text="FUSION" color="#14B8A6" />

          {/* Convergence arrows */}
          <Connection x1={185} y1={285} x2={430} y2={440} markerId="rp-arrow" label="list₁" strokeWidth={0.8} />
          <Connection x1={545} y1={285} x2={500} y2={440} markerId="rp-arrow-blue" label="list₂" strokeWidth={0.8} />
          <Connection x1={905} y1={285} x2={570} y2={440} markerId="rp-arrow-purple" label="list₃" strokeWidth={0.8} />
          <Connection x1={905} y1={390} x2={600} y2={440} color="#F59E0B" markerId="rp-arrow-amber" label="list₄" dashed strokeWidth={0.8} />

          {/* RRF Fusion Box */}
          <PhaseBox x={300} y={425} width={500} height={100} title="Reciprocal Rank Fusion (RRF)" color="#14B8A6">
            {/* Formula */}
            <text x={550} y={462} textAnchor="middle" fill="#14B8A6" fontSize="13" fontFamily="ui-monospace, monospace" fontWeight={600}>
              score(d) = Σ 1 / (k + rankᵢ(d))
            </text>
            <text x={550} y={480} textAnchor="middle" fill="#64748B" fontSize="10" fontFamily="ui-monospace, monospace">
              k = 60 (default)  ·  i = 1..N retrieval paths
            </text>
            <SvgNode x={450} y={490} width={200} height={28} label="Ranked Results" sublabel="unified score per node" style={{ fill: '#0C1A1A', stroke: '#14B8A6', strokeWidth: 1.5 }} />
          </PhaseBox>

          {/* ═══════ FSRS WEIGHTING ═══════ */}
          <SectionLabel x={30} y={545} text="FSRS WEIGHTING" color="#10B981" />
          <Connection x1={550} y1={525} x2={550} y2={555} markerId="rp-arrow" label="apply decay" strokeWidth={0.8} />

          <PhaseBox x={250} y={555} width={600} height={55} title="Final Retrieval Score" color="#10B981">
            <text x={550} y={588} textAnchor="middle" fill="#10B981" fontSize="12" fontFamily="ui-monospace, monospace">
              final_score(node) = RRF_score × R(node)
            </text>
            <text x={550} y={602} textAnchor="middle" fill="#64748B" fontSize="9" fontFamily="ui-monospace, monospace">
              R(node) = 2^(-Δt / stability)  ·  Low retrievability = lower rank
            </text>
          </PhaseBox>

          {/* ═══════ OUTPUT ═══════ */}
          <Connection x1={550} y1={610} x2={550} y2={635} markerId="rp-arrow" />
          <SvgNode x={420} y={635} width={260} height={38} label="Top-K Results" sublabel="with decomposed scores" style={{ fill: '#0C1A1A', stroke: '#14B8A6', strokeWidth: 1.5 }} />
        </svg>
      </motion.div>

      {/* Key insights */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        <div className="rounded-lg border border-border/50 bg-surface/50 p-4">
          <h4 className="text-xs font-mono font-semibold text-teal-400 mb-2 uppercase tracking-wider">Qué recupera cada path</h4>
          <div className="space-y-2 text-xs text-muted-foreground">
            <div className="flex gap-2 items-start">
              <span className="text-teal-400 font-mono mt-0.5">BM25</span>
              <span>El nombre &quot;Juan&quot; en una conversación de hace 6 meses — el vector quizás no lo captura, pero el token exacto sí.</span>
            </div>
            <div className="flex gap-2 items-start">
              <span className="text-blue-400 font-mono mt-0.5">BGE-M3</span>
              <span>Todo lo relacionado con &quot;el proyecto que aplazamos&quot; aunque nadie dijo exactamente esas palabras.</span>
            </div>
            <div className="flex gap-2 items-start">
              <span className="text-purple-400 font-mono mt-0.5">PageRank</span>
              <span>La persona más mencionada en el grafo, o el concepto con más conexiones al contexto de la query.</span>
            </div>
            <div className="flex gap-2 items-start">
              <span className="text-amber-400 font-mono mt-0.5">PPR (future)</span>
              <span>La reunión con Ana porque mencionaste &quot;el proyecto&quot; — asociación, no similitud léxica ni semántica.</span>
            </div>
          </div>
        </div>
        <div className="rounded-lg border border-border/50 bg-surface/50 p-4">
          <h4 className="text-xs font-mono font-semibold text-amber-400 mb-2 uppercase tracking-wider">Por qué RRF y no加权平均</h4>
          <div className="space-y-2 text-xs text-muted-foreground">
            <p>RRF es <strong className="text-slate-300">rank-based</strong>, no score-based. Esto significa:</p>
            <ul className="list-disc list-inside space-y-1 ml-2">
              <li>No necesita normalizar scores entre paths (BM25 y cosine similarity tienen escalas diferentes)</li>
              <li>Un nodo que es top-1 en un solo path vence a uno que es top-5 en tres paths — prioriza señales fuertes</li>
              <li>k=60 suaviza la contribución de ranks altos sin necesitar tuning</li>
              <li>Añadir un cuarto path (HippoRAG-PPR) es trivial: una lista más en la suma</li>
            </ul>
          </div>
        </div>
      </div>
    </div>
  );
}