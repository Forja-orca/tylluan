import React, { useState, useEffect, useCallback, useMemo } from 'react';
import {
  FolderArchive,
  Search,
  RefreshCw,
  Clock,
  User,
  HardDrive,
  CheckCircle2,
  AlertTriangle,
  Copy,
  Check,
  X,
  FileCode,
  Hash,
  Layers,
  ArrowUpDown,
  Filter,
  Eye,
  FileBox,
  ShieldCheck,
  Activity,
  ChevronDown
} from 'lucide-react';
import { cn } from '../lib/utils';
import { usePolling } from '../hooks/usePolling';

export interface OutputFileEntry {
  path: string;
  bytes: number;
  sha256: string;
}

export interface OutputManifest {
  schema_version: number;
  guild: string;
  tool: string;
  requested_by: string;
  run_id: string;
  created_at: number;
  call_success: boolean;
  delivery_status: string;
  files: OutputFileEntry[];
  claimed_outputs_verified?: number;
  window_scan_started_unix?: number;
}

export interface RunSummary {
  guild: string;
  run_id: string;
  created_at: number;
  requested_by: string;
  tool: string;
  files_count: number;
  total_bytes: number;
  delivery_status: string;
}

interface OutputsLedgerPanelProps {
  bridge: {
    fetchRaw: (url: string, init?: RequestInit) => Promise<any>;
  } | null;
  notify?: (msg: string, type?: 'info' | 'error') => void;
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(2))} ${sizes[i]}`;
}

function formatRelativeTime(unixSecs: number): string {
  if (!unixSecs) return 'unknown';
  const now = Math.floor(Date.now() / 1000);
  const diff = now - unixSecs;
  if (diff < 0) return 'just now';
  if (diff < 60) return `${diff}s ago`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return `${Math.floor(diff / 86400)}d ago`;
}

export function OutputsLedgerPanel({ bridge, notify }: OutputsLedgerPanelProps) {
  const [runs, setRuns] = useState<RunSummary[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Filters
  const [guildFilter, setGuildFilter] = useState<string>('all');
  const [statusFilter, setStatusFilter] = useState<string>('all');
  const [searchQuery, setSearchQuery] = useState('');
  const [copiedKey, setCopiedKey] = useState<string | null>(null);

  // Manifest modal state
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const [manifestLoading, setManifestLoading] = useState(false);
  const [selectedManifest, setSelectedManifest] = useState<OutputManifest | null>(null);
  const [manifestError, setManifestError] = useState<string | null>(null);
  const [manifestViewMode, setManifestViewMode] = useState<'files' | 'json'>('files');

  const fetchRuns = useCallback(async () => {
    if (!bridge) return;
    setLoading(true);
    setError(null);
    try {
      const query = guildFilter !== 'all' ? `?guild=${encodeURIComponent(guildFilter)}&limit=100` : '?limit=100';
      const res = await bridge.fetchRaw(`/api/v1/outputs${query}`);
      if (res && Array.isArray(res.runs)) {
        setRuns(res.runs);
      } else {
        setRuns([]);
      }
    } catch (err: any) {
      console.error('Failed to fetch outputs ledger:', err);
      setError(err.message || 'Failed to fetch outputs ledger');
    } finally {
      setLoading(false);
    }
  }, [bridge, guildFilter]);

  useEffect(() => {
    fetchRuns();
  }, [fetchRuns]);

  usePolling('outputs-ledger-list', fetchRuns, { interval: 'medium', enabled: !!bridge });

  const fetchManifest = useCallback(async (runId: string) => {
    if (!bridge) return;
    setSelectedRunId(runId);
    setManifestLoading(true);
    setManifestError(null);
    setSelectedManifest(null);
    try {
      const data = await bridge.fetchRaw(`/api/v1/outputs/${encodeURIComponent(runId)}/manifest`);
      if (data && data.run_id) {
        setSelectedManifest(data);
      } else {
        setManifestError('Invalid manifest format returned from server');
      }
    } catch (err: any) {
      console.error(`Failed to fetch manifest for ${runId}:`, err);
      setManifestError(err.message || `Failed to fetch manifest for ${runId}`);
    } finally {
      setManifestLoading(false);
    }
  }, [bridge]);

  const copyToClipboard = (text: string, key: string) => {
    navigator.clipboard.writeText(text);
    setCopiedKey(key);
    if (notify) notify('Copied to clipboard', 'info');
    setTimeout(() => setCopiedKey(null), 2000);
  };

  // Distinct guilds list from runs
  const availableGuilds = useMemo(() => {
    const set = new Set<string>();
    runs.forEach(r => {
      if (r.guild) set.add(r.guild);
    });
    return Array.from(set).sort();
  }, [runs]);

  // Filtered runs
  const filteredRuns = useMemo(() => {
    return runs.filter(run => {
      if (guildFilter !== 'all' && run.guild !== guildFilter) return false;
      if (statusFilter !== 'all' && run.delivery_status !== statusFilter) return false;
      if (searchQuery.trim()) {
        const q = searchQuery.toLowerCase();
        const matchesRunId = run.run_id.toLowerCase().includes(q);
        const matchesGuild = run.guild.toLowerCase().includes(q);
        const matchesTool = run.tool.toLowerCase().includes(q);
        const matchesAuthor = run.requested_by.toLowerCase().includes(q);
        if (!matchesRunId && !matchesGuild && !matchesTool && !matchesAuthor) return false;
      }
      return true;
    });
  }, [runs, guildFilter, statusFilter, searchQuery]);

  // Aggregate stats
  const stats = useMemo(() => {
    const totalRuns = runs.length;
    const totalFiles = runs.reduce((acc, r) => acc + (r.files_count || 0), 0);
    const totalBytes = runs.reduce((acc, r) => acc + (r.total_bytes || 0), 0);
    const totalGuilds = availableGuilds.length;
    return { totalRuns, totalFiles, totalBytes, totalGuilds };
  }, [runs, availableGuilds]);

  return (
    <div className="space-y-6">
      {/* Header Banner */}
      <div className="flex flex-col md:flex-row justify-between items-start md:items-center gap-4 bg-slate-900/40 p-5 rounded-xl border border-slate-800">
        <div>
          <div className="flex items-center gap-2.5">
            <div className="p-2 rounded-lg bg-emerald-500/10 border border-emerald-500/20 text-emerald-400">
              <FolderArchive className="w-5 h-5" />
            </div>
            <div>
              <h2 className="text-base font-bold text-slate-100 flex items-center gap-2">
                Outputs Ledger
                <span className="text-[11px] font-mono font-normal px-2 py-0.5 rounded-full bg-slate-800 border border-slate-700 text-slate-400">
                  bwc-d0fb0812
                </span>
              </h2>
              <p className="text-xs text-slate-400 mt-0.5">
                Read-only index of immutable artifacts produced by guilds under <code className="text-emerald-400 font-mono text-[11px]">data/outputs/</code>
              </p>
            </div>
          </div>
        </div>

        <div className="flex items-center gap-3">
          <button
            onClick={fetchRuns}
            disabled={loading}
            className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-mono font-medium rounded-lg border border-slate-700 bg-slate-800/80 text-slate-300 hover:text-white hover:border-slate-600 transition-all disabled:opacity-50"
            title="Refresh outputs list"
          >
            <RefreshCw className={cn("w-3.5 h-3.5", loading && "animate-spin text-emerald-400")} />
            <span>Refresh</span>
          </button>
        </div>
      </div>

      {/* Aggregate Stats Cards */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        <div className="bg-slate-900/50 border border-slate-800/80 rounded-xl p-4">
          <div className="flex items-center justify-between">
            <span className="text-xs font-mono text-slate-400">Indexed Runs</span>
            <Layers className="w-4 h-4 text-emerald-400" />
          </div>
          <div className="text-2xl font-bold font-mono text-slate-100 mt-1">
            {stats.totalRuns}
          </div>
          <div className="text-[11px] text-slate-500 mt-0.5">
            {runs.filter(r => r.delivery_status === 'ok').length} verified ok
          </div>
        </div>

        <div className="bg-slate-900/50 border border-slate-800/80 rounded-xl p-4">
          <div className="flex items-center justify-between">
            <span className="text-xs font-mono text-slate-400">Total Artifacts</span>
            <FileBox className="w-4 h-4 text-blue-400" />
          </div>
          <div className="text-2xl font-bold font-mono text-slate-100 mt-1">
            {stats.totalFiles}
          </div>
          <div className="text-[11px] text-slate-500 mt-0.5">
            Individual files tracked
          </div>
        </div>

        <div className="bg-slate-900/50 border border-slate-800/80 rounded-xl p-4">
          <div className="flex items-center justify-between">
            <span className="text-xs font-mono text-slate-400">Aggregated Size</span>
            <HardDrive className="w-4 h-4 text-purple-400" />
          </div>
          <div className="text-2xl font-bold font-mono text-slate-100 mt-1">
            {formatBytes(stats.totalBytes)}
          </div>
          <div className="text-[11px] text-slate-500 mt-0.5">
            Across all guild runs
          </div>
        </div>

        <div className="bg-slate-900/50 border border-slate-800/80 rounded-xl p-4">
          <div className="flex items-center justify-between">
            <span className="text-xs font-mono text-slate-400">Active Guilds</span>
            <Activity className="w-4 h-4 text-amber-400" />
          </div>
          <div className="text-2xl font-bold font-mono text-slate-100 mt-1">
            {stats.totalGuilds}
          </div>
          <div className="text-[11px] text-slate-500 mt-0.5">
            {availableGuilds.join(', ') || 'None'}
          </div>
        </div>
      </div>

      {/* Filter and Search Bar */}
      <div className="flex flex-col md:flex-row gap-3 items-stretch md:items-center justify-between bg-slate-900/30 p-3 rounded-xl border border-slate-800/80">
        <div className="flex flex-1 items-center gap-2 bg-slate-950/60 border border-slate-800 rounded-lg px-3 py-1.5 focus-within:border-emerald-500/50 transition-colors">
          <Search className="w-4 h-4 text-slate-500" />
          <input
            type="text"
            placeholder="Search run ID, guild, tool, author..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="bg-transparent text-xs text-slate-200 placeholder-slate-500 focus:outline-none w-full font-mono"
          />
          {searchQuery && (
            <button onClick={() => setSearchQuery('')} className="text-slate-500 hover:text-slate-300">
              <X className="w-3.5 h-3.5" />
            </button>
          )}
        </div>

        <div className="flex items-center gap-2 flex-wrap">
          {/* Guild Filter */}
          <div className="flex items-center gap-1.5 text-xs font-mono bg-slate-950/60 border border-slate-800 rounded-lg px-2.5 py-1.5 text-slate-300">
            <Filter className="w-3.5 h-3.5 text-slate-500" />
            <span className="text-slate-500">Guild:</span>
            <select
              value={guildFilter}
              onChange={(e) => setGuildFilter(e.target.value)}
              className="bg-transparent border-none text-slate-200 focus:outline-none font-mono cursor-pointer"
            >
              <option value="all" className="bg-slate-900 text-slate-200">All Guilds</option>
              {availableGuilds.map(g => (
                <option key={g} value={g} className="bg-slate-900 text-slate-200">{g}</option>
              ))}
            </select>
          </div>

          {/* Status Filter */}
          <div className="flex items-center gap-1.5 text-xs font-mono bg-slate-950/60 border border-slate-800 rounded-lg px-2.5 py-1.5 text-slate-300">
            <span className="text-slate-500">Status:</span>
            <select
              value={statusFilter}
              onChange={(e) => setStatusFilter(e.target.value)}
              className="bg-transparent border-none text-slate-200 focus:outline-none font-mono cursor-pointer"
            >
              <option value="all" className="bg-slate-900 text-slate-200">All</option>
              <option value="ok" className="bg-slate-900 text-emerald-400">ok</option>
              <option value="partial" className="bg-slate-900 text-amber-400">partial</option>
            </select>
          </div>
        </div>
      </div>

      {/* Error alert if any */}
      {error && (
        <div className="bg-red-500/10 border border-red-500/30 rounded-xl p-4 flex items-center gap-3 text-red-400 text-xs font-mono">
          <AlertTriangle className="w-4 h-4 flex-shrink-0" />
          <span>{error}</span>
        </div>
      )}

      {/* Main Runs Table / List */}
      <div className="bg-slate-900/40 border border-slate-800 rounded-xl overflow-hidden">
        {filteredRuns.length === 0 ? (
          <div className="p-12 text-center">
            <FolderArchive className="w-12 h-12 text-slate-600 mx-auto mb-3" />
            <h3 className="text-sm font-bold text-slate-300">No output runs found</h3>
            <p className="text-xs text-slate-500 max-w-md mx-auto mt-1">
              {runs.length === 0
                ? "Guilds have not generated indexed artifacts yet. When a guild writes files under data/outputs/<guild>/, manifests are automatically recorded."
                : "No runs match your current filters."}
            </p>
          </div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-left text-xs font-mono border-collapse">
              <thead>
                <tr className="border-b border-slate-800 bg-slate-900/80 text-slate-400">
                  <th className="py-3 px-4 font-semibold">Run ID</th>
                  <th className="py-3 px-4 font-semibold">Guild & Tool</th>
                  <th className="py-3 px-4 font-semibold">Requested By</th>
                  <th className="py-3 px-4 font-semibold">Artifacts</th>
                  <th className="py-3 px-4 font-semibold">Total Size</th>
                  <th className="py-3 px-4 font-semibold">Status</th>
                  <th className="py-3 px-4 font-semibold">Created</th>
                  <th className="py-3 px-4 font-semibold text-right">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-800/60">
                {filteredRuns.map((run) => {
                  const isOk = run.delivery_status === 'ok';
                  return (
                    <tr
                      key={run.run_id}
                      className="hover:bg-slate-800/30 transition-colors group"
                    >
                      <td className="py-3 px-4 text-slate-200">
                        <div className="flex items-center gap-1.5">
                          <span className="font-mono text-emerald-400" title={run.run_id}>
                            {run.run_id.length > 12 ? `${run.run_id.substring(0, 12)}…` : run.run_id}
                          </span>
                          <button
                            onClick={() => copyToClipboard(run.run_id, `run_${run.run_id}`)}
                            className="text-slate-500 hover:text-slate-300 p-0.5 rounded opacity-0 group-hover:opacity-100 transition-opacity"
                            title="Copy full Run ID"
                          >
                            {copiedKey === `run_${run.run_id}` ? (
                              <Check className="w-3 h-3 text-emerald-400" />
                            ) : (
                              <Copy className="w-3 h-3" />
                            )}
                          </button>
                        </div>
                      </td>

                      <td className="py-3 px-4">
                        <div className="flex items-center gap-1.5">
                          <span className="px-2 py-0.5 rounded bg-slate-800 border border-slate-700 text-slate-300 font-semibold">
                            {run.guild}
                          </span>
                          <span className="text-slate-500">/</span>
                          <span className="text-slate-400">{run.tool}</span>
                        </div>
                      </td>

                      <td className="py-3 px-4">
                        <div className="flex items-center gap-1 text-slate-300">
                          <User className="w-3.5 h-3.5 text-slate-500" />
                          <span>{run.requested_by || 'system'}</span>
                        </div>
                      </td>

                      <td className="py-3 px-4 text-slate-300">
                        <span className="px-2 py-0.5 rounded bg-blue-500/10 text-blue-400 border border-blue-500/20 font-mono">
                          {run.files_count} {run.files_count === 1 ? 'file' : 'files'}
                        </span>
                      </td>

                      <td className="py-3 px-4 text-slate-400">
                        {formatBytes(run.total_bytes)}
                      </td>

                      <td className="py-3 px-4">
                        <span
                          className={cn(
                            "inline-flex items-center gap-1 px-2 py-0.5 rounded text-[11px] font-mono border",
                            isOk
                              ? "bg-emerald-500/10 text-emerald-400 border-emerald-500/20"
                              : "bg-amber-500/10 text-amber-400 border-amber-500/20"
                          )}
                        >
                          {isOk ? <CheckCircle2 className="w-3 h-3" /> : <AlertTriangle className="w-3 h-3" />}
                          {run.delivery_status}
                        </span>
                      </td>

                      <td className="py-3 px-4 text-slate-400" title={run.created_at ? new Date(run.created_at * 1000).toLocaleString() : ''}>
                        <div className="flex items-center gap-1">
                          <Clock className="w-3 h-3 text-slate-500" />
                          <span>{formatRelativeTime(run.created_at)}</span>
                        </div>
                      </td>

                      <td className="py-3 px-4 text-right">
                        <button
                          onClick={() => fetchManifest(run.run_id)}
                          className="inline-flex items-center gap-1 px-2.5 py-1 rounded bg-slate-800 hover:bg-slate-700 text-emerald-400 border border-slate-700 hover:border-emerald-500/40 transition-all text-xs font-mono"
                        >
                          <Eye className="w-3.5 h-3.5" />
                          <span>Inspect</span>
                        </button>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* Manifest Modal / Drawer */}
      {selectedRunId && (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/70 backdrop-blur-sm">
          <div
            className="bg-slate-900 border border-slate-800 rounded-2xl w-full max-w-3xl max-h-[85vh] flex flex-col shadow-2xl overflow-hidden animate-in fade-in zoom-in-95 duration-150"
            onClick={(e) => e.stopPropagation()}
          >
            {/* Modal Header */}
            <div className="flex items-center justify-between p-4 border-b border-slate-800 bg-slate-900/90">
              <div className="flex items-center gap-3">
                <div className="p-2 rounded-lg bg-emerald-500/10 border border-emerald-500/20 text-emerald-400">
                  <FileCode className="w-5 h-5" />
                </div>
                <div>
                  <h3 className="text-sm font-bold text-slate-100 flex items-center gap-2">
                    Manifest Inspector
                    {selectedManifest && (
                      <span
                        className={cn(
                          "text-[10px] font-mono px-2 py-0.5 rounded-full border",
                          selectedManifest.delivery_status === 'ok'
                            ? "bg-emerald-500/10 text-emerald-400 border-emerald-500/20"
                            : "bg-amber-500/10 text-amber-400 border-amber-500/20"
                        )}
                      >
                        {selectedManifest.delivery_status}
                      </span>
                    )}
                  </h3>
                  <p className="text-xs text-slate-400 font-mono mt-0.5 flex items-center gap-1.5">
                    <span>Run:</span>
                    <span className="text-emerald-400">{selectedRunId}</span>
                    <button
                      onClick={() => copyToClipboard(selectedRunId, 'modal_run_id')}
                      className="text-slate-500 hover:text-slate-300"
                      title="Copy Run ID"
                    >
                      {copiedKey === 'modal_run_id' ? <Check className="w-3 h-3 text-emerald-400" /> : <Copy className="w-3 h-3" />}
                    </button>
                  </p>
                </div>
              </div>

              <div className="flex items-center gap-2">
                {/* View toggle */}
                <div className="flex bg-slate-950 border border-slate-800 rounded-lg p-0.5 text-xs font-mono">
                  <button
                    onClick={() => setManifestViewMode('files')}
                    className={cn(
                      "px-2.5 py-1 rounded transition-colors",
                      manifestViewMode === 'files'
                        ? "bg-emerald-500/20 text-emerald-400 font-semibold"
                        : "text-slate-400 hover:text-slate-200"
                    )}
                  >
                    Files ({selectedManifest?.files?.length || 0})
                  </button>
                  <button
                    onClick={() => setManifestViewMode('json')}
                    className={cn(
                      "px-2.5 py-1 rounded transition-colors",
                      manifestViewMode === 'json'
                        ? "bg-emerald-500/20 text-emerald-400 font-semibold"
                        : "text-slate-400 hover:text-slate-200"
                    )}
                  >
                    Raw JSON
                  </button>
                </div>

                <button
                  onClick={() => {
                    setSelectedRunId(null);
                    setSelectedManifest(null);
                  }}
                  className="p-1.5 text-slate-400 hover:text-white rounded-lg hover:bg-slate-800 transition-colors"
                >
                  <X className="w-5 h-5" />
                </button>
              </div>
            </div>

            {/* Modal Body */}
            <div className="flex-1 overflow-y-auto p-5 space-y-4">
              {manifestLoading ? (
                <div className="py-16 text-center text-slate-400 text-xs font-mono space-y-2">
                  <RefreshCw className="w-6 h-6 animate-spin text-emerald-400 mx-auto" />
                  <p>Loading manifest from disk index...</p>
                </div>
              ) : manifestError ? (
                <div className="p-4 rounded-xl bg-red-500/10 border border-red-500/30 text-red-400 text-xs font-mono flex items-center gap-2">
                  <AlertTriangle className="w-4 h-4 flex-shrink-0" />
                  <span>{manifestError}</span>
                </div>
              ) : selectedManifest ? (
                <>
                  {/* Meta Overview Cards */}
                  <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
                    <div className="bg-slate-950/60 p-3 rounded-lg border border-slate-800">
                      <span className="text-[10px] font-mono uppercase text-slate-500">Guild & Tool</span>
                      <p className="text-xs font-mono text-slate-200 font-semibold mt-0.5">
                        {selectedManifest.guild} / {selectedManifest.tool}
                      </p>
                    </div>

                    <div className="bg-slate-950/60 p-3 rounded-lg border border-slate-800">
                      <span className="text-[10px] font-mono uppercase text-slate-500">Requested By</span>
                      <p className="text-xs font-mono text-slate-200 font-semibold mt-0.5">
                        {selectedManifest.requested_by || 'system'}
                      </p>
                    </div>

                    <div className="bg-slate-950/60 p-3 rounded-lg border border-slate-800">
                      <span className="text-[10px] font-mono uppercase text-slate-500">Created At</span>
                      <p className="text-xs font-mono text-slate-200 mt-0.5">
                        {selectedManifest.created_at ? new Date(selectedManifest.created_at * 1000).toLocaleString() : 'N/A'}
                      </p>
                    </div>

                    <div className="bg-slate-950/60 p-3 rounded-lg border border-slate-800">
                      <span className="text-[10px] font-mono uppercase text-slate-500">Schema Version</span>
                      <p className="text-xs font-mono text-emerald-400 font-semibold mt-0.5">
                        v{selectedManifest.schema_version}
                      </p>
                    </div>
                  </div>

                  {manifestViewMode === 'files' ? (
                    <div className="space-y-3">
                      <h4 className="text-xs font-mono uppercase text-slate-400 font-semibold">
                        Indexed Artifacts ({selectedManifest.files.length})
                      </h4>

                      {selectedManifest.files.length === 0 ? (
                        <div className="p-8 text-center bg-slate-950/40 rounded-xl border border-slate-800 text-slate-500 text-xs font-mono">
                          No artifact files indexed in this run.
                        </div>
                      ) : (
                        <div className="space-y-2">
                          {selectedManifest.files.map((file, idx) => (
                            <div
                              key={idx}
                              className="bg-slate-950/70 border border-slate-800 rounded-xl p-3.5 space-y-2 hover:border-slate-700 transition-colors"
                            >
                              <div className="flex items-center justify-between">
                                <div className="flex items-center gap-2">
                                  <FileBox className="w-4 h-4 text-emerald-400 flex-shrink-0" />
                                  <span className="font-mono text-xs font-bold text-slate-200 break-all">
                                    {file.path}
                                  </span>
                                </div>
                                <span className="font-mono text-xs text-slate-400 bg-slate-900 px-2 py-0.5 rounded border border-slate-800">
                                  {formatBytes(file.bytes)}
                                </span>
                              </div>

                              {/* SHA256 */}
                              <div className="flex items-center gap-2 bg-slate-900/80 p-2 rounded-lg border border-slate-800/80 text-[11px] font-mono text-slate-400">
                                <Hash className="w-3.5 h-3.5 text-slate-500 flex-shrink-0" />
                                <span className="text-slate-500">sha256:</span>
                                <span className="text-slate-300 select-all break-all">{file.sha256}</span>
                                <button
                                  onClick={() => copyToClipboard(file.sha256, `hash_${idx}`)}
                                  className="ml-auto text-slate-500 hover:text-slate-300 p-1"
                                  title="Copy SHA-256 hash"
                                >
                                  {copiedKey === `hash_${idx}` ? (
                                    <Check className="w-3.5 h-3.5 text-emerald-400" />
                                  ) : (
                                    <Copy className="w-3.5 h-3.5" />
                                  )}
                                </button>
                              </div>
                            </div>
                          ))}
                        </div>
                      )}
                    </div>
                  ) : (
                    <div className="space-y-2">
                      <div className="flex items-center justify-between">
                        <span className="text-xs font-mono uppercase text-slate-400 font-semibold">
                          Raw manifest.json
                        </span>
                        <button
                          onClick={() => copyToClipboard(JSON.stringify(selectedManifest, null, 2), 'raw_json')}
                          className="flex items-center gap-1 text-xs font-mono text-emerald-400 hover:text-emerald-300 px-2 py-1 rounded bg-slate-950 border border-slate-800"
                        >
                          {copiedKey === 'raw_json' ? <Check className="w-3.5 h-3.5" /> : <Copy className="w-3.5 h-3.5" />}
                          <span>Copy JSON</span>
                        </button>
                      </div>

                      <pre className="p-4 bg-slate-950 rounded-xl border border-slate-800 text-xs font-mono text-slate-300 overflow-x-auto max-h-96">
                        {JSON.stringify(selectedManifest, null, 2)}
                      </pre>
                    </div>
                  )}
                </>
              ) : null}
            </div>

            {/* Modal Footer */}
            <div className="p-4 border-t border-slate-800 bg-slate-900/90 flex justify-end">
              <button
                onClick={() => {
                  setSelectedRunId(null);
                  setSelectedManifest(null);
                }}
                className="px-4 py-2 bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-mono rounded-lg transition-colors"
              >
                Close
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export default OutputsLedgerPanel;
