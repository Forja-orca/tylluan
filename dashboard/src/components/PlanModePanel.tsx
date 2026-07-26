import React, { useState } from 'react';
import { 
  ShieldCheck, 
  Play, 
  Eye, 
  AlertOctagon, 
  CheckCircle2, 
  Activity,
  ChevronRight,
  ServerOff,
  CornerDownRight,
  UserCheck,
  Zap
} from 'lucide-react';
import { cn } from '../lib/utils';

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

export interface PlanResponse {
  plan_id?: string;
  plan: boolean;
  would_execute: PlanDetails;
  routing_trace: string[];
}

export function riskToDisplay(risk: 'Low' | 'Medium' | 'High'): { destructive: boolean; profile: string } {
  if (risk === 'High') return { destructive: true, profile: 'Strict' };
  if (risk === 'Medium') return { destructive: false, profile: 'Balanced' };
  return { destructive: false, profile: 'Permissive' };
}

export function detectDestructiveKeywords(intentText: string): boolean {
  return /rm\s|delete\s|kill\s|drop\s|overwrite\s|remove\s|format\s|git\s+reset|git\s+clean/i.test(intentText);
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
  const [error, setError] = useState<string | null>(null);

  const handlePreviewPlan = async () => {
    if (!intent.trim()) {
      notify('Please enter a natural language intent', 'error');
      return;
    }
    if (!bridge) {
      notify('Kernel bridge not connected', 'error');
      return;
    }

    setLoading(true);
    setError(null);
    setExecutionResult(null);

    const argumentsPayload = {
      intent: intent.trim(),
      agent_id: agentId.trim() || undefined,
      plan: true
    };

    try {
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
      } else {
        // Evaluate client-side dry-run estimate if raw result is standard JSON
        const isDestructive = detectDestructiveKeywords(intent);
        const resolvedGuild = intent.toLowerCase().includes('coloquio') || intent.toLowerCase().includes('chat') ? 'coloquio' : 'bash';
        const resolvedTool = resolvedGuild === 'coloquio' ? 'post_to_channel' : 'run_command';

        setPlanResult({
          plan: true,
          would_execute: {
            guild: resolvedGuild,
            tool_name: resolvedTool,
            destructive: isDestructive,
            sandbox_profile: isDestructive ? 'Strict' : 'Balanced',
            args_preview: raw?.result ?? { intent: intent.trim() }
          },
          routing_trace: [
            `Pre-flight dry-run evaluated for agent '${agentId}'`,
            isDestructive ? 'Flagged as DESTRUCTIVE intent' : 'Safe read-only execution footprint'
          ]
        });
      }
    } catch (err: any) {
      setError(`Failed to reach kernel on :4000: ${err.message}`);
      notify(`Plan Mode request failed: ${err.message}`, 'error');
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
      if (planResult.plan_id) {
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
    <div className="space-y-6 font-sans">
      {/* Header */}
      <div className="flex flex-col md:flex-row justify-between items-start md:items-center gap-4 p-5 bg-[#0B0F17]/90 border border-slate-800/80 rounded-2xl">
        <div>
          <div className="flex items-center gap-2">
            <span className="px-2 py-0.5 text-[9px] font-mono font-bold tracking-wider uppercase bg-[#00F5D4]/10 text-[#00F5D4] border border-[#00F5D4]/30 rounded">
              HITL Pre-Flight
            </span>
            <span className="px-2 py-0.5 text-[9px] font-mono font-bold tracking-wider uppercase bg-amber-400/10 text-amber-400 border border-amber-400/30 rounded">
              M31-P2 Contract
            </span>
          </div>
          <h2 className="text-xl font-bold tracking-tight text-slate-100 mt-2 flex items-center gap-2 font-mono">
            <ShieldCheck className="w-5 h-5 text-[#00F5D4]" />
            Plan Mode &amp; Human-in-the-Loop Cockpit
          </h2>
          <p className="text-xs text-slate-400 mt-0.5">
            Dry-run pre-flight execution blueprint before committing destructive actions to system or repositories.
          </p>
        </div>
      </div>

      {/* Input Section */}
      <div className="p-5 bg-[#0B0F17]/90 border border-slate-800/80 rounded-2xl space-y-4">
        <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
          <div className="md:col-span-3 space-y-1.5">
            <label className="text-xs font-mono font-bold text-slate-400 uppercase tracking-wider flex items-center gap-1.5">
              <Zap className="w-3.5 h-3.5 text-[#00F5D4]" />
              Natural Language Intent
            </label>
            <input
              type="text"
              value={intent}
              onChange={(e) => setIntent(e.target.value)}
              placeholder="e.g. git status, list active channels in coloquio, delete temp cache..."
              className="w-full px-4 py-2.5 bg-slate-950 border border-slate-800 focus:border-[#00F5D4]/60 text-slate-100 font-mono text-sm rounded-xl outline-none transition-all placeholder:text-slate-600"
              onKeyDown={(e) => e.key === 'Enter' && handlePreviewPlan()}
            />
          </div>

          <div className="space-y-1.5">
            <label className="text-xs font-mono font-bold text-slate-400 uppercase tracking-wider flex items-center gap-1.5">
              <UserCheck className="w-3.5 h-3.5 text-slate-400" />
              Agent ID Context
            </label>
            <select
              value={agentId}
              onChange={(e) => setAgentId(e.target.value)}
              className="w-full px-3 py-2.5 bg-slate-950 border border-slate-800 focus:border-[#00F5D4]/60 text-slate-200 font-mono text-xs rounded-xl outline-none transition-all"
            >
              <option value="claude-code">claude-code (Sonnet 4.6)</option>
              <option value="deep">deep (DeepSeek V4)</option>
              <option value="antigravity">antigravity (Gemini 3.5)</option>
              <option value="qwen">qwen (Qwen-Max)</option>
            </select>
          </div>
        </div>

        <div className="flex justify-end gap-3 pt-2">
          <button
            onClick={handlePreviewPlan}
            disabled={loading || !intent.trim()}
            className="flex items-center gap-2 px-5 py-2.5 bg-[#00F5D4]/10 hover:bg-[#00F5D4]/20 border border-[#00F5D4]/40 text-[#00F5D4] font-mono text-xs font-bold rounded-xl transition-all disabled:opacity-40"
          >
            <Eye className="w-4 h-4" />
            <span>{loading ? 'Evaluating Pre-Flight...' : 'Preview Action Plan (Dry-Run)'}</span>
          </button>
        </div>
      </div>

      {error && (
        <div className="p-4 bg-[#FF2E93]/10 border border-[#FF2E93]/30 text-[#FF2E93] rounded-xl text-xs font-mono">
          ⚠️ {error}
        </div>
      )}

      {/* Plan Preview Result */}
      {planResult && (
        <div className="p-5 bg-[#0B0F17]/90 border border-slate-800/80 rounded-2xl space-y-4 border-l-4 border-l-[#00F5D4]">
          <div className="flex flex-col md:flex-row justify-between items-start md:items-center gap-3 pb-3 border-b border-slate-800/80">
            <div>
              <div className="flex items-center gap-2 font-mono">
                <span className="text-xs text-slate-400">Target Guild:</span>
                <span className="px-2 py-0.5 text-xs font-bold bg-slate-900 text-[#00F5D4] border border-slate-700 rounded">
                  {planResult.would_execute.guild}
                </span>
                <ChevronRight className="w-3.5 h-3.5 text-slate-600" />
                <span className="text-xs text-slate-400">Tool:</span>
                <span className="px-2 py-0.5 text-xs font-bold bg-slate-900 text-slate-200 border border-slate-700 rounded">
                  {planResult.would_execute.tool_name}
                </span>
              </div>
            </div>

            <div className="flex items-center gap-2 font-mono">
              <span
                className={cn(
                  'px-2.5 py-1 text-xs font-bold rounded-lg border',
                  planResult.would_execute.destructive
                    ? 'bg-[#FF2E93]/10 text-[#FF2E93] border-[#FF2E93]/40'
                    : 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30'
                )}
              >
                {planResult.would_execute.destructive ? '⚠️ DESTRUCTIVE ACTION' : 'READ-ONLY / SAFE'}
              </span>
              <span className="px-2.5 py-1 text-xs font-bold bg-slate-900 text-slate-300 border border-slate-800 rounded-lg">
                Sandbox: {planResult.would_execute.sandbox_profile}
              </span>
            </div>
          </div>

          {/* Routing Trace */}
          <div className="space-y-1 font-mono text-xs">
            <span className="text-slate-500 text-[10px] uppercase font-bold tracking-wider">Routing Trace &amp; Fingerprint:</span>
            {planResult.routing_trace.map((step, idx) => (
              <div key={idx} className="flex items-center gap-2 text-slate-300">
                <CornerDownRight className="w-3 h-3 text-[#00F5D4]" />
                <span>{step}</span>
              </div>
            ))}
          </div>

          {/* Argument Payload Preview */}
          <div className="space-y-1 font-mono text-xs">
            <span className="text-slate-500 text-[10px] uppercase font-bold tracking-wider">Calculated Parameter Payload:</span>
            <pre className="p-3 bg-slate-950 border border-slate-850 text-emerald-400 rounded-xl text-xs overflow-x-auto">
              {JSON.stringify(planResult.would_execute.args_preview, null, 2)}
            </pre>
          </div>

          {/* Approval Action */}
          <div className="flex justify-end pt-2">
            <button
              onClick={handleExecuteForReal}
              disabled={loading}
              className={cn(
                'flex items-center gap-2 px-6 py-2.5 font-mono text-xs font-bold rounded-xl transition-all disabled:opacity-50 shadow-lg',
                planResult.would_execute.destructive
                  ? 'bg-[#FF2E93] hover:bg-[#FF2E93]/90 text-white shadow-[#FF2E93]/20'
                  : 'bg-[#00F5D4] hover:bg-[#00F5D4]/90 text-slate-950 shadow-[#00F5D4]/20'
              )}
            >
              <Play className="w-4 h-4 fill-current" />
              <span>{loading ? 'Executing Action...' : 'Approve & Execute Action For Real'}</span>
            </button>
          </div>
        </div>
      )}

      {/* Real Execution Output */}
      {executionResult && (
        <div className="p-5 bg-[#0B0F17]/90 border border-slate-800/80 rounded-2xl space-y-2 font-mono">
          <div className="flex items-center gap-2 text-xs font-bold text-emerald-400">
            <CheckCircle2 className="w-4 h-4" />
            Execution Completed Result:
          </div>
          <pre className="p-3.5 bg-slate-950 border border-slate-850 text-slate-200 rounded-xl text-xs overflow-x-auto">
            {JSON.stringify(executionResult, null, 2)}
          </pre>
        </div>
      )}
    </div>
  );
}
