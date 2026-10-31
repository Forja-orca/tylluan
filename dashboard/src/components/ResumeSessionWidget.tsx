import React, { useState } from 'react';
import { 
  Search, 
  RefreshCw, 
  Clock, 
  HelpCircle,
  AlertTriangle
} from 'lucide-react';
import type { NexusBridge } from '../lib/nexus-bridge';
import { cn } from '../lib/utils';

export interface AgentMemorySummary {
  summary: string | null;
  node_id?: string;
  created_at?: string;
}

interface ResumeSessionWidgetProps {
  bridge: NexusBridge | null;
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
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      console.error("Session summary fetch error:", msg);
      setResult(null);
      setError(`Failed to retrieve session context (${agentId.trim()}): ${msg}`);
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
            "text-[11px] font-mono leading-relaxed text-muted-foreground",
            isListItem ? "pl-4 -indent-4 mb-2 text-foreground" : "mb-3 font-semibold text-foreground"
          )}
        >
          {line}
        </p>
      );
    });
  };

  return (
    <div className="bg-card border border-border rounded p-4 space-y-4 font-sans">
      {/* Header */}
      <div className="flex items-center justify-between border-b border-border pb-3">
        <div className="flex items-center gap-2">
          <span className="text-xs font-bold text-foreground">Reanudar Contexto de Sesión</span>
        </div>
        <span className="text-[11px] font-mono text-muted-foreground">Capa de Seguridad</span>
      </div>

      {/* Control Input */}
      <div className="flex items-center gap-2">
        <div className="relative flex-1">
          <Search className="absolute left-2.5 top-2 w-3.5 h-3.5 text-muted-foreground" aria-hidden="true" />
          <input
            type="text"
            value={agentId}
            onChange={(e) => setAgentId(e.target.value)}
            placeholder="Buscar ID de agente..."
            className="w-full pl-8 pr-3 py-1.5 bg-card border border-border focus:border-primary focus:outline-none rounded text-xs font-mono text-foreground placeholder:text-muted-foreground"
          />
        </div>
        <button
          onClick={handleCheckSession}
          disabled={loading || !agentId.trim()}
          className="px-3 py-1.5 bg-primary hover:bg-primary/90 disabled:opacity-50 text-xs font-bold font-mono text-primary-foreground rounded flex items-center gap-1.5 transition-colors border border-primary"
        >
          <RefreshCw className={cn("w-3 h-3 text-primary-foreground", loading && "animate-spin")} aria-hidden="true" />
          Consultar
        </button>
      </div>

      {error && (
        <div className="p-3 bg-red-500/10 border border-red-500/30 rounded text-xs font-mono text-red-600">
          {error}
        </div>
      )}

      {/* Results Display */}
      {result ? (
        result.summary ? (
          <div className="space-y-3">
            <div className="p-3 bg-muted/30 border border-border rounded space-y-2.5">
              <div className="flex justify-between items-center text-[11px] font-mono text-muted-foreground">
                <span>Agente: @{agentId}</span>
                {result.created_at && (
                  <span className="flex items-center gap-1">
                    <Clock className="w-3 h-3 text-muted-foreground" aria-hidden="true" />
                    {formatRelativeTime(result.created_at)}
                  </span>
                )}
              </div>
              <div className="border-t border-border/60 pt-2.5">
                {renderSummaryLines(result.summary)}
              </div>
            </div>
          </div>
        ) : (
          <div className="p-4 bg-muted/20 border border-border rounded flex flex-col items-center justify-center text-center font-mono py-6">
            <AlertTriangle className="w-6 h-6 text-muted-foreground mb-2" aria-hidden="true" />
            <p className="text-xs text-foreground font-semibold">Sin sesiones previas</p>
            <p className="text-[11px] text-muted-foreground mt-1">No hay registros de consolidación de memoria para @{agentId}.</p>
          </div>
        )
      ) : (
        <div className="p-4 bg-muted/20 border border-border rounded flex flex-col items-center justify-center text-center font-mono py-8">
          <HelpCircle className="w-6 h-6 text-muted-foreground mb-2" aria-hidden="true" />
          <p className="text-xs text-muted-foreground">Introduce el ID de un agente para consultar el resumen</p>
        </div>
      )}
    </div>
  );
}
