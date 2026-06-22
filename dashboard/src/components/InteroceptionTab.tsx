import React, { useState, useEffect, useRef, useCallback } from 'react';
import { 
  Zap, 
  Activity, 
  Clock 
} from 'lucide-react';
import { useNexus } from '../hooks/useNexus';
import { cn } from '../lib/utils';
import type { 
  Interoception, 
  HormoneAmbient, 
  Guild,
  AgentProfile
} from '../lib/nexus-bridge';
import type { MemoryStats } from '../hooks/useNexus';

function Sparkline({ data, color, height = 16 }: { data: number[], color: string, height?: number }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || data.length < 2) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    const w = canvas.width, h = canvas.height;
    ctx.clearRect(0, 0, w, h);
    const max = Math.max(...data, 1);
    const pts = data.map((v, i) => [i / (data.length - 1) * w, h - (v / max) * h * 0.9]);
    ctx.beginPath();
    ctx.moveTo(pts[0][0], pts[0][1]);
    pts.slice(1).forEach(([x, y]) => ctx.lineTo(x, y));
    ctx.strokeStyle = color;
    ctx.lineWidth = 1.5;
    ctx.stroke();
  }, [data, color]);
  return <canvas ref={canvasRef} width={120} height={height} className="opacity-80" />;
}

function ForestStatus({ interoception, guilds }: { interoception: Interoception | null, guilds: Guild[] }) {
  const { bridge } = useNexus();
  const [silva, setSilva] = useState<{ node_count?: number; edge_count?: number } | null>(null);
  const edgeHistory = useRef<number[]>([]);

  useEffect(() => {
    if (!bridge) return;
    const poll = async () => {
      try {
        const stats = await bridge.getMemoryStats();
        setSilva(stats);
      } catch (e) {
        console.error('ForestStatus polling failed:', e);
      }
    };
    poll();
    // Poll memory stats every 60s (not available via SSE in useNexus)
    const interval = setInterval(poll, 60000);
    return () => clearInterval(interval);
  }, [bridge]);

  if (silva?.edge_count !== undefined) {
    const h = edgeHistory.current;
    if (h.length === 0 || h[h.length - 1] !== silva.edge_count) {
      h.push(silva.edge_count);
      if (h.length > 10) h.shift();
    }
  }

  const narrativeLines: string[] = [];
  if (!interoception) {
    narrativeLines.push('The forest is dormant. No interoception data.');
  } else {
    const { homeostasis, stress_level, knowledge_hunger, active_pheromones } = interoception;
    const nodes = silva?.node_count ?? 0;
    const edges = silva?.edge_count ?? 0;

    if (homeostasis > 0.75) narrativeLines.push('The forest flourishes — high homeostasis, deep roots.');
    else if (homeostasis > 0.45) narrativeLines.push('The forest breathes slowly — moderate homeostasis.');
    else narrativeLines.push('The forest is under strain — critical homeostasis.');

    if (stress_level > 0.7) narrativeLines.push(`⚠️ Elevated cortisol (${(stress_level * 100).toFixed(0)}%). Several recent failed attempts.`);
    else if (stress_level < 0.2) narrativeLines.push('No active stress signals.');

    if (knowledge_hunger > 0.6) narrativeLines.push(`Cognitive hunger active (${(knowledge_hunger * 100).toFixed(0)}%) — the forest seeks new connections.`);
    
    if ((interoception?.graph_density ?? 0) < 0.001) narrativeLines.push("The forest needs connections — use tylluan_remember with varied intents");

    if (active_pheromones > 3) narrativeLines.push(`${active_pheromones} active pheromones — high hormonal activity.`);
    
    // Git Guild Monitor
    const gitGuild = guilds?.find(g => g.name === 'git');
    if (gitGuild) {
      if (!gitGuild.running) narrativeLines.push('⚠️ Guild git OFFLINE — version control commands disabled.');
      else if ((gitGuild.restarts_5m ?? 0) > 0) narrativeLines.push(`🔄 Guild git unstable: ${gitGuild.restarts_5m} recent restarts.`);
    }

    narrativeLines.push(`Network: ${nodes} nodes · ${edges} edges · density ${(interoception?.graph_density ?? 0).toFixed(4)}`);
  }

  const densityVal = interoception?.graph_density ?? 0;
  const densityPercent = Math.min(100, (densityVal / 0.01) * 100);

  return (
    <div className="rounded-xl border border-slate-800 bg-slate-900/50 p-4 space-y-3">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <span className="text-green-400 text-sm">🌳</span>
          <span className="text-xs font-bold uppercase tracking-widest text-slate-400">Estado del Bosque Cognitivo</span>
        </div>
        {edgeHistory.current.length >= 2 && (
          <Sparkline data={edgeHistory.current} color="#6ee7b7" />
        )}
      </div>
      <div className="space-y-1">
        {narrativeLines.map((line, i) => (
          <p key={i} className="text-xs font-mono text-slate-300 leading-relaxed">{line}</p>
        ))}
      </div>

      <div className="mt-2 pt-2 border-t border-slate-800/50">
        <div className="flex justify-between text-[8px] uppercase text-slate-500 mb-1 font-bold tracking-tighter">
          <span>Densidad del Bosque</span>
          <span>{densityVal.toFixed(4)} / 0.0100</span>
        </div>
        <div className="h-1 bg-slate-800 rounded-full overflow-hidden">
          <div 
            className="h-full bg-emerald-500/50 transition-all duration-1000 shadow-[0_0_8px_rgba(16,185,129,0.3)]" 
            style={{ width: `${densityPercent}%` }} 
          />
        </div>
      </div>
    </div>
  );
}

interface Props {
  interoception: Interoception | null;
  memoryStats: MemoryStats | null;
}

export function InteroceptionTab({ interoception, memoryStats }: Props) {
  const { bridge, guilds } = useNexus();
  const [hormones, setHormones] = useState<HormoneAmbient | null>(null);
  const [profiles, setProfiles] = useState<AgentProfile[]>([]);

  useEffect(() => {
    if (!bridge) return;
    const loadProfiles = async () => {
      try {
        const p = await bridge.getAgentProfiles();
        setProfiles(p);
      } catch (e) {
        console.error('Failed to load agent profiles in InteroceptionTab:', e);
      }
    };
    loadProfiles();
    const interval = setInterval(loadProfiles, 30000);
    return () => clearInterval(interval);
  }, [bridge]);

  useEffect(() => {
    if (!bridge) return;
    const poll = async () => {
      if (document.visibilityState === 'hidden') return;
      try {
        const h = await bridge.getHormones();
        setHormones(h);
      } catch (e) {
        console.error('Failed to poll hormones:', e);
      }
    };
    poll();
    const interval = setInterval(poll, 30000);
    return () => clearInterval(interval);
  }, [bridge]);

  const HormoneBar = ({ label, value, color }: { label: string, value: number, color: string }) => (
    <div className="space-y-1">
      <div className="flex justify-between text-[10px] font-mono">
        <span className="text-slate-400">{label}</span>
        <span style={{ color }}>{(value * 100).toFixed(0)}%</span>
      </div>
      <div className="h-1.5 w-full bg-slate-800 rounded-full overflow-hidden border border-slate-700/30">
        <div 
          className="h-full transition-all duration-1000" 
          style={{ width: `${value * 100}%`, backgroundColor: color, boxShadow: `0 0 10px ${color}40` }}
        />
      </div>
    </div>
  );

  return (
    <div className="space-y-6">
      <ForestStatus interoception={interoception} guilds={guilds} />
      {/* Hormone Dashboard */}
      <div className="rounded-xl border border-slate-800 bg-slate-900/50 p-4 shadow-lg">
        <div className="flex items-center gap-2 mb-4">
          <Zap className="w-4 h-4 text-amber-400" />
          <span className="text-xs font-bold uppercase tracking-widest text-slate-400">Biological Signal Ambient</span>
        </div>
        <div className="grid grid-cols-1 md:grid-cols-5 gap-4">
          <HormoneBar label="Stress (Cortisol)" value={hormones?.stress ?? 0} color="#ef4444" />
          <HormoneBar label="Novelty (Dopamine)" value={hormones?.novelty ?? 0} color="#22c55e" />
          <HormoneBar label="Saturation (Serotonin)" value={hormones?.saturation ?? 0} color="#eab308" />
          <HormoneBar label="Energy (ATP)" value={hormones?.energy ?? 0} color="#60a5fa" />
          <HormoneBar label="Homeostasis" value={hormones?.homeostasis ?? 0} color="#a78bfa" />
        </div>
      </div>

      <div className="grid grid-cols-2 lg:grid-cols-4 gap-3">
        <div className="rounded-lg border border-slate-800 bg-slate-900/50 p-3">
          <div className="text-[10px] text-slate-500 uppercase tracking-widest mb-1">Homeostasis</div>
          <div className="text-2xl font-bold font-mono" style={{ color: interoception ? (interoception.homeostasis > 0.7 ? '#34d399' : interoception.homeostasis > 0.4 ? '#fbbf24' : '#ef4444') : '#64748b' }}>
            {interoception ? `${(interoception.homeostasis * 100).toFixed(0)}%` : '—'}
          </div>
        </div>
        <div className="rounded-lg border border-slate-800 bg-slate-900/50 p-3">
          <div className="text-[10px] text-slate-500 uppercase tracking-widest mb-1">Stress</div>
          <div className="text-2xl font-bold font-mono" style={{ color: interoception ? (interoception.stress_level < 0.3 ? '#34d399' : interoception.stress_level < 0.6 ? '#fbbf24' : '#ef4444') : '#64748b' }}>
            {interoception ? `${(interoception.stress_level * 100).toFixed(0)}%` : '—'}
          </div>
        </div>
        <div className="rounded-lg border border-slate-800 bg-slate-900/50 p-3">
          <div className="text-[10px] text-slate-500 uppercase tracking-widest mb-1">Knowledge Hunger</div>
          <div className="text-2xl font-bold font-mono" style={{ color: interoception ? (interoception.knowledge_hunger > 0.7 ? '#fbbf24' : '#34d399') : '#64748b' }}>
            {interoception ? `${(interoception.knowledge_hunger * 100).toFixed(0)}%` : '—'}
          </div>
        </div>
        <div className="rounded-lg border border-slate-800 bg-slate-900/50 p-3">
          <div className="text-[10px] text-slate-500 uppercase tracking-widest mb-1">Graph Density</div>
          <div className="text-2xl font-bold font-mono text-slate-300">
            {interoception ? interoception.graph_density.toFixed(4) : '—'}
          </div>
        </div>
      </div>

      <div className="grid grid-cols-2 gap-3">
        <div className="rounded-lg border border-slate-800 bg-slate-900/50 p-3">
          <div className="text-[10px] text-slate-500 uppercase tracking-widest mb-2">Active Pheromones</div>
          <div className="text-sm font-mono text-emerald-400">
            {interoception ? `${interoception.active_pheromones} signals` : '—'}
          </div>
        </div>
      </div>

      {interoception?.recommendations && interoception.recommendations.length > 0 && (
        <div className="rounded-lg border border-slate-800 bg-slate-900/50 p-3">
          <div className="text-[10px] text-slate-500 uppercase tracking-widest mb-2">Recommendations</div>
          <ul className="space-y-1">
            {interoception.recommendations.map((rec: string, i: number) => (
              <li key={i} className="text-xs text-amber-400 font-mono">• {rec}</li>
            ))}
          </ul>
        </div>
      )}

      {/* Agent Metabolic & Allocation Matrix */}
      {profiles.length > 0 && (
        <div className="rounded-xl border border-slate-800 bg-slate-900/50 p-4 space-y-4 shadow-lg animate-in fade-in duration-500">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <span className="text-violet-400 text-sm">🧬</span>
              <span className="text-xs font-bold uppercase tracking-widest text-slate-400">Matriz Metabólica y Asignación de Agentes</span>
            </div>
            <span className="text-[9px] font-mono text-slate-500 uppercase">Eficiencia del Colectivo</span>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            {profiles.map((p) => {
              const aid = p.agent_id.toLowerCase();
              let costTier = "Low Power (Local)";
              let costColor = "text-emerald-400";
              let description = "Local inference / Rapid execution";
              
              if (aid.includes("cloud-high") || aid.includes("claude") || aid.includes("sonnet") || aid.includes("gpt4")) {
                costTier = "High Power (Cloud)";
                costColor = "text-red-400";
                description = "High capacity model · Complex reasoning & architecture";
              } else if (aid.includes("cloud-medium") || aid.includes("gemini") || aid.includes("flash")) {
                costTier = "Medium Power (Cloud)";
                costColor = "text-amber-400";
                description = "Balanced model · UI layout & visual editing";
              } else if (aid.includes("local") || aid.includes("llama")) {
                costTier = "Low Power (Local/API)";
                costColor = "text-emerald-400";
                description = "Local model · Rapid utility execution & web search";
              } else if (aid.includes("bg") || aid.includes("background")) {
                costTier = "Background (Local)";
                costColor = "text-cyan-400";
                description = "Background task agent · Autonomous batch execution";
              }

              return (
                <div key={p.agent_id} className="p-3 rounded-lg bg-slate-950/40 border border-slate-800/80 space-y-3">
                  <div className="flex items-start justify-between">
                    <div>
                      <h5 className="text-xs font-bold text-slate-200">{p.agent_id}</h5>
                      <p className="text-[9px] text-slate-500 font-mono mt-0.5">{description}</p>
                    </div>
                    <span className={cn("text-[8px] font-mono uppercase px-1.5 py-0.5 rounded bg-slate-900 border border-slate-800", costColor)}>
                      {costTier}
                    </span>
                  </div>

                  <div className="flex justify-between items-center text-[10px] text-slate-400">
                    <span>Llamadas totales:</span>
                    <span className="font-mono text-slate-200 font-bold">{p.total_calls}</span>
                  </div>

                  {Object.keys(p.competencies).length > 0 && (
                    <div className="space-y-1.5 pt-2 border-t border-slate-800/60">
                      <p className="text-[8px] text-slate-500 uppercase font-bold tracking-widest">Competencia por Dominio</p>
                      <div className="grid grid-cols-2 gap-x-3 gap-y-1.5">
                        {Object.entries(p.competencies).map(([guild, val]) => (
                          <div key={guild} className="space-y-0.5">
                            <div className="flex justify-between text-[8px] font-mono">
                              <span className="text-slate-500 uppercase truncate max-w-[80px]" title={guild}>{guild}</span>
                              <span className="text-cyan-400/80">{(val * 100).toFixed(0)}%</span>
                            </div>
                            <div className="h-1 bg-slate-900 rounded-full overflow-hidden">
                              <div className="h-full bg-cyan-500/50" style={{ width: `${val * 100}%` }} />
                            </div>
                          </div>
                        ))}
                      </div>
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      )}

      {/* Agent rhythms */}
      {interoception && Object.keys(interoception.agent_rhythms ?? {}).length > 0 && (
        <div className="rounded-xl border border-slate-800 bg-slate-900/50 p-4">
          <div className="text-xs font-bold uppercase tracking-widest text-slate-400 mb-3">Ritmos de agentes</div>
          <div className="space-y-2">
            {Object.entries(interoception.agent_rhythms).map(([agentId, rhythm]) => (
              <div key={agentId} className="flex items-center gap-3 text-xs">
                <span className="font-mono text-slate-300 w-32 truncate">{agentId}</span>
                <span className="px-2 py-0.5 rounded text-[10px] bg-slate-800 text-slate-400">
                  {rhythm.client}
                </span>
                <span className="text-slate-400 font-mono">
                  {rhythm.tool_calls} calls
                </span>
                <span className="text-slate-500 ml-auto">
                  {Math.floor((rhythm.last_active_secs_ago ?? 0) / 60)}m ago
                </span>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Capabilities row */}
      <div className="flex gap-3 mt-4">
        <div className={cn(
          "flex items-center gap-2 px-3 py-1.5 rounded-full text-[10px] font-bold border",
          interoception?.capabilities?.embeddings_loaded
            ? "bg-emerald-500/10 border-emerald-500/30 text-emerald-400"
            : "bg-slate-800 border-slate-700 text-slate-500"
        )}>
          <div className={cn("w-1.5 h-1.5 rounded-full", interoception?.capabilities?.embeddings_loaded ? "bg-emerald-500" : "bg-slate-600")} />
          {interoception?.capabilities?.embedding_model ?? 'embeddings'} 
        </div>
        <div className={cn(
          "flex items-center gap-2 px-3 py-1.5 rounded-full text-[10px] font-bold border",
          interoception?.capabilities?.reranker_loaded
            ? "bg-violet-500/10 border-violet-500/30 text-violet-400"
            : "bg-slate-800 border-slate-700 text-slate-500"
        )}>
          <div className={cn("w-1.5 h-1.5 rounded-full", interoception?.capabilities?.reranker_loaded ? "bg-violet-500" : "bg-slate-600")} />
          {interoception?.capabilities?.reranker_loaded ? interoception.capabilities.reranker_model : 'reranker offline'}
        </div>

        {interoception?.tunnel && (
          <div className="flex items-center gap-2">
            <span className={cn(
              "px-2 py-1 rounded-full text-[10px] font-bold border",
              interoception.tunnel.wsl_bridge_active
                ? "bg-blue-500/10 text-blue-400 border-blue-500/20"
                : "bg-slate-700/50 text-slate-500 border-slate-700"
            )}>
              {interoception.tunnel.wsl_bridge_active ? "🌉 WSL Bridge" : "WSL Bridge OFF"}
            </span>
            {interoception.tunnel.wsl_url && (
              <span className="text-[9px] font-mono text-slate-500 truncate max-w-[200px]"
                    title={interoception.tunnel.wsl_url}>
                {interoception.tunnel.wsl_url}
              </span>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
