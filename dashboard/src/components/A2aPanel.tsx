import React, { useState, useEffect } from 'react';
import { useNexus } from '../hooks/useNexus';
import { Search, Check, X, ShieldCheck, Key, Globe, Layers, AlertCircle, RefreshCw, Clock } from 'lucide-react';
import { cn } from '../lib/utils';

interface AgentCard {
  protocolVersion: string;
  name: string;
  url: string;
  skills: Array<{ name: string; description: string; category?: string }>;
  securitySchemes: Record<string, any>;
}



interface Props {
  notify: (msg: string, type?: 'info' | 'error') => void;
}

export default function A2aPanel({ notify }: Props) {
  const { approvals, bridge, refreshData } = useNexus();
  const [card, setCard] = useState<AgentCard | null>(null);
  const [loadingCard, setLoadingCard] = useState(false);

  // Task Lookup
  const [taskId, setTaskId] = useState('');
  const [searchedTask, setSearchedTask] = useState<any | null>(null);
  const [loadingTask, setLoadingTask] = useState(false);
  const [taskError, setTaskError] = useState<string | null>(null);

  // Approval action loading states
  const [actingApprovalId, setActingApprovalId] = useState<string | null>(null);

  const fetchAgentCard = async () => {
    if (!bridge) return;
    setLoadingCard(true);
    try {
      const data = await bridge.getAgentCard() as AgentCard;
      setCard(data);
    } catch (e: any) {
      console.error("Error al cargar agent-card.json:", e.message);
      setCard(null);
    } finally {
      setLoadingCard(false);
    }
  };

  const handleInspectTask = async () => {
    if (!taskId.trim() || !bridge) return;
    setLoadingTask(true);
    setTaskError(null);
    setSearchedTask(null);
    try {
      const res = await bridge.getA2aTaskStatus(taskId.trim());
      setSearchedTask(res);
    } catch (e: any) {
      console.error(`Error querying A2A task '${taskId}':`, e.message);
      setSearchedTask(null);
      setTaskError(`A2A task '${taskId.trim()}' not found or query failed: ${e.message}`);
    } finally {
      setLoadingTask(false);
    }
  };

  const handleApprove = async (id: string) => {
    if (!bridge) return;
    setActingApprovalId(id);
    try {
      await bridge.approveAction(id);
      notify(`Action approved successfully`, 'info');
      await refreshData();
    } catch (e: any) {
      notify(`Approval error: ${e.message}`, 'error');
    } finally {
      setActingApprovalId(null);
    }
  };

  const handleReject = async (id: string) => {
    if (!bridge) return;
    setActingApprovalId(id);
    try {
      await bridge.rejectAction(id);
      notify(`Action rejected successfully`, 'info');
      await refreshData();
    } catch (e: any) {
      notify(`Rejection error: ${e.message}`, 'error');
    } finally {
      setActingApprovalId(null);
    }
  };

  useEffect(() => {
    fetchAgentCard();
  }, [bridge]);

  return (
    <div className="space-y-6 h-full flex flex-col min-h-0 overflow-y-auto">
      
      {/* Grid of details */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6 items-start">
        
        {/* Left: Agent Card */}
        <div className="bg-slate-900/60 rounded-xl p-5 space-y-4">
          <div className="flex items-center justify-between border-b border-slate-800/50 pb-3">
            <div className="flex items-center gap-2">
              <Globe className="w-5 h-5 text-emerald-400" />
              <h3 className="text-sm font-medium text-slate-100">A2A Agent Card Manifest</h3>
            </div>
            <button
              onClick={fetchAgentCard}
              disabled={loadingCard}
              className="p-1 hover:bg-slate-800 rounded transition-colors text-slate-500 hover:text-slate-200 cursor-pointer disabled:opacity-50"
              title="Reload Agent Card"
            >
              <RefreshCw className={cn("w-3.5 h-3.5", loadingCard && "animate-spin")} />
            </button>
          </div>

          {loadingCard ? (
            <div className="py-12 flex items-center justify-center gap-2 text-xs text-slate-500 font-mono">
              <RefreshCw className="w-4 h-4 animate-spin text-emerald-400" /> Loading manifest...
            </div>
          ) : card ? (
            <div className="space-y-4 text-xs">
              <div className="grid grid-cols-2 gap-4">
                <div className="space-y-1">
                  <span className="text-[10px] text-slate-500 font-mono block">Agent Name</span>
                  <span className="font-bold text-slate-200">{card.name}</span>
                </div>
                <div className="space-y-1">
                  <span className="text-[10px] text-slate-500 font-mono block">Protocol Version</span>
                  <span className="font-mono text-emerald-400 font-bold">{card.protocolVersion}</span>
                </div>
              </div>

              <div className="space-y-1">
                <span className="text-[10px] text-slate-500 font-mono block">JSON-RPC A2A Endpoint</span>
                <span className="font-mono text-slate-300 block bg-slate-950 p-2 rounded-lg truncate select-all">{card.url}</span>
              </div>

              <div className="space-y-2">
                <span className="text-[10px] text-slate-500 font-mono block">Capabilities & Skills ({card.skills.length})</span>
                <div className="grid grid-cols-1 sm:grid-cols-2 gap-2 max-h-48 overflow-y-auto pr-1">
                  {card.skills.map((skill, idx) => (
                    <div key={idx} className="p-2 rounded-lg bg-slate-950 flex flex-col space-y-1">
                      <span className="font-medium text-slate-300 text-[10px]">{skill.name}</span>
                      <span className="text-[10px] text-slate-500 leading-tight">{skill.description}</span>
                    </div>
                  ))}
                </div>
              </div>

              <div className="space-y-2 border-t border-slate-800/50 pt-3">
                <span className="text-[10px] text-slate-500 font-mono block">Security & Authentication</span>
                {Object.entries(card.securitySchemes).map(([key, value]: [string, any]) => (
                  <div key={key} className="p-3 rounded-lg bg-slate-950 flex items-start gap-2.5">
                    <Key className="w-4 h-4 text-violet-400 shrink-0 mt-0.5" />
                    <div>
                      <div className="flex items-center gap-1.5">
                        <span className="font-medium text-slate-200 font-mono text-[10px]">{key}</span>
                        <span className="px-1.5 py-0.2 rounded bg-violet-500/10 text-violet-400 text-[10px] font-medium">{value.scheme}</span>
                      </div>
                      <p className="text-[10px] text-slate-500 mt-1 leading-tight">{value.description}</p>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          ) : (
            <div className="py-8 text-center text-slate-600 italic">Could not load A2A manifest</div>
          )}
        </div>

        {/* Right: Task Inspector */}
        <div className="bg-slate-900/60 rounded-xl p-5 space-y-4">
          <div className="flex items-center gap-2 border-b border-slate-800/50 pb-3">
            <Layers className="w-5 h-5 text-indigo-400" />
            <h3 className="text-sm font-medium text-slate-100">A2A Task Inspector</h3>
          </div>

          <div className="space-y-3">
            <p className="text-xs text-slate-500 leading-relaxed">
              Query status of an incoming task via <code className="font-mono text-slate-300">tasks/get</code>. Enter task ID to query:
            </p>
            <div className="flex gap-2">
              <div className="relative flex-1">
                <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-slate-500" />
                <input
                  type="text"
                  value={taskId}
                  onChange={(e) => setTaskId(e.target.value)}
                  placeholder="e.g. task-a2a-001, task-a2a-003"
                  className="w-full pl-10 pr-4 py-2 bg-slate-950 border border-slate-800 rounded-lg text-xs focus:ring-1 ring-emerald-500 text-slate-200 transition-all placeholder:text-slate-500"
                />
              </div>
              <button
                onClick={handleInspectTask}
                disabled={loadingTask || !taskId.trim()}
                className="px-4 py-2 bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 text-xs font-bold text-slate-50 rounded-lg transition-colors flex items-center gap-1.5 shrink-0 cursor-pointer"
              >
                {loadingTask ? <RefreshCw className="w-3.5 h-3.5 animate-spin" /> : <Search className="w-3.5 h-3.5" />}
                Inspect
              </button>
            </div>
            {taskError && (
              <p className="text-[10px] text-red-400 font-mono flex items-center gap-1">
                <AlertCircle className="w-3 h-3 shrink-0" />
                {taskError}
              </p>
            )}
          </div>

          {loadingTask ? (
            <div className="py-12 flex items-center justify-center gap-2 text-xs text-slate-500 font-mono">
              <RefreshCw className="w-4 h-4 animate-spin text-indigo-400" /> Querying task status...
            </div>
          ) : searchedTask ? (
            <div className="space-y-4 text-xs border-t border-slate-800/50 pt-4">
              <div className="grid grid-cols-2 gap-4">
                <div className="space-y-1">
                  <span className="text-[10px] text-slate-500 font-mono block">Task ID</span>
                  <span className="font-bold font-mono text-slate-200">{searchedTask.id}</span>
                </div>
                <div className="space-y-1">
                  <span className="text-[10px] text-slate-500 font-mono block">Status / State</span>
                  <span className={cn(
                    "px-1.5 py-0.5 rounded text-[10px] font-bold uppercase font-mono border inline-block",
                    searchedTask.state === 'completed' ? "bg-emerald-500/10 text-emerald-400 border-emerald-500/20" :
                    searchedTask.state === 'working' ? "bg-blue-500/10 text-blue-400 border-blue-500/20" :
                    searchedTask.state === 'input-required' ? "bg-amber-500/10 text-amber-400 border-amber-500/20 animate-pulse" :
                    "bg-slate-800 text-slate-400 border-slate-700"
                  )}>
                    {searchedTask.state}
                  </span>
                </div>
              </div>

              <div className="space-y-1">
                <span className="text-[10px] text-slate-500 font-mono block">Client Agent ID</span>
                <span className="font-mono text-slate-300 font-medium bg-slate-950 px-2 py-1.5 rounded-lg block">{searchedTask.client_agent_id}</span>
              </div>

              <div className="space-y-1">
                <span className="text-[10px] text-slate-500 font-mono block">Result / Output</span>
                <div className="bg-slate-950 p-3 rounded-lg border border-slate-800 font-mono text-[11px] leading-relaxed text-slate-300 whitespace-pre-wrap max-h-36 overflow-y-auto">
                  {searchedTask.result || <span className="text-slate-500 italic">No output yet (Task is still in progress)</span>}
                </div>
              </div>

              <div className="grid grid-cols-2 gap-4 border-t border-slate-800 pt-3">
                <div className="flex items-center gap-1.5 text-slate-500">
                  <Clock className="w-3.5 h-3.5" />
                  <div className="flex flex-col">
                    <span className="text-[10px] font-medium text-slate-400">Created At</span>
                    <span className="text-[10px] font-mono text-slate-400">{new Date(searchedTask.created_at).toLocaleTimeString()}</span>
                  </div>
                </div>
                <div className="flex items-center gap-1.5 text-slate-500">
                  <Clock className="w-3.5 h-3.5" />
                  <div className="flex flex-col">
                    <span className="text-[10px] font-medium text-slate-400">Updated At</span>
                    <span className="text-[10px] font-mono text-slate-400">{new Date(searchedTask.updated_at).toLocaleTimeString()}</span>
                  </div>
                </div>
              </div>
            </div>
          ) : null}
        </div>
      </div>

      {/* Action Approvals (Human-in-the-Loop) */}
      <div className="bg-slate-900/60 rounded-xl p-5 flex-1 min-h-0 flex flex-col">
        <div className="flex items-center justify-between border-b border-slate-800/50 pb-3 mb-4">
          <div className="flex items-center gap-2">
            <ShieldCheck className="w-5 h-5 text-amber-500" />
            <h3 className="text-sm font-medium text-slate-100">A2A Sandbox Approvals (Human-in-the-Loop)</h3>
          </div>
          <span className="px-2 py-0.5 rounded bg-amber-500/10 text-amber-500 text-[11px] font-medium">
            {approvals.length} PENDING
          </span>
        </div>

        {approvals.length === 0 ? (
          <div className="flex-1 py-12 flex flex-col items-center justify-center text-slate-600">
            <ShieldCheck className="w-8 h-8 opacity-20 mb-2" />
            <p className="text-xs">No pending approvals required.</p>
            <p className="text-[10px] opacity-60 mt-0.5">All incoming A2A calls are executing within current sandbox profiles.</p>
          </div>
        ) : (
          <div className="flex-1 overflow-y-auto space-y-3 pr-1">
            {approvals.map((app) => (
              <div key={app.id} className="p-4 rounded-xl bg-slate-950 flex flex-col sm:flex-row sm:items-center justify-between gap-4">
                <div className="space-y-1.5 max-w-2xl">
                  <div className="flex items-center gap-2 flex-wrap">
                    <span className="px-2 py-0.5 bg-red-500/10 text-red-400 text-[10px] font-medium rounded-md">
                      Blocked Action
                    </span>
                    <span className="text-[10px] text-slate-500 font-mono">ID: {app.id}</span>
                  </div>
                  <p className="text-xs text-slate-300 font-medium">
                    Intent: <code className="text-emerald-400 font-mono bg-slate-900 px-1 py-0.5 rounded">{(app.params?.intent as string) || 'unknown intent'}</code>
                  </p>
                  <div className="flex gap-4 text-[10px] text-slate-500 font-mono">
                    <span>Guild: <strong className="text-slate-400">{app.guild || 'unknown'}</strong></span>
                    <span>Tool: <strong className="text-slate-400">{app.tool || 'unknown'}</strong></span>
                  </div>
                </div>

                <div className="flex gap-2 shrink-0">
                  <button
                    onClick={() => handleReject(app.id)}
                    disabled={actingApprovalId !== null}
                    className="px-3 py-1.5 bg-red-500/10 border border-red-500/20 hover:bg-red-500/20 disabled:opacity-50 text-red-400 text-xs font-bold rounded-lg transition-all flex items-center gap-1 cursor-pointer"
                  >
                    <X className="w-3.5 h-3.5" /> Reject
                  </button>
                  <button
                    onClick={() => handleApprove(app.id)}
                    disabled={actingApprovalId !== null}
                    className="px-3 py-1.5 bg-emerald-500/10 border border-emerald-500/20 hover:bg-emerald-500/20 disabled:opacity-50 text-emerald-400 text-xs font-bold rounded-lg transition-all flex items-center gap-1 cursor-pointer"
                  >
                    <Check className="w-3.5 h-3.5" /> Approve
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
