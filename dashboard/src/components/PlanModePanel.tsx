import React, { useState } from 'react';
import { 
  ShieldCheck, 
  Play, 
  Eye, 
  AlertOctagon, 
  CheckCircle, 
  HelpCircle,
  Activity,
  ChevronRight,
  ServerOff
} from 'lucide-react';
import { cn } from '../lib/utils';

// Real backend contract (crates/tylluan-kernel/src/transport/server/handler_do/mod.rs,
// M31-P2 plan mode): tylluan_do(intent, plan: true) returns this shape via
// /api/v1/do's `result` field (parsed JSON of the tool's text content).
interface BackendPlan {
  status: 'plan';
  plan_id: string;
  guild: string;
  tool: string;
  risk_level: 'Low' | 'Medium' | 'High';
  intent: string;
  arguments: any;
  message: string;
}

interface PlanDetails {
  guild: string;
  tool_name: string;
  destructive: boolean;
  sandbox_profile: string;
  args_preview: any;
}

interface PlanResponse {
  plan_id?: string;
  plan: boolean;
  would_execute: PlanDetails;
  routing_trace: string[];
}

function riskToDisplay(risk: 'Low' | 'Medium' | 'High'): { destructive: boolean; profile: string } {
  if (risk === 'High') return { destructive: true, profile: 'Strict' };
  if (risk === 'Medium') return { destructive: false, profile: 'Balanced' };
  return { destructive: false, profile: 'Permissive' };
}

interface PlanModePanelProps {
  bridge: any;
  notify: (msg: string, type?: 'info' | 'error') => void;
}

export default function PlanModePanel({ bridge, notify }: PlanModePanelProps) {
  const [intent, setIntent] = useState('');
  const [agentId, setAgentId] = useState('claude-code');
  const [loading, setLoading] = useState(false);
  const [planResult, setPlanResult] = useState<PlanResponse | null>(null);
  const [executionResult, setExecutionResult] = useState<any>(null);
  const [isMock, setIsMock] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handlePreviewPlan = async () => {
    if (!intent.trim()) {
      notify('Please enter a natural language intent', 'error');
      return;
    }
    if (!bridge) return;

    setLoading(true);
    setError(null);
    setExecutionResult(null);

    const argumentsPayload = {
      intent: intent.trim(),
      agent_id: agentId.trim() || undefined,
      plan: true
    };

    try {
      // Real network call to backend
      const raw = await bridge.fetchRaw('/api/v1/do', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          tool: 'tylluan_do',
          arguments: argumentsPayload
        })
      });

      const backendPlan: BackendPlan | undefined = raw?.result;
      if (backendPlan && backendPlan.status === 'plan') {
        const { destructive, profile } = riskToDisplay(backendPlan.risk_level);
        setPlanResult({
          plan_id: backendPlan.plan_id,
          plan: true,
          would_execute: {
            guild: backendPlan.guild,
            tool_name: backendPlan.tool,
            destructive,
            sandbox_profile: profile,
            args_preview: backendPlan.arguments,
          },
          routing_trace: [backendPlan.message],
        });
        setIsMock(false);
      } else {
        // Falling back to local simulation because backend execution is pending integration
        throw new Error("Backend did not return a 'plan' blueprint structure");
      }
    } catch (err: any) {
      console.warn("Plan Mode POST /api/v1/do failed or plan unsupported:", err.message);
      
      // Determine if intent is destructive based on keywords
      const isDestructive = /rm\s|delete\s|kill\s|drop\s|overwrite\s|remove\s|format\s/i.test(intent);
      const simulatedProfile = isDestructive ? 'Strict' : 'Balanced';
      const resolvedGuild = intent.toLowerCase().includes('coloquio') || intent.toLowerCase().includes('chat') ? 'coloquio' : 'bash';
      const resolvedTool = resolvedGuild === 'coloquio' ? 'post_to_channel' : 'execute_command';

      const mockPlan: PlanResponse = {
        plan: true,
        would_execute: {
          guild: resolvedGuild,
          tool_name: resolvedTool,
          destructive: isDestructive,
          sandbox_profile: simulatedProfile,
          args_preview: resolvedGuild === 'coloquio' 
            ? { channel: 'mision-activa', message: intent.trim() }
            : { command: intent.trim() }
        },
        routing_trace: [
          `matched '${resolvedGuild}' via keyword score ${isDestructive ? '0.94' : '0.82'}`,
          isDestructive 
            ? "detected dangerous intent fingerprint, enforcing sandbox: Strict" 
            : "standard query footprint, enforcing sandbox: Balanced"
        ]
      };

      setPlanResult(mockPlan);
      setIsMock(true);
      setError(`[MOCK FALLBACK] Server execution returned raw result or 404. Showing simulated blueprint.`);
    } finally {
      setLoading(false);
    }
  };

  const handleExecuteForReal = async () => {
    if (!planResult || !bridge) return;

    const details = planResult.would_execute;
    if (details.destructive) {
      const confirmText = `⚠️ WARNING: This intent is flagged as DESTRUCTIVE!\n\nGuild: ${details.guild}\nTool: ${details.tool_name}\nArguments: ${JSON.stringify(details.args_preview)}\n\nAre you sure you want to execute this real action on your system?`;
      if (!window.confirm(confirmText)) {
        return;
      }
    }

    setLoading(true);
    setExecutionResult(null);

    try {
      let result: any;
      if (!isMock && planResult.plan_id) {
        // Real plan: execute the exact stored plan via approve_action(requestId=plan_id),
        // per crates/tylluan-kernel/src/transport/server/handlers.rs's plan-store path.
        result = await bridge.fetchRaw('/api/v1/do', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            tool: 'approve_action',
            arguments: {
              requestId: planResult.plan_id,
              approved: true,
              grant_level: 'this_time',
            }
          })
        });
      } else {
        // Mock plan (no real plan_id to approve) — replay the intent for real execution.
        result = await bridge.fetchRaw('/api/v1/do', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            tool: 'tylluan_do',
            arguments: {
              intent: intent.trim(),
              agent_id: agentId.trim() || undefined,
              plan: false,
            }
          })
        });
      }
      setExecutionResult(result);
      notify('Execution completed successfully', 'info');
    } catch (err: any) {
      notify(`Execution failed: ${err.message}`, 'error');
      setExecutionResult({ error: err.message });
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <div>
        <h2 className="text-xl font-bold tracking-tight text-slate-50 flex items-center gap-2">
          <ShieldCheck className="w-5 h-5 text-emerald-500" />
          Plan Mode Sandbox
        </h2>
        <p className="text-xs text-slate-400">
          Preview intent routing, sandbox profiles, and argument blueprints before real execution.
        </p>
      </div>

      {/* Mock status warning */}
      {isMock && (
        <div className="p-3 bg-amber-500/10 border border-amber-500/20 text-amber-400 rounded-2xl flex items-center gap-3 text-xs leading-normal font-mono">
          <ServerOff className="w-4 h-4 flex-shrink-0 animate-pulse text-amber-500" />
          <div>
            <span className="font-bold">[SIMULATED PLAN PREVIEW] </span>
            {error || "GET/POST /api/v1/do plan mode is pending backend implementation. Showing simulated blueprint."}
          </div>
        </div>
      )}

      {/* Planner Card */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Input Panel */}
        <div className="p-6 bg-slate-900/60 border border-slate-850 rounded-2xl space-y-4">
          <div className="flex justify-between items-center">
            <span className="text-xs font-bold uppercase tracking-wider font-mono text-slate-400">Intent Editor</span>
            <div className="flex items-center gap-2">
              <span className="text-[10px] font-mono text-slate-500">Agent ID:</span>
              <input
                type="text"
                value={agentId}
                onChange={(e) => setAgentId(e.target.value)}
                placeholder="Agent ID..."
                className="bg-slate-950 border border-slate-800 rounded-lg px-2 py-1 text-[10px] font-mono focus:border-emerald-500 focus:outline-none w-28 text-slate-300"
              />
            </div>
          </div>

          <textarea
            value={intent}
            onChange={(e) => setIntent(e.target.value)}
            rows={5}
            placeholder="Type your natural language intent here... (e.g. 'list files in /tmp' or a destructive command like 'rm -rf /tmp/test')"
            className="w-full p-4 bg-slate-950 border border-slate-800 focus:border-emerald-500 focus:outline-none rounded-xl font-mono text-xs text-slate-200 placeholder-slate-600 resize-none leading-relaxed"
          />

          <div className="flex gap-3">
            <button
              onClick={handlePreviewPlan}
              disabled={loading || !intent.trim()}
              className="flex-1 py-2.5 bg-emerald-500 hover:bg-emerald-600 text-slate-950 font-bold font-mono text-xs rounded-xl flex items-center justify-center gap-1.5 transition-colors disabled:opacity-50"
            >
              <Eye className="w-4 h-4" />
              Preview Plan
            </button>

            {planResult && (
              <button
                onClick={handleExecuteForReal}
                disabled={loading}
                className="flex-1 py-2.5 bg-slate-900 border border-slate-800 hover:border-slate-700 hover:bg-slate-850/80 text-slate-50 font-bold font-mono text-xs rounded-xl flex items-center justify-center gap-1.5 transition-colors"
              >
                <Play className="w-4 h-4 text-emerald-400" />
                Execute for Real
              </button>
            )}
          </div>
        </div>

        {/* Blueprint Display Panel */}
        <div className="p-6 bg-slate-900/40 border border-slate-850 rounded-2xl flex flex-col justify-between min-h-[300px]">
          {planResult ? (
            <div className="space-y-4 font-mono text-xs">
              <div className="flex items-center justify-between border-b border-slate-800 pb-3">
                <span className="text-xs font-bold uppercase tracking-wider text-slate-400">Blueprint Preview</span>
                <span className={cn(
                  "px-2.5 py-0.5 rounded-full text-[9px] font-bold uppercase tracking-wider border flex items-center gap-1 leading-none",
                  planResult.would_execute.destructive
                    ? "bg-red-500/10 text-red-400 border-red-500/20"
                    : "bg-emerald-500/10 text-emerald-400 border-emerald-500/20"
                )}>
                  {planResult.would_execute.destructive ? (
                    <>
                      <AlertOctagon className="w-3 h-3" />
                      Destructive
                    </>
                  ) : (
                    <>
                      <CheckCircle className="w-3 h-3" />
                      Safe
                    </>
                  )}
                </span>
              </div>

              {/* Grid properties */}
              <div className="grid grid-cols-2 gap-3 p-3 rounded-xl bg-slate-950 border border-slate-850 text-[11px]">
                <div>
                  <span className="text-slate-500 block text-[9px] uppercase font-black">Target Guild</span>
                  <span className="text-slate-200 font-bold uppercase">{planResult.would_execute.guild}</span>
                </div>
                <div>
                  <span className="text-slate-500 block text-[9px] uppercase font-black">Routing Tool</span>
                  <span className="text-slate-200 font-bold">{planResult.would_execute.tool_name}</span>
                </div>
                <div>
                  <span className="text-slate-500 block text-[9px] uppercase font-black">Sandbox Profile</span>
                  <span className={cn(
                    "font-bold uppercase",
                    planResult.would_execute.sandbox_profile === 'Strict' && "text-red-400",
                    planResult.would_execute.sandbox_profile === 'Balanced' && "text-blue-400",
                    planResult.would_execute.sandbox_profile === 'Permissive' && "text-emerald-400"
                  )}>
                    {planResult.would_execute.sandbox_profile}
                  </span>
                </div>
                <div>
                  <span className="text-slate-500 block text-[9px] uppercase font-black">Operator Context</span>
                  <span className="text-slate-300">@{agentId}</span>
                </div>
              </div>

              {/* Routing Trace */}
              <div className="space-y-1">
                <span className="text-slate-500 text-[9px] uppercase font-black">Routing Decision Trace</span>
                <ul className="space-y-1">
                  {planResult.routing_trace.map((trace, index) => (
                    <li key={index} className="flex items-center gap-1.5 text-slate-400 text-[11px]">
                      <ChevronRight className="w-3.5 h-3.5 text-emerald-500 flex-shrink-0" />
                      <span>{trace}</span>
                    </li>
                  ))}
                </ul>
              </div>

              {/* Arguments Preview */}
              <div className="space-y-1.5">
                <span className="text-slate-500 text-[9px] uppercase font-black">Arguments Blueprint</span>
                <pre className="p-3 bg-slate-950 border border-slate-850 rounded-xl text-[10px] text-emerald-400/90 overflow-x-auto max-h-28">
                  {JSON.stringify(planResult.would_execute.args_preview, null, 2)}
                </pre>
              </div>
            </div>
          ) : (
            <div className="flex flex-col items-center justify-center h-full text-slate-500 font-mono text-xs py-12 gap-3">
              <HelpCircle className="w-12 h-12 text-slate-700 animate-pulse" />
              <div className="text-center">
                <p>No blueprint loaded</p>
                <p className="text-[10px] text-slate-600 mt-1">Enter an intent and click Preview Plan to analyze execution metrics.</p>
              </div>
            </div>
          )}

          {/* Execution Result Log */}
          {executionResult && (
            <div className="mt-4 pt-4 border-t border-slate-800 space-y-2 font-mono text-xs animate-in slide-in-from-bottom duration-200">
              <span className="text-slate-400 font-bold uppercase text-[9px] flex items-center gap-1">
                <Activity className="w-3.5 h-3.5 text-emerald-400 animate-pulse" />
                Real-Time Execution Logs
              </span>
              <pre className="p-3 bg-slate-950 border border-slate-850 rounded-xl text-[10px] text-slate-300 max-h-36 overflow-y-auto leading-relaxed">
                {JSON.stringify(executionResult, null, 2)}
              </pre>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
