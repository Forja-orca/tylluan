import React, { useState, useEffect, useCallback } from 'react';
import {
  Clock,
  Check
} from 'lucide-react';
import type { NexusBridge } from '../lib/nexus-bridge';

// Real backend contract (crates/tylluan-kernel/src/repo_map.rs, M31-P4).
interface TopLevelDir {
  name: string;
  file_count: number;
  dir_count: number;
}

interface LangStats {
  files: number;
  lines: number;
  pct: number;
}

interface KeyFile {
  path: string;
  kind: string;
}

interface RepoMapData {
  root: string;
  built_at_unix: number;
  build_duration_ms: number;
  total_files: number;
  total_dirs: number;
  total_lines: number;
  languages: Record<string, LangStats>;
  top_level_dirs: TopLevelDir[];
  key_files: KeyFile[];
  identifiers: Record<string, string[]>;
}

interface RepoMapWidgetProps {
  bridge: NexusBridge | null;
}

function formatRelativeTime(unixSecs: number): string {
  try {
    const date = new Date(unixSecs * 1000);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffSecs = Math.floor(diffMs / 1000);
    const diffMins = Math.floor(diffSecs / 60);
    const diffHours = Math.floor(diffMins / 60);

    if (diffSecs < 60) return 'just now';
    if (diffMins < 60) return `${diffMins}m ago`;
    if (diffHours < 24) return `${diffHours}h ago`;
    return date.toLocaleDateString();
  } catch {
    return 'unknown';
  }
}

export default function RepoMapWidget({ bridge }: RepoMapWidgetProps) {
  const [data, setData] = useState<RepoMapData | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchRepoMap = useCallback(async () => {
    if (!bridge) return;
    setLoading(true);
    setError(null);
    try {
      const res = await bridge.getRepoMap();
      if (res && (res.top_level_dirs || res.total_files !== undefined)) {
        setData(res);
      } else {
        throw new Error("Invalid repository map response from server");
      }
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      console.error("Repo Map API error:", msg);
      setData(null);
      setError(`Failed to fetch project topology (GET /api/v1/repo-map): ${msg}`);
    } finally {
      setLoading(false);
    }
  }, [bridge]);

  useEffect(() => {
    fetchRepoMap();
  }, [fetchRepoMap]);

  if (loading) {
    return (
      <div className="bg-card border border-border rounded p-4 flex items-center justify-center py-12">
        <span className="text-xs text-muted-foreground font-mono">Cargando mapa del repositorio...</span>
      </div>
    );
  }

  if (error && !data) {
    return (
      <div className="bg-card border border-red-500/30 rounded p-4 text-center text-xs text-red-600 font-mono">
        {error}
      </div>
    );
  }

  if (!data) return null;

  // Calculate max count for top_level_dirs bar scaling
  const maxFileCount = Math.max(...data.top_level_dirs.map(d => d.file_count), 1);

  return (
    <div className="bg-card border border-border rounded p-4 space-y-4 font-sans">
      {/* Header */}
      <div className="flex items-center justify-between border-b border-border pb-3">
        <div className="flex items-center gap-2">
          <span className="text-xs font-bold text-foreground">Mapa de Topología del Repositorio</span>
        </div>
        <span className="text-[11px] font-mono text-muted-foreground flex items-center gap-1">
          <Clock className="w-3 h-3 text-muted-foreground" aria-hidden="true" />
          Compilado {formatRelativeTime(data.built_at_unix)} ({data.build_duration_ms}ms)
        </span>
      </div>

      {/* Totals row */}
      <div className="grid grid-cols-3 gap-2 text-center font-mono">
        <div className="p-2 bg-muted/40 border border-border rounded">
          <div className="text-sm font-bold text-primary">{data.total_files.toLocaleString()}</div>
          <div className="text-[11px] text-muted-foreground">Archivos</div>
        </div>
        <div className="p-2 bg-muted/40 border border-border rounded">
          <div className="text-sm font-bold text-primary">{data.total_dirs.toLocaleString()}</div>
          <div className="text-[11px] text-muted-foreground">Directorios</div>
        </div>
        <div className="p-2 bg-muted/40 border border-border rounded">
          <div className="text-sm font-bold text-primary">{data.total_lines.toLocaleString()}</div>
          <div className="text-[11px] text-muted-foreground">Líneas</div>
        </div>
      </div>

      {/* Top Level Dirs Treemap-like Bars */}
      <div className="space-y-2.5">
        <div className="text-xs font-bold text-foreground">Distribución de archivos por directorio</div>
        <div className="space-y-2 font-mono text-xs">
          {data.top_level_dirs.map((dir) => {
            const percentage = (dir.file_count / maxFileCount) * 100;
            return (
              <div key={dir.name} className="space-y-1">
                <div className="flex justify-between items-center text-[11px]">
                  <span className="text-foreground font-bold">/{dir.name}</span>
                  <span className="text-muted-foreground">{dir.file_count} archivos</span>
                </div>
                <div className="w-full bg-muted rounded h-1.5 overflow-hidden border border-border">
                  <div 
                    className="bg-primary h-full rounded transition-all duration-300" 
                    style={{ width: `${percentage}%` }}
                  />
                </div>
              </div>
            );
          })}
        </div>
      </div>

      {/* Language Breakdown Chips */}
      <div className="space-y-2 pt-2 border-t border-border">
        <div className="text-xs font-bold text-foreground">
          Lenguajes
        </div>
        <div className="flex flex-wrap gap-1.5">
          {Object.entries(data.languages)
            .sort(([, a], [, b]) => b.files - a.files)
            .map(([lang, stats]) => (
            <span
              key={lang}
              className="px-2 py-0.5 rounded bg-muted border border-border text-[11px] font-mono text-foreground flex items-center gap-1"
            >
              <span className="text-primary font-bold">{lang}</span>
              <span className="text-muted-foreground">({stats.files} archivos, {stats.pct.toFixed(1)}%)</span>
            </span>
          ))}
        </div>
      </div>

      {/* Key Files Inventory */}
      <div className="space-y-2 pt-2 border-t border-border">
        <div className="text-xs font-bold text-foreground">
          Archivos clave detectados
        </div>
        <div className="grid grid-cols-2 gap-2 text-[11px] font-mono">
          {data.key_files.map((f) => (
            <div
              key={f.path}
              className="flex items-center gap-1.5 p-1.5 rounded border border-border bg-muted/30 text-foreground"
            >
              <Check className="w-3 h-3 text-emerald-600 flex-shrink-0" />
              <span className="truncate">{f.path}</span>
              <span className="text-muted-foreground ml-auto">{f.kind}</span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
