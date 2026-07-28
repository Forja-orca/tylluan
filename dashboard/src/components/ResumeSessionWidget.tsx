import React, { useState } from 'react';
import { 
  History, 
  Search, 
  RefreshCw, 
  Clock, 
  HelpCircle,
  AlertTriangle,
  ServerOff
} from 'lucide-react';
import { cn } from '../lib/utils';

export interface AgentMemorySummary {
  summary: string | null;
  node_id?: string;
  created_at?: string;
}

interface ResumeSessionWidgetProps {
  bridge: any;
}

function formatRelativeTime(dateStr: string): string {
  try {
    const date = new Date(dateStr);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffSecs = Math.floor(diffMs / 1000);
    const diffMins = Math.floor(diffSecs / 60);
    const diffHours = Math.floor(diffMins / 60);
    const diffDays = Math.floor(diffHours / 24);

    if (diffSecs < 60) return 'just now';
    if (diffMins < 60) return `${diffMins}m ago`;
    if (diffHours < 24) return `${diffHours}h ago`;
    return `${diffDays}d ago`;
  } catch {
    return dateStr;
  }
}

export default function ResumeSessionWidget({ bridge }: ResumeSessionWidgetProps) {
  const [agentId, setAgentId] = useState('claude-code');
  const [loading, setLoading] = useState(false);
  const [result, setResult] = useState<AgentMemorySummary | null>(null);
  const [error, setError] = useState<string | null>(null);

  const handleCheckSession = async () => {
    if (!agentId.trim()) return;
    if (!bridge) return;

    setLoading(true);
    setError(null);
    try {
      const data = await bridge.getAgentMemorySummary(agentId.trim());
      setResult(data);
    } catch (err: any) {
      console.error("Session summary fetch error:", err.message);
      setResult(null);
      setError(`Error al consultar contexto de sesión (${agentId.trim()}): ${err.message}`);
    } finally {
      setLoading(false);
    }
  };

  // Helper to split numbered lines into list items for render
  const renderSummaryLines = (content: string) => {
    return content.split('\n').map((line, idx) => {
      const isListItem = /^\d+\.\s/.test(line);
      return (
        <p 
          key={idx} 
          className={cn(
            "text-[11px] font-mono leading-relaxed text-slate-300",
            isListItem ? "pl-4 -indent-4 mb-2 text-slate-200" : "mb-3 font-semibold text-slate-400"
          )}
        >
          {line}
        </p>
      );
    });
  };

  return (
    <div className="bg-slate-900/60 border border-slate-850 rounded-2xl p-4 space-y-4">
      {/* Header */}
      <div className="flex items-center justify-between border-b border-slate-800 pb-3">
        <div className="flex items-center gap-2">
          <History className="w-4 h-4 text-emerald-400" />
          <span className="text-xs font-bold uppercase tracking-wider font-mono text-slate-200">Resume Session Context</span>
        </div>
        <span className="text-[9px] font-mono text-slate-500">Security Layer</span>
      </div>

      {/* Control Input */}
      <div className="flex items-center gap-2">
        <div className="relative flex-1">
          <Search className="absolute left-2.5 top-2 w-3.5 h-3.5 text-slate-500" />
          <input
            type="text"
            value={agentId}
            onChange={(e) => setAgentId(e.target.value)}
            placeholder="Search Agent ID..."
            className="w-full pl-8 pr-3 py-1.5 bg-slate-950 border border-slate-850 focus:border-emerald-500 focus:outline-none rounded-xl text-[11px] font-mono text-slate-300 placeholder-slate-600"
          />
        </div>
        <button
          onClick={handleCheckSession}
          disabled={loading || !agentId.trim()}
          className="px-3 py-1.5 bg-slate-800 hover:bg-slate-700 disabled:opacity-50 text-[11px] font-bold font-mono text-slate-300 rounded-xl flex items-center gap-1.5 transition-all border border-slate-700"
        >
          <RefreshCw className={cn("w-3 h-3 text-emerald-400", loading && "animate-spin")} />
          Check
        </button>
      </div>

      {/* Results Display */}
      {result ? (
        result.summary ? (
          <div className="space-y-3">
            <div className="p-3 bg-slate-950 border border-slate-850 rounded-xl space-y-2.5">
              <div className="flex justify-between items-center text-[10px] font-mono text-slate-500">
                <span>Agent: @{agentId}</span>
                {result.created_at && (
                  <span className="flex items-center gap-1">
                    <Clock className="w-3 h-3" />
                    {formatRelativeTime(result.created_at)}
                  </span>
                )}
              </div>
              <div className="border-t border-slate-900 pt-2.5">
                {renderSummaryLines(result.summary)}
              </div>
            </div>
          </div>
        ) : (
          <div className="p-4 bg-slate-950 border border-slate-850 rounded-xl flex flex-col items-center justify-center text-center font-mono py-6">
            <AlertTriangle className="w-8 h-8 text-slate-600 mb-2" />
            <p className="text-[10px] text-slate-400">No previous sessions found</p>
            <p className="text-[9px] text-slate-600 mt-1">There are no memory consolidation logs registered for @{agentId}.</p>
          </div>
        )
      ) : (
        <div className="p-4 bg-slate-950 border border-slate-850 rounded-xl flex flex-col items-center justify-center text-center font-mono py-8">
          <HelpCircle className="w-8 h-8 text-slate-700 animate-pulse mb-2" />
          <p className="text-[10px] text-slate-500">Enter agent ID to pull summary</p>
        </div>
      )}
    </div>
  );
}
