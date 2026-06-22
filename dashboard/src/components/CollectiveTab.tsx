import React, { useState, useEffect, useRef, useCallback, useMemo } from 'react';
import {
  User,
  Users,
  RefreshCw,
  Zap,
  X,
  FileText,
  BookOpen,
  Trash2,
  WifiOff,
  MessageSquare,
  Send,
  Plus,
  ChevronDown
} from 'lucide-react';
import { useNexus } from '../hooks/useNexus';
import type { 
  NexusEvent, 
  AgentMemory, 
  AgentMemorySummary, 
  AgentProfile 
} from '../lib/nexus-bridge';
import { cn } from '../lib/utils';
import { SessionsTab } from './SessionsTab';

// --- Types ---
interface CollectivePulse {
  active_agents: string[];
  active_count: number;
  broadcasts_last_hour: number;
  graph: { nodes: number; edges: number };
  ts: string;
}

interface HeatmapEntry {
  node_id: string;
  touches: number;
}

interface TimelineEvent {
  id: string;
  type: string;
  content: string;
  weight: number;
  updated_at: string | null;
}

const AGENT_COLORS = ['#60a5fa','#34d399','#fbbf24','#f87171','#a78bfa','#fb923c','#22d3ee','#e879f9'];

// --- Sub-component: RealtimeAgentsTab ---
function RealtimeAgentsTab({ notify }: { notify: (msg: string, type?: 'info' | 'error') => void }) {
  const { sessions, events, bridge } = useNexus();

  const [activity, setActivity] = useState<Record<string, { tool: string; intent?: string }>>({});
  const [selectedAgentId, setSelectedAgentId] = useState<string | null>(null);
  const [memories, setMemories] = useState<AgentMemory[]>([]);
  const [summary, setSummary] = useState<AgentMemorySummary | null>(null);
  const [profiles, setProfiles] = useState<AgentProfile[]>([]);
  const [loadingMemory, setLoadingMemory] = useState(false);
  const [identity, setIdentity] = useState<any>(null);
  const [askIdentityResponse, setAskIdentityResponse] = useState<string | null>(null);
  const [askingIdentity, setAskingIdentity] = useState(false);

  useEffect(() => {
    if (!bridge) return;
    const load = () => {
      if (document.visibilityState === 'hidden') return;
      bridge.getAgentProfiles().then(setProfiles).catch(console.error);
    };
    load();
    const interval = setInterval(load, 30000);
    const onVisibility = () => { if (document.visibilityState === 'visible') load(); };
    document.addEventListener('visibilitychange', onVisibility);
    return () => {
      clearInterval(interval);
      document.removeEventListener('visibilitychange', onVisibility);
    };
  }, [bridge]);

  useEffect(() => {
    if (!bridge || !selectedAgentId) {
      setMemories([]);
      setSummary(null);
      setAskIdentityResponse(null);
      return;
    }
    
    const poll = async () => {
      try {
        const results = await Promise.allSettled([
          bridge.getAgentMemories(selectedAgentId),
          bridge.getAgentMemorySummary(selectedAgentId)
        ]);
        const m = results[0].status === 'fulfilled' ? results[0].value : [];
        const s = results[1].status === 'fulfilled' ? results[1].value : null;
        setMemories(m.slice(0, 20));
        setSummary(s);
      } catch (e) {
        console.error('Agent memory polling failed:', e);
      }
    };

    setLoadingMemory(true);
    poll().finally(() => setLoadingMemory(false));
    
    // Poll agent memory data every 60s (agent-specific endpoints not available via SSE)
    const interval = setInterval(poll, 60000);
    return () => clearInterval(interval);
  }, [selectedAgentId, bridge]);

  useEffect(() => {
    if (!bridge || !selectedAgentId) {
      setIdentity(null);
      return;
    }
    bridge.getAgentIdentity(selectedAgentId).then(setIdentity).catch(() => setIdentity(null));
  }, [selectedAgentId, bridge]);

  const handleDeleteMemories = async (agentId: string) => {
    if (!bridge || !confirm(`¿Estás seguro de que deseas que el sistema olvide todo sobre ${agentId}? Esta acción es irreversible.`)) return;
    try {
      await bridge.deleteAgentMemories(agentId);
      notify(`Memoria de ${agentId} eliminada`, 'info');
      setSelectedAgentId(null);
    } catch (e) {
      notify(`Error deleting memory: ${e}`, 'error');
    }
  };

  const handleAskIdentity = async () => {
    if (!bridge || !selectedAgentId) return;
    setAskingIdentity(true);
    setAskIdentityResponse(null);
    try {
      const res = await bridge.fetchRaw('/api/v1/do', {
        method: 'POST',
        body: JSON.stringify({ 
          tool: 'tylluan_think', 
          query: 'quién soy', 
          agent_id: selectedAgentId 
        })
      });
      const text = res?.result || res?.response || (Array.isArray(res?.content) ? res.content[0]?.text : null) || JSON.stringify(res);
      setAskIdentityResponse(text);
    } catch (e) {
      notify('Error al consultar identidad', 'error');
    } finally {
      setAskingIdentity(false);
    }
  };

  useEffect(() => {
    const latestEvent = events[0];
    if (latestEvent?.type === 'tool_call') {
      const { agent_id, tool, status, intent } = latestEvent.data;
      if (status === 'started') {
        setActivity(prev => ({ ...prev, [agent_id]: { tool, intent } }));
      } else {
        setTimeout(() => {
          setActivity(prev => {
            const next = { ...prev };
            delete next[agent_id];
            return next;
          });
        }, 3000);
      }
    }
  }, [events]);

  const formatUptime = (secs: number) => {
    if (secs < 60) return `${secs}s`;
    if (secs < 3600) return `${Math.floor(secs / 60)}m`;
    return `${Math.floor(secs / 3600)}h ${Math.floor((secs % 3600) / 60)}m`;
  };

  const agentColor = (id: string) => {
    const colors = ['emerald', 'violet', 'sky', 'amber', 'rose', 'teal'];
    const idx = id.split('').reduce((a, c) => a + c.charCodeAt(0), 0) % colors.length;
    return colors[idx];
  };

  const colorMap: Record<string, { bg: string; border: string; text: string; ring: string }> = {
    emerald: { bg: 'bg-emerald-500/10', border: 'border-emerald-500/30', text: 'text-emerald-400', ring: 'ring-emerald-500/40' },
    violet:  { bg: 'bg-violet-500/10',  border: 'border-violet-500/30',  text: 'text-violet-400',  ring: 'ring-violet-500/40' },
    sky:     { bg: 'bg-sky-500/10',     border: 'border-sky-500/30',     text: 'text-sky-400',     ring: 'ring-sky-500/40' },
    amber:   { bg: 'bg-amber-500/10',   border: 'border-amber-500/30',   text: 'text-amber-400',   ring: 'ring-amber-500/40' },
    rose:    { bg: 'bg-rose-500/10',    border: 'border-rose-500/30',    text: 'text-rose-400',    ring: 'ring-rose-500/40' },
    teal:    { bg: 'bg-teal-500/10',    border: 'border-teal-500/30',    text: 'text-teal-400',    ring: 'ring-teal-500/40' },
  };

  // Show empty state if no profiles have been registered
  if (!loadingMemory && profiles.length === 0 && sessions.length === 0) {
    return (
      <div className="py-12 rounded-2xl border-2 border-dashed border-slate-800 flex flex-col items-center justify-center text-center bg-slate-900/20">
        <div className="w-16 h-16 rounded-full bg-slate-900 border border-slate-800 flex items-center justify-center mb-4">
          <Users className="w-8 h-8 text-slate-700" />
        </div>
        <h4 className="text-sm font-bold text-slate-400">No Agent Profiles Registered</h4>
        <p className="text-xs text-slate-600 max-w-sm mt-2">
          Agent profiles appear here when agents connect via MCP with an agent_id. Connect your first agent to start building the collective knowledge graph.
        </p>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <span className="text-xs font-bold uppercase tracking-widest text-slate-500">Connected Agents</span>
          <p className="text-[10px] text-slate-600 font-mono mt-0.5">MCP Protocol Bridge · HTTP Streamable + SSE</p>
        </div>
        <div className="flex items-center gap-3">
          <div className="flex -space-x-2">
            {(sessions || []).slice(0, 6).map((s) => {
              const agentLabel = s.agent_id ?? s.client_name ?? 'anon';
              const color = colorMap[agentColor(agentLabel)] ?? colorMap.emerald;
              return (
                <div key={s.id} className={`w-7 h-7 rounded-full border-2 border-slate-950 ${color.bg} flex items-center justify-center text-[9px] font-bold ${color.text} ring-1 ${color.ring}`}>
                  {agentLabel.charAt(0).toUpperCase()}
                </div>
              );
            })}
          </div>
          <div className="px-2 py-1 rounded-full bg-slate-800 border border-slate-700">
            <span className="text-[10px] text-slate-400 font-mono">{(sessions || []).length} online</span>
          </div>
        </div>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
        {sessions.map((session) => {
          const agentKey = session.agent_id ?? session.client_name ?? 'anon';
          const color = colorMap[agentColor(agentKey)] ?? colorMap.emerald;
          const isActive = agentKey ? !!activity[agentKey] : false;
          const act = agentKey ? activity[agentKey] : undefined;
          return (
            <div key={session.id} className={cn("group relative rounded-2xl border bg-slate-900/50 overflow-hidden transition-all duration-300 shadow-lg", isActive ? `${color.border} shadow-[0_0_20px_rgba(0,0,0,0.5)]` : 'border-slate-800 hover:border-slate-700')}>
              {isActive && (
                <div className={cn("absolute top-0 left-0 right-0 h-0.5 animate-pulse", color.text.replace('text-', 'bg-'))} />
              )}
              <div className="p-5">
                <div className="flex items-start justify-between mb-4">
                  <div className="flex items-center gap-3">
                    <div className={cn("w-10 h-10 rounded-xl flex items-center justify-center border transition-all", color.bg, color.border, isActive && "ring-2 " + color.ring)}>
                      <span className={cn("text-base font-bold", color.text)}>{agentKey.charAt(0).toUpperCase()}</span>
                    </div>
                    <div>
                      <h4 className="text-sm font-bold text-slate-100">{session.agent_id ?? session.client_name ?? 'anon'}</h4>
                      <p className="text-[10px] font-mono text-slate-500">{session.id.slice(0, 16)}…</p>
                    </div>
                  </div>
                  <div className={cn("flex items-center gap-1.5 px-2 py-0.5 rounded-full text-[9px] font-bold border", isActive ? `${color.bg} ${color.border} ${color.text}` : 'bg-slate-800/60 border-slate-700 text-slate-500')}>
                    <div className={cn("w-1.5 h-1.5 rounded-full", isActive ? color.text.replace('text-', 'bg-') + ' animate-ping' : 'bg-slate-600')} />
                    {isActive ? 'ACTIVE' : 'IDLE'}
                  </div>
                </div>
                <div className="grid grid-cols-2 gap-2 mb-4">
                  <div className="rounded-lg bg-slate-800/50 px-3 py-2">
                    <p className="text-[9px] text-slate-500 uppercase tracking-wider mb-0.5">Tool Calls</p>
                    <p className={cn("text-lg font-bold font-mono", color.text)}>{session.tool_count ?? 0}</p>
                  </div>
                  <div className="rounded-lg bg-slate-800/50 px-3 py-2">
                    <p className="text-[9px] text-slate-500 uppercase tracking-wider mb-0.5">Uptime</p>
                    <p className="text-lg font-bold font-mono text-slate-300">{formatUptime(session.created_unix ? Math.floor(Date.now() / 1000) - session.created_unix : 0)}</p>
                  </div>
                </div>
                <div className="space-y-2">
                  {isActive && act ? (
                    <div className={cn("rounded-lg border px-3 py-2", color.bg, color.border)}>
                      <p className={cn("text-[9px] uppercase tracking-wider mb-0.5 opacity-70", color.text)}>Executing</p>
                      <p className={cn("text-xs font-bold font-mono", color.text)}>{act.tool}</p>
                      {act.intent && <p className="text-[10px] text-slate-400 mt-0.5 truncate">{act.intent}</p>}
                    </div>
                  ) : session.last_intent ? (
                    <div className="rounded-lg bg-slate-800/40 px-3 py-2">
                      <p className="text-[9px] text-slate-500 uppercase tracking-wider mb-0.5">Last Intent</p>
                      <p className="text-[10px] text-slate-400 truncate">{session.last_intent}</p>
                    </div>
                  ) : (
                    <div className="rounded-lg bg-slate-800/30 px-3 py-2">
                      <p className="text-[10px] text-slate-600 italic">No activity recorded yet</p>
                    </div>
                  )}
                  {session.last_guild && <span className={cn("font-bold uppercase", color.text)}>→ {session.last_guild}</span>}
                </div>
                <button
                  type="button"
                  onClick={() => setSelectedAgentId(session.agent_id ?? session.client_name ?? null)}
                  className={cn("mt-4 w-full py-2 rounded-xl border text-[10px] font-bold uppercase tracking-widest hover:brightness-125 transition-all flex items-center justify-center gap-2", color.border, color.bg, color.text)}
                >
                  <BookOpen className="w-3 h-3" /> Ver Memoria e Identidad
                </button>
              </div>
            </div>
          );
        })}

        {sessions.length === 0 && (
          <div className="col-span-full py-24 rounded-2xl border-2 border-dashed border-slate-800 flex flex-col items-center justify-center text-center">
            <div className="w-16 h-16 rounded-full bg-slate-900 border border-slate-800 flex items-center justify-center mb-4">
              <WifiOff className="w-8 h-8 text-slate-700" />
            </div>
            <h4 className="text-sm font-bold text-slate-400">No Active Connections</h4>
            <p className="text-xs text-slate-600 max-w-xs mt-2">Connect a MCP client to see it appear here in real-time.</p>
            <div className="mt-4 font-mono text-[10px] text-slate-700 bg-slate-900 border border-slate-800 rounded-lg px-4 py-2">
              claude mcp add tylluan --transport sse --url http://localhost:3030/sse
            </div>
          </div>
        )}
      </div>

      {selectedAgentId && (
        <div className="fixed inset-0 z-[60] flex justify-end animate-in fade-in duration-300">
          <div className="absolute inset-0 bg-slate-950/60 backdrop-blur-sm" onClick={() => setSelectedAgentId(null)} />
          <div className="relative w-full max-w-md bg-slate-900 border-l border-slate-800 shadow-2xl flex flex-col animate-in slide-in-from-right duration-500">
            <div className="p-6 border-b border-slate-800 flex items-center justify-between">
              <div className="flex items-center gap-4">
                <div className="w-12 h-12 rounded-2xl bg-slate-800 flex items-center justify-center border border-slate-700">
                  <User className="w-6 h-6 text-slate-400" />
                </div>
                <div>
                  <h3 className="text-lg font-bold text-slate-100">{selectedAgentId}</h3>
                  <p className="text-[10px] text-slate-500 font-mono uppercase tracking-widest">Sovereign Agent Profile</p>
                </div>
              </div>
              <div className="flex items-center gap-2">
                <button 
                  onClick={handleAskIdentity}
                  disabled={askingIdentity}
                  className="flex items-center gap-1.5 px-3 py-1.5 bg-violet-500/20 hover:bg-violet-500/30 text-violet-400 rounded-lg text-[10px] font-bold uppercase transition-all border border-violet-500/30 disabled:opacity-50"
                >
                  {askingIdentity ? <RefreshCw className="w-3 h-3 animate-spin" /> : <Zap className="w-3 h-3" />}
                  Ask Identity
                </button>
                <button onClick={() => setSelectedAgentId(null)} className="p-2 hover:bg-slate-800 rounded-full text-slate-500">
                  <X className="w-5 h-5" />
                </button>
              </div>
            </div>

            <div className="flex-1 overflow-y-auto p-6 space-y-8">
              <section className="space-y-3">
                <h4 className="text-[10px] font-bold text-slate-500 uppercase tracking-widest flex items-center gap-2">
                  <FileText className="w-3 h-3" /> Resumen Narrativo
                </h4>
                <div className="p-4 rounded-xl bg-slate-950/50 border border-slate-800 text-sm text-slate-300 italic leading-relaxed">
                  {summary?.summary || "Sin resumen narrativo aún. El agente está en proceso de autodescubrimiento."}
                </div>
              </section>

              {profiles.find(p => p.agent_id === selectedAgentId) && (
                <section className="space-y-3">
                  <h4 className="text-[10px] font-bold text-slate-500 uppercase tracking-widest flex items-center gap-2">
                    <Zap className="w-3 h-3 text-amber-400" /> Perfil de Competencias
                  </h4>
                  <div className="grid grid-cols-1 gap-2">
                    {Object.entries(profiles.find(p => p.agent_id === selectedAgentId)!.competencies).map(([guild, value]) => (
                      <div key={guild} className="space-y-1">
                        <div className="flex justify-between text-[9px] font-mono">
                          <span className="text-slate-400 uppercase">{guild}</span>
                          <span className="text-cyan-400">{(value * 100).toFixed(0)}%</span>
                        </div>
                        <div className="h-1 bg-slate-800 rounded-full overflow-hidden">
                          <div className="h-full bg-cyan-500/50" style={{ width: `${value * 100}%` }} />
                        </div>
                      </div>
                    ))}
                  </div>
                </section>
              )}

              {(askIdentityResponse || askingIdentity) && (
                <section className="space-y-3 animate-in zoom-in duration-300">
                  <div className="p-4 rounded-xl bg-violet-950/20 border border-violet-500/40 shadow-xl shadow-violet-500/5">
                    <h4 className="text-[10px] font-bold text-violet-400 uppercase tracking-widest mb-2 flex items-center gap-2">
                      <Zap className="w-3 h-3" /> Respuesta del Cortex
                    </h4>
                    {askingIdentity ? (
                      <div className="flex items-center gap-3 py-2 text-slate-500 italic text-sm">
                        <RefreshCw className="w-4 h-4 animate-spin" /> Analizando el pasado...
                      </div>
                    ) : (
                      <div className="text-sm text-slate-100 leading-relaxed font-serif">
                        <span className="text-violet-400 text-lg font-bold">"</span>
                        {askIdentityResponse}
                        <span className="text-violet-400 text-lg font-bold">"</span>
                      </div>
                    )}
                  </div>
                </section>
              )}

              <button
                type="button"
                onClick={() => handleDeleteMemories(selectedAgentId)}
                className="w-full py-3 rounded-xl border border-red-500/30 bg-red-500/10 text-red-400 text-xs font-bold uppercase tracking-widest hover:bg-red-500/20 transition-all flex items-center justify-center gap-2"
              >
                <Trash2 className="w-4 h-4" /> Olvidar Agente y Reiniciar Identidad
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

// --- Main Component: CollectiveTab ---
export function CollectiveTab() {
  const { bridge } = useNexus();
  const [pulse, setPulse] = useState<CollectivePulse | null>(null);
  const [timeline, setTimeline] = useState<TimelineEvent[]>([]);
  const [heatmap, setHeatmap] = useState<{ date: string; count: number }[]>([]);
  const [reputation, setReputation] = useState<any[]>([]);
  const [error, setError] = useState<string | null>(null);

  const agentColor = (id: string) => AGENT_COLORS[Math.abs(id.split('').reduce((a,c) => a + c.charCodeAt(0), 0)) % AGENT_COLORS.length];

  const fetchAll = useCallback(async () => {
    if (!bridge) return;
    try {
      const results = await Promise.allSettled([
        bridge.fetchRaw('/api/v1/collective/pulse', {}),
        bridge.fetchRaw('/api/v1/collective/timeline', {}),
        bridge.getCollectiveHeatmap(),
        bridge.getCollectiveReputation(),
      ]);
      const p = results[0].status === 'fulfilled' ? results[0].value : null;
      const t = results[1].status === 'fulfilled' ? results[1].value : null;
      const h = results[2].status === 'fulfilled' ? results[2].value : null;
      const r = results[3].status === 'fulfilled' ? results[3].value : null;

      if (p) setPulse(p);
      if (t) setTimeline(t.events ?? []);
      if (h) setHeatmap(h.heatmap ?? []);
      if (r) setReputation(r.reputation ?? []);

      const anySuccess = results.some(res => res.status === 'fulfilled');
      if (anySuccess) {
        setError(null);
      } else {
        setError('No se puede conectar al kernel');
      }
    } catch (e) {
      setError('No se puede conectar al kernel');
    }
  }, [bridge]);

  useEffect(() => {
    fetchAll();
    const id = setInterval(() => {
      if (document.visibilityState !== 'hidden') fetchAll();
    }, 30000);
    const onVisibility = () => { if (document.visibilityState === 'visible') fetchAll(); };
    document.addEventListener('visibilitychange', onVisibility);
    return () => {
      clearInterval(id);
      document.removeEventListener('visibilitychange', onVisibility);
    };
  }, [fetchAll]);

  if (error) return (
    <div className="p-6 text-red-400 text-sm">{error}</div>
  );

  return (
    <div className="p-4 space-y-12">
      {/* SECTION 1: Collective/Agents Realtime */}
      <div className="space-y-6">
        <RealtimeAgentsTab notify={() => {}} />

        {/* Pulse header */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
        <div className="bg-slate-800 rounded-lg p-4 border border-slate-700">
          <div className="text-2xl font-bold text-emerald-400">{pulse?.active_count ?? '—'}</div>
          <div className="text-xs text-slate-400 mt-1">Agentes activos</div>
        </div>
        <div className="bg-slate-800 rounded-lg p-4 border border-slate-700">
          <div className="text-2xl font-bold text-blue-400">{pulse?.broadcasts_last_hour ?? '—'}</div>
          <div className="text-xs text-slate-400 mt-1">Broadcasts última hora</div>
        </div>
        <div className="bg-slate-800 rounded-lg p-4 border border-slate-700">
          <div className="text-2xl font-bold text-violet-400">{pulse?.graph.nodes ?? '—'}</div>
          <div className="text-xs text-slate-400 mt-1">Graph nodes</div>
        </div>
        <div className="bg-slate-800 rounded-lg p-4 border border-slate-700">
          <div className="text-2xl font-bold text-amber-400">{pulse?.graph.edges ?? '—'}</div>
          <div className="text-xs text-slate-400 mt-1">Conexiones</div>
        </div>
      </div>

      {/* Active agents badges */}
      {pulse && pulse.active_agents.length > 0 && (
        <div className="bg-slate-800 rounded-lg p-4 border border-slate-700">
          <div className="text-xs text-slate-400 mb-2 uppercase tracking-wide">Agentes en línea</div>
          <div className="flex flex-wrap gap-2">
            {pulse.active_agents.map(a => (
              <span key={a} className="px-3 py-1 rounded-full text-xs font-medium flex items-center gap-1"
                style={{ backgroundColor: agentColor(a) + '22', color: agentColor(a), border: `1px solid ${agentColor(a)}44` }}>
                <span className="w-1.5 h-1.5 rounded-full animate-pulse inline-block" style={{ backgroundColor: agentColor(a) }} />
                {a}
              </span>
            ))}
          </div>
        </div>
      )}

      {/* Collective Heatmap (GitHub Style) */}
      <div className="bg-slate-900/50 rounded-2xl border border-slate-800 p-6 shadow-xl">
        <div className="flex items-center justify-between mb-6">
          <div>
            <h3 className="text-sm font-bold text-slate-100 flex items-center gap-2">
              <RefreshCw className="w-4 h-4 text-emerald-400" /> Stigmergy Heatmap
            </h3>
            <p className="text-[10px] text-slate-500 font-mono mt-0.5 uppercase tracking-wider">Collective Activity across the Realm</p>
          </div>
          <div className="flex items-center gap-4 text-[10px] text-slate-500 font-mono">
            <div className="flex items-center gap-1"><div className="w-2 h-2 rounded-sm bg-slate-800" /> 0</div>
            <div className="flex items-center gap-1"><div className="w-2 h-2 rounded-sm bg-emerald-900" /> 1-3</div>
            <div className="flex items-center gap-1"><div className="w-2 h-2 rounded-sm bg-emerald-600" /> 4-9</div>
            <div className="flex items-center gap-1"><div className="w-2 h-2 rounded-sm bg-emerald-400" /> 10+</div>
          </div>
        </div>
        <HeatmapGrid data={heatmap} />
      </div>

      {/* Domain Reputation */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <div className="bg-slate-900/50 rounded-2xl border border-slate-800 overflow-hidden shadow-xl">
          <div className="px-6 py-4 border-b border-slate-800 bg-slate-800/30">
            <h3 className="text-sm font-bold text-slate-100 flex items-center gap-2">
              <Zap className="w-4 h-4 text-amber-400" /> Guild Reliability
            </h3>
            <p className="text-[10px] text-slate-500 font-mono uppercase tracking-wider mt-0.5">Reputation by Domain</p>
          </div>
          <div className="p-6 space-y-4">
            {reputation.length === 0 && (
              <div className="py-8 text-center text-slate-600 text-xs italic">No reputation data available yet</div>
            )}
            {reputation.map((item, idx) => (
              <div key={item.guild + idx} className="group flex items-center justify-between p-3 rounded-xl bg-slate-800/40 border border-slate-700/50 hover:border-slate-600 transition-all">
                <div className="flex items-center gap-4">
                  <div className={cn(
                    "w-10 h-10 rounded-xl flex items-center justify-center border font-bold text-xs uppercase transition-all",
                    item.total_calls === 0 ? "bg-slate-500/10 border-slate-700/50 text-slate-500" :
                    item.tier === 'reliable' ? "bg-emerald-500/10 border-emerald-500/30 text-emerald-400" :
                    "bg-amber-500/10 border-amber-500/30 text-amber-400"
                  )}>
                    {item.guild.charAt(0)}
                  </div>
                  <div>
                    <div className="flex items-center gap-2">
                      <h4 className="text-xs font-bold text-slate-200">{item.guild}</h4>
                      <span className={cn(
                        "px-1.5 py-0.5 rounded text-[8px] font-bold uppercase tracking-widest",
                        item.total_calls === 0 ? "bg-slate-800 text-slate-600" :
                        item.tier === 'reliable' ? "bg-emerald-500/20 text-emerald-400" :
                        "bg-amber-500/20 text-amber-400"
                      )}>
                        {item.total_calls === 0 ? 'dormant' : item.tier}
                      </span>
                    </div>
                    <p className="text-[10px] text-slate-500 mt-0.5 font-mono">{item.agent_id}</p>
                  </div>
                </div>
                <div className="text-right">
                  <div className="text-xs font-bold text-slate-100">{item.score.toFixed(1)}%</div>
                  <div className="text-[9px] text-slate-500 font-mono mt-0.5">{item.total_calls} calls</div>
                </div>
              </div>
            ))}
          </div>
        </div>

        {/* Timeline (Repurposed as Activity Stream) */}
        <div className="bg-slate-900/50 rounded-2xl border border-slate-800 overflow-hidden shadow-xl">
          <div className="px-6 py-4 border-b border-slate-800 bg-slate-800/30 flex items-center justify-between">
            <div>
              <h3 className="text-sm font-bold text-slate-100 flex items-center gap-2">
                <FileText className="w-4 h-4 text-blue-400" /> Activity Stream
              </h3>
              <p className="text-[10px] text-slate-500 font-mono uppercase tracking-wider mt-0.5">Real-time Collective Memory</p>
            </div>
            <div className="px-2 py-1 rounded bg-slate-800 text-[10px] text-slate-500 font-mono border border-slate-700">
              {timeline.length} events
            </div>
          </div>
          <div className="max-h-[420px] overflow-y-auto divide-y divide-slate-800">
            {timeline.length === 0 && (
              <div className="p-12 text-center text-slate-600 text-sm italic">Sin actividad reciente</div>
            )}
            {timeline.map((ev, i) => (
              <div key={ev.id + i} className="p-4 flex items-start gap-4 hover:bg-slate-800/50 transition-colors">
                <div className="mt-1 flex-shrink-0">
                  <div className={cn("w-2 h-2 rounded-full", 
                    ev.type.includes('tool') ? 'bg-emerald-400' :
                    ev.type === 'identity' ? 'bg-amber-400' :
                    ev.type === 'agent_memory' ? 'bg-blue-400' :
                    ev.type === 'concept' ? 'bg-violet-400' : 'bg-slate-500'
                  )} />
                </div>
                <div className="flex-1 min-w-0">
                  <p className="text-[11px] text-slate-300 leading-relaxed font-medium">{ev.content}</p>
                  <div className="flex items-center gap-3 mt-1.5 opacity-60">
                    <span className="text-[9px] text-slate-400 uppercase tracking-widest font-bold">{ev.type}</span>
                    {ev.updated_at && (
                      <>
                        <span className="text-slate-700 text-[8px]">|</span>
                        <span className="text-[9px] text-slate-500 font-mono">{new Date(ev.updated_at).toLocaleTimeString()}</span>
                      </>
                    )}
                  </div>
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>

      </div>{/* end SECTION 1 space-y-6 */}


      {/* SECTION 3: Sessions Management */}
      <div className="pt-8 border-t border-slate-800">
        <SessionsTab bridge={bridge} notify={(msg, type) => console.log(`[SessionsTab] ${type}: ${msg}`)} />
      </div>

    </div>
  );
}

// --- Sub-component: HeatmapGrid ---
function HeatmapGrid({ data }: { data: { date: string; count: number }[] }) {
  const grid = useMemo(() => {
    const lookup = new Map(data.map(d => [d.date, d.count]));
    const today = new Date();
    const start = new Date(today);
    start.setDate(today.getDate() - 363);
    
    return Array.from({ length: 364 }, (_, i) => {
      const d = new Date(start);
      d.setDate(start.getDate() + i);
      const key = d.toISOString().split('T')[0];
      const count = lookup.get(key) ?? 0;
      const color = count === 0 ? 'bg-slate-800'
        : count < 4  ? 'bg-emerald-900'
        : count < 10 ? 'bg-emerald-600'
        : 'bg-emerald-400';
      return { key, label: `${count} events on ${key}`, color };
    });
  }, [data]);

  return (
    <div className="flex flex-col gap-1">
      <div
        className="grid gap-0.5"
        style={{ 
          gridTemplateColumns: 'repeat(52, minmax(0, 1fr))',
          gridAutoRows: '12px' 
        }}
      >
        {grid.map(cell => (
          <div
            key={cell.key}
            className={cn("rounded-sm transition-colors", cell.color)}
            title={cell.label}
            role="img"
            aria-label={cell.label}
          />
        ))}
      </div>
      <p className="text-[9px] text-slate-500 text-right font-mono uppercase tracking-widest mt-2">Last 52 weeks of collective activity</p>
    </div>
  );
}
