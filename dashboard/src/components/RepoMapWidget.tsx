import React, { useState, useEffect } from 'react';
import {
  FolderGit2,
  FileCode2,
  FileCheck2,
  ServerOff,
  Clock,
  Check
} from 'lucide-react';

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
  bridge: any;
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
  const [isMock, setIsMock] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchRepoMap = async () => {
    if (!bridge) return;
    setLoading(true);
    setError(null);
    try {
      const res = await bridge.getRepoMap();
      if (res && (res.top_level_dirs || res.total_files !== undefined)) {
        setData(res);
        setIsMock(false);
      } else {
        throw new Error("Respuesta de mapa de código inválida del servidor");
      }
    } catch (err: any) {
      console.error("Repo Map API error:", err.message);
      setData(null);
      setIsMock(false);
      setError(`Error obteniendo topología del proyecto (GET /api/v1/repo-map): ${err.message}`);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchRepoMap();
  }, [bridge]);

  if (loading) {
    return (
      <div className="bg-slate-900/60 border border-slate-850 rounded-2xl p-4 flex items-center justify-center py-12">
        <FolderGit2 className="w-8 h-8 text-slate-700 animate-pulse" />
      </div>
    );
  }

  if (!data) return null;

  // Calculate max count for top_level_dirs bar scaling
  const maxFileCount = Math.max(...data.top_level_dirs.map(d => d.file_count), 1);

  return (
    <div className="bg-slate-900/60 border border-slate-850 rounded-2xl p-4 space-y-4">
      {/* Header */}
      <div className="flex items-center justify-between border-b border-slate-800 pb-3">
        <div className="flex items-center gap-2">
          <FolderGit2 className="w-4 h-4 text-emerald-400" />
          <span className="text-xs font-bold uppercase tracking-wider font-mono text-slate-200">Repository Blueprint Map</span>
        </div>
        <span className="text-[9px] font-mono text-slate-500 flex items-center gap-1">
          <Clock className="w-2.5 h-2.5" />
          Built {formatRelativeTime(data.built_at_unix)} ({data.build_duration_ms}ms)
        </span>
      </div>

      {/* Simulated warning banner */}
      {isMock && (
        <div className="p-2.5 bg-amber-500/10 border border-amber-500/20 text-amber-400 rounded-xl flex items-center gap-2 text-[10px] leading-normal font-mono">
          <ServerOff className="w-3.5 h-3.5 flex-shrink-0 animate-pulse text-amber-500" />
          <div>
            <span className="font-bold">[SIMULATED REPO MAP] </span>
            {error || "GET /api/v1/repo-map is pending backend integration."}
          </div>
        </div>
      )}

      {/* Totals row */}
      <div className="grid grid-cols-3 gap-2 text-center font-mono">
        <div className="p-2 bg-slate-950 border border-slate-850 rounded-lg">
          <div className="text-sm font-bold text-emerald-400">{data.total_files.toLocaleString()}</div>
          <div className="text-[9px] text-slate-500 uppercase">Files</div>
        </div>
        <div className="p-2 bg-slate-950 border border-slate-850 rounded-lg">
          <div className="text-sm font-bold text-emerald-400">{data.total_dirs.toLocaleString()}</div>
          <div className="text-[9px] text-slate-500 uppercase">Dirs</div>
        </div>
        <div className="p-2 bg-slate-950 border border-slate-850 rounded-lg">
          <div className="text-sm font-bold text-emerald-400">{data.total_lines.toLocaleString()}</div>
          <div className="text-[9px] text-slate-500 uppercase">Lines</div>
        </div>
      </div>

      {/* Top Level Dirs Treemap-like Bars */}
      <div className="space-y-2.5">
        <div className="text-[9px] font-mono text-slate-500 uppercase tracking-widest">Directories file distribution</div>
        <div className="space-y-2 font-mono text-xs">
          {data.top_level_dirs.map((dir) => {
            const percentage = (dir.file_count / maxFileCount) * 100;
            return (
              <div key={dir.name} className="space-y-1">
                <div className="flex justify-between items-center text-[10px]">
                  <span className="text-slate-300 font-bold">/{dir.name}</span>
                  <span className="text-slate-500">{dir.file_count} files</span>
                </div>
                <div className="w-full bg-slate-950 rounded-full h-1.5 overflow-hidden border border-slate-850">
                  <div 
                    className="bg-emerald-500/80 h-full rounded-full transition-all duration-500" 
                    style={{ width: `${percentage}%` }}
                  />
                </div>
              </div>
            );
          })}
        </div>
      </div>

      {/* Language Breakdown Chips */}
      <div className="space-y-2 pt-2 border-t border-slate-800/40">
        <div className="text-[9px] font-mono text-slate-500 uppercase tracking-widest flex items-center gap-1">
          <FileCode2 className="w-3 h-3 text-slate-450" />
          <span>Languages</span>
        </div>
        <div className="flex flex-wrap gap-1.5">
          {Object.entries(data.languages)
            .sort(([, a], [, b]) => b.files - a.files)
            .map(([lang, stats]) => (
            <span
              key={lang}
              className="px-2 py-0.5 rounded-lg bg-slate-950 border border-slate-850 text-[10px] font-mono text-slate-300 flex items-center gap-1"
            >
              <span className="text-emerald-400 font-black">{lang}</span>
              <span className="text-slate-550">({stats.files} files, {stats.pct.toFixed(1)}%)</span>
            </span>
          ))}
        </div>
      </div>

      {/* Key Files Inventory (backend only reports files it found -- not a full checklist) */}
      <div className="space-y-2 pt-2 border-t border-slate-800/40">
        <div className="text-[9px] font-mono text-slate-500 uppercase tracking-widest flex items-center gap-1">
          <FileCheck2 className="w-3 h-3 text-slate-450" />
          <span>Key files found</span>
        </div>
        <div className="grid grid-cols-2 gap-2 text-[10px] font-mono">
          {data.key_files.map((f) => (
            <div
              key={f.path}
              className="flex items-center gap-1.5 p-1.5 rounded-lg border bg-emerald-500/5 border-emerald-500/10 text-slate-300"
            >
              <Check className="w-3 h-3 text-emerald-400 flex-shrink-0" />
              <span className="truncate">{f.path}</span>
              <span className="text-slate-600 ml-auto">{f.kind}</span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
