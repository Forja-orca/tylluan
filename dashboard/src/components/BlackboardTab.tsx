import React, { useState, useEffect } from 'react';
import { MessageSquare, Clock, User, Users, RefreshCw, AlertCircle, Zap, GitCommit, ChevronDown } from 'lucide-react';
import { useNexus } from '../hooks/useNexus';
import { NexusBridge } from '../lib/nexus-bridge';
import { cn } from '../lib/utils';

interface BlackboardTask {
  id: string;
  content: string;
  created_by: string;
  assigned_to?: string;
  priority: 'high' | 'medium' | 'low';
  age_mins: number;
}

interface BlackboardData {
  pending: BlackboardTask[];
  completed_today: number;
  active_agents: string[];
  total_tasks: number;
}

interface BlackboardTabProps {
  bridge: NexusBridge | null;
}

export function BlackboardTab({ bridge }: BlackboardTabProps) {
  const { events } = useNexus();
  const [data, setData] = useState<BlackboardData | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [expandedTasks, setExpandedTasks] = useState<Set<string>>(new Set());

  const fetchData = async () => {
    if (!bridge) return;
    setLoading(true);
    try {
      const resp = await bridge.fetchRaw('/api/v1/blackboard', {});
      setData(resp);
      setError(null);
    } catch (err: any) {
      if (err.message?.includes('404')) {
        setError('Blackboard disponible tras rebuild del kernel');
      } else {
        setError(err.message || 'Error al conectar con Blackboard');
      }
      setData(null);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchData();
    const interval = setInterval(fetchData, 30000);
    return () => clearInterval(interval);
  }, [bridge]);

  const toggleExpand = (id: string) => {
    setExpandedTasks(prev => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const getPriorityColor = (p: string) => {
    switch (p) {
      case 'high': return 'bg-red-500/20 text-red-400 border-red-500/30';
      case 'medium': return 'bg-amber-500/20 text-amber-400 border-amber-500/30';
      case 'low': return 'bg-emerald-500/20 text-emerald-400 border-emerald-500/30';
      default: return 'bg-slate-500/20 text-slate-400 border-slate-500/30';
    }
  };

  const getAgentColor = (agent: string) => {
    const a = agent.toLowerCase();
    if (a.includes('anthropic') || a.includes('claude')) return 'bg-blue-500 text-white';
    if (a.includes('google') || a.includes('gemini')) return 'bg-red-500 text-white';
    
    // Dynamic color selection based on hash
    let hash = 0;
    for (let i = 0; i < a.length; i++) {
      hash = a.charCodeAt(i) + ((hash << 5) - hash);
    }
    const colors = ['bg-orange-500 text-white', 'bg-blue-500 text-white', 'bg-purple-500 text-white', 'bg-cyan-500 text-white', 'bg-pink-500 text-white', 'bg-indigo-500 text-white', 'bg-teal-500 text-white'];
    return colors[Math.abs(hash) % colors.length];
  };

  if (loading && !data) {
    return (
      <div className="flex flex-col items-center justify-center h-64 text-slate-500 animate-pulse">
        <RefreshCw className="w-8 h-8 mb-2 animate-spin" />
        <p className="text-sm font-medium">Synchronizing multi-agent blackboard...</p>
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex flex-col items-center justify-center h-96 p-8 text-center border-2 border-dashed border-slate-800 rounded-3xl bg-slate-900/20">
        <div className="w-16 h-16 bg-slate-800 rounded-2xl flex items-center justify-center mb-4">
          <MessageSquare className="w-8 h-8 text-slate-600" />
        </div>
        <h3 className="text-lg font-bold text-slate-300 mb-2">Pizarra Desconectada</h3>
        <p className="text-sm text-slate-500 max-w-xs">{error}</p>
        <button 
          onClick={fetchData}
          className="mt-6 px-4 py-2 bg-slate-800 hover:bg-slate-700 text-slate-300 rounded-xl text-sm font-bold transition-colors cursor-pointer"
        >
          Reintentar Conexión
        </button>
      </div>
    );
  }

  const pendingCount = data?.pending.length || 0;
  const completedToday = data?.completed_today || 0;

  return (
    <div className="space-y-6 animate-in fade-in duration-500">
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4 bg-slate-900/50 border border-slate-800 p-6 rounded-2xl backdrop-blur-sm">
        <div>
          <h2 className="text-xl font-black text-white tracking-tight flex items-center gap-2">
            <MessageSquare className="w-5 h-5 text-emerald-400" />
            Collective Blackboard
          </h2>
          <p className="text-xs text-slate-500 mt-1">
            <span className="text-emerald-400 font-bold">{pendingCount}</span> tareas pendientes · 
            <span className="text-slate-300 font-bold ml-1">{completedToday}</span> completadas hoy
          </p>
        </div>
        <button 
          onClick={fetchData}
          disabled={loading}
          className="flex items-center gap-2 px-4 py-2 bg-emerald-500/10 hover:bg-emerald-500/20 text-emerald-400 rounded-xl text-sm font-bold border border-emerald-500/20 transition-all cursor-pointer disabled:opacity-50"
        >
          <RefreshCw className={cn("w-4 h-4", loading && "animate-spin")} />
          Refrescar
        </button>
      </div>

      {/* Empty state when no tasks and no active agents */}
      {data?.pending.length === 0 && (!data?.active_agents || data.active_agents.length === 0) && (
        <div className="py-24 rounded-2xl border-2 border-dashed border-slate-800 flex flex-col items-center justify-center text-center bg-slate-900/20">
          <div className="w-16 h-16 rounded-full bg-slate-900 border border-slate-800 flex items-center justify-center mb-4">
            <MessageSquare className="w-8 h-8 text-slate-700" />
          </div>
          <h4 className="text-sm font-bold text-slate-400">Blackboard is Empty</h4>
          <p className="text-xs text-slate-600 max-w-sm mt-2">
            No tasks queued. Tasks appear here when tylluan_do creates multi-step operations that require delegation to specialized agents or guilds.
          </p>
        </div>
      )}

      {/* Task grid (only show if there are tasks or active agents) */}
      {(data?.pending.length ?? 0) > 0 || (data?.active_agents?.length ?? 0) > 0 && (
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        <div className="lg:col-span-2 space-y-3">
          <h3 className="text-[10px] font-bold uppercase tracking-widest text-slate-500 mb-2 px-1">Tareas en curso</h3>
          {data?.pending.length === 0 ? (
            <div className="p-12 text-center border border-slate-800 rounded-2xl bg-slate-900/20">
              <p className="text-sm text-slate-600 italic">No hay tareas pendientes en el Blackboard.</p>
            </div>
          ) : (
            data?.pending.map(task => (
              <div 
                key={task.id} 
                className="group bg-slate-900/40 border border-slate-800 hover:border-slate-700 p-4 rounded-2xl transition-all"
              >
                <div className="flex items-start justify-between gap-4 mb-3">
                  <div className="flex items-center gap-2">
                    <span className={cn("px-2 py-0.5 rounded-full text-[9px] font-black uppercase border", getPriorityColor(task.priority))}>
                      {task.priority}
                    </span>
                    <div className="flex items-center gap-1.5 text-[10px] text-slate-500 font-mono">
                      <Clock className="w-3 h-3" />
                      hace {task.age_mins} min
                    </div>
                  </div>
                  <div className="text-[9px] text-slate-600 font-mono select-all">ID: {task.id.slice(0, 8)}</div>
                </div>

                <div 
                  className={cn(
                    "text-sm text-slate-300 leading-relaxed cursor-pointer transition-colors hover:text-slate-100",
                    !expandedTasks.has(task.id) && "line-clamp-2"
                  )}
                  onClick={() => toggleExpand(task.id)}
                >
                  {task.content}
                </div>

                <div className="mt-4 pt-3 border-t border-slate-800/50 flex items-center justify-between">
                  <div className="flex items-center gap-4">
                    <div className="flex items-center gap-1.5">
                      <span className="text-[9px] text-slate-500 uppercase tracking-widest font-bold">De:</span>
                      <span className="text-[10px] font-bold text-slate-300">{task.created_by}</span>
                    </div>
                    <div className="w-px h-3 bg-slate-800" />
                    <div className="flex items-center gap-1.5">
                      <span className="text-[9px] text-slate-500 uppercase tracking-widest font-bold">Para:</span>
                      <span className={cn(
                        "text-[10px] font-bold",
                        task.assigned_to ? "text-emerald-400" : "text-slate-600 italic"
                      )}>
                        {task.assigned_to || 'sin asignar'}
                      </span>
                    </div>
                  </div>
                </div>
              </div>
            ))
          )}
        </div>

        <div className="space-y-6">
          <div className="bg-slate-900/50 border border-slate-800 rounded-2xl p-5">
            <h3 className="text-[10px] font-bold uppercase tracking-widest text-slate-500 mb-4 flex items-center gap-2">
              <Users className="w-3.5 h-3.5 text-blue-400" />
              Agentes Activos ahora
            </h3>
            <div className="space-y-3">
              {data?.active_agents.map(agent => {
                const minAge = data.pending.length
                  ? Math.min(...data.pending.map(t => t.age_mins))
                  : null;
                const lastActivity = minAge !== null
                  ? minAge < 1 ? '< 1m'
                  : minAge < 60 ? `< ${Math.ceil(minAge)}m`
                  : `< ${Math.ceil(minAge / 60)}h`
                  : 'sin tareas';
                return (
                  <div key={agent} className="flex items-center gap-3 p-2 rounded-xl hover:bg-slate-800/30 transition-colors">
                    <div className={cn("w-8 h-8 rounded-lg flex items-center justify-center font-bold text-xs uppercase shadow-lg", getAgentColor(agent))}>
                      {agent.charAt(0)}
                    </div>
                    <div>
                      <div className="text-xs font-bold text-slate-200">{agent}</div>
                      <div className="text-[9px] text-slate-500 font-mono tracking-tighter">Última actividad: {lastActivity}</div>
                    </div>
                  </div>
                );
              })}
              {(!data?.active_agents || data.active_agents.length === 0) && (
                <p className="text-xs text-slate-600 italic text-center py-4">No hay agentes activos recientemente.</p>
              )}
            </div>
          </div>

          <div className="p-5 rounded-2xl bg-gradient-to-br from-emerald-500/5 to-blue-500/5 border border-slate-800">
            <h4 className="text-xs font-bold text-slate-300 mb-2">¿Cómo funciona?</h4>
            <p className="text-[11px] text-slate-500 leading-relaxed">
              El Blackboard permite a los agentes delegar tareas complejas a otros guilds o agentes especializados cuando detectan que no pueden resolver un paso de forma óptima.
            </p>
          </div>

          <ReasoningChain events={events} />
        </div>
      </div>
      )}
    </div>
  );
}

function ReasoningChain({ events }: { events: any[] }) {
  const thoughts = events
    .filter(ev => ev.type === 'tool_call' || ev.type === 'concept' || ev.data?.tool === 'tylluan_think')
    .slice(0, 8);

  return (
    <div className="bg-slate-900/50 border border-slate-800 rounded-2xl p-5 overflow-hidden">
      <h3 className="text-[10px] font-bold uppercase tracking-widest text-slate-500 mb-6 flex items-center gap-2">
        <GitCommit className="w-3.5 h-3.5 text-violet-400" />
        Chain of Thought (CoT)
      </h3>
      
      <div className="relative space-y-6 before:absolute before:left-[11px] before:top-2 before:bottom-2 before:w-px before:bg-slate-800">
        {thoughts.map((ev, i) => (
          <div key={i} className="relative pl-8 animate-in slide-in-from-left-2 duration-300">
            <div className={cn(
              "absolute left-0 top-1 w-6 h-6 rounded-full border-2 border-slate-900 flex items-center justify-center z-10",
              ev.type === 'tool_call' ? "bg-emerald-500" : "bg-violet-500"
            )}>
              {ev.type === 'tool_call' ? <Zap className="w-3 h-3 text-white" /> : <GitCommit className="w-3 h-3 text-white" />}
            </div>
            <div>
              <div className="flex items-center gap-2 mb-1">
                <span className="text-[10px] font-bold text-slate-300 uppercase">{ev.data?.agent_id || 'system'}</span>
                <span className="text-[8px] text-slate-600 font-mono tracking-tighter">
                    {new Date(ev.ts || Date.now()).toLocaleTimeString()}
                </span>
              </div>
              <p className="text-xs text-slate-400 leading-relaxed italic">
                {ev.data?.intent || ev.data?.query || ev.content || "Reasoning step..."}
              </p>
              {ev.data?.tool && (
                <div className="mt-2 inline-flex items-center gap-1.5 px-2 py-0.5 rounded bg-slate-800 border border-slate-700 text-[9px] font-mono text-slate-500">
                    {ev.data.tool}
                </div>
              )}
            </div>
          </div>
        ))}

        {thoughts.length === 0 && (
          <div className="py-4 text-center text-slate-700 italic text-[10px]">
            Esperando procesos de razonamiento...
          </div>
        )}
      </div>
    </div>
  );
}
