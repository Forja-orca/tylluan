import React, { useState, useEffect, useRef, useCallback } from 'react';
import {
  Cpu,
  ServerOff,
  CheckCircle2,
  XCircle,
  Loader2,
  Eye,
  HelpCircle,
  Clock,
  Play
} from 'lucide-react';
import { cn } from '../lib/utils';
import { BackgroundJob } from '../lib/nexus-bridge';
import { usePolling } from '../hooks/usePolling';

interface BackgroundJobsPanelProps {
  bridge: any;
  notify: (msg: string, type?: 'info' | 'error') => void;
}

// Real backend (crates/tylluan-kernel/src/transport/server/background_jobs.rs, M31-P6)
// has no "list all jobs" capability at all -- @bg:<intent> starts one job and returns
// its id, @job:<id> checks one specific job. There's nothing to page through
// server-side, so this panel tracks jobs it started/observed locally (this session
// only) rather than loading a list from a nonexistent endpoint.
export default function BackgroundJobsPanel({ bridge, notify }: BackgroundJobsPanelProps) {
  const [jobs, setJobs] = useState<BackgroundJob[]>([]);
  const [intent, setIntent] = useState('');
  const [starting, setStarting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [selectedJobId, setSelectedJobId] = useState<string | null>(null);
  const pollRef = useRef<Record<string, ReturnType<typeof setInterval>>>({});

  const loadJobs = async () => {
    if (!bridge) return;
    try {
      const res = await bridge.listBackgroundJobs();
      if (res && res.jobs) {
        setJobs(res.jobs.map((j: any) => ({
          id: j.id,
          guild: j.guild || j.name || 'kernel',
          intent: j.intent || j.description || j.name,
          status: j.status || 'running',
          started_at: j.created_at || new Date().toISOString(),
          elapsed_secs: j.elapsed_secs || 0,
        })));
      }
    } catch (err: any) {
      console.error("Error al cargar lista de trabajos en segundo plano:", err.message);
    }
  };

  usePolling('bg-jobs-list', loadJobs, { interval: 'standard', enabled: !!bridge });

  const handleStartJob = async () => {
    if (!intent.trim() || !bridge) return;
    setStarting(true);
    setError(null);
    try {
      const { jobId, guild } = await bridge.startBackgroundJob(intent.trim());
      const job: BackgroundJob = {
        id: jobId,
        guild,
        intent: intent.trim(),
        status: 'pending',
        started_at: new Date().toISOString(),
        elapsed_secs: 0,
      };
      setJobs(prev => [job, ...prev]);
      setIntent('');
      pollJob(jobId);
    } catch (err: any) {
      console.error("Error al iniciar trabajo en segundo plano:", err.message);
      setError(`Error al iniciar trabajo (@bg:${intent.trim()}): ${err.message}`);
    } finally {
      setStarting(false);
    }
  };

  // Poll a real job's status every 3s until it's completed/failed.
  const pollJob = useCallback((jobId: string) => {
    if (pollRef.current[jobId]) return;
    pollRef.current[jobId] = setInterval(async () => {
      try {
        const { status, text } = await bridge?.getJobStatus(jobId) ?? {};
        if (!status) return;
        setJobs(prev => prev.map(j => j.id === jobId ? { ...j, status, result_text: text } : j));
        if (status !== 'pending') {
          clearInterval(pollRef.current[jobId]);
          delete pollRef.current[jobId];
        }
      } catch {
        // transient fetch failure -- keep polling
      }
    }, 3000);
  }, [bridge]);

  // Elapsed-time ticker for pending jobs (via coordinator instead of raw setInterval).
  const tickElapsed = useCallback(() => {
    setJobs(prev => prev.map(j => j.status === 'pending'
      ? { ...j, elapsed_secs: Math.floor((Date.now() - new Date(j.started_at).getTime()) / 1000) }
      : j));
  }, []);
  usePolling('bg-jobs-elapsed', tickElapsed, { interval: 'fast', enabled: true });

  // Real-time updates via the guild_job_complete SSE event (params: job_id, guild,
  // status, summary, ts -- see emit_job_complete in background_jobs.rs).
  useEffect(() => {
    const handleJobComplete = (e: Event) => {
      const detail = (e as CustomEvent).detail;
      if (!detail?.job_id) return;
      notify(`Job ${detail.job_id} (${detail.guild}) finished: ${detail.status}`, detail.status === 'failed' ? 'error' : 'info');
      setJobs(prev => {
        const exists = prev.some(j => j.id === detail.job_id);
        if (exists) {
          return prev.map(j => j.id === detail.job_id
            ? { ...j, status: detail.status, result_text: detail.summary }
            : j);
        }
        // Job completed before we ever saw it queued (e.g. started from another client)
        return [{
          id: detail.job_id,
          guild: detail.guild || 'unknown',
          intent: '(started elsewhere)',
          status: detail.status,
          started_at: new Date(detail.ts || Date.now()).toISOString(),
          elapsed_secs: 0,
          result_text: detail.summary,
        }, ...prev];
      });
      if (pollRef.current[detail.job_id]) {
        clearInterval(pollRef.current[detail.job_id]);
        delete pollRef.current[detail.job_id];
      }
    };

    window.addEventListener('nexus_event_guild_job_complete', handleJobComplete);
    return () => window.removeEventListener('nexus_event_guild_job_complete', handleJobComplete);
  }, [notify]);

  useEffect(() => {
    return () => {
      Object.values(pollRef.current).forEach(clearInterval);
    };
  }, []);

  const getStatusIcon = (status: BackgroundJob['status']) => {
    switch (status) {
      case 'pending':
        return <Loader2 className="w-4 h-4 text-emerald-400 animate-spin" />;
      case 'completed':
        return <CheckCircle2 className="w-4 h-4 text-emerald-500" />;
      case 'failed':
        return <XCircle className="w-4 h-4 text-red-500" />;
      default:
        return <HelpCircle className="w-4 h-4 text-slate-500" />;
    }
  };

  const getStatusBadge = (status: BackgroundJob['status']) => {
    const base = "px-2 py-0.5 rounded-full text-[9px] font-bold uppercase tracking-wider border leading-none";
    switch (status) {
      case 'pending':
        return cn(base, "bg-emerald-500/10 text-emerald-400 border-emerald-500/20");
      case 'completed':
        return cn(base, "bg-emerald-500/10 text-emerald-500 border-emerald-500/20");
      case 'failed':
        return cn(base, "bg-red-500/10 text-red-400 border-red-500/20");
      default:
        return cn(base, "bg-slate-500/10 text-slate-500 border-slate-500/20");
    }
  };

  const selectedJob = jobs.find(j => j.id === selectedJobId) ?? null;

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex justify-between items-start">
        <div>
          <h2 className="text-xl font-bold tracking-tight text-slate-50 flex items-center gap-2">
            <Cpu className="w-5 h-5 text-emerald-500" />
            Background Jobs
          </h2>
          <p className="text-xs text-slate-400 mt-1">
            Run slow guild calls (deep_analysis, vision, knowledge) without blocking, via{' '}
            <code className="text-emerald-400 bg-emerald-400/10 px-1 py-0.5 rounded">@bg:&lt;intent&gt;</code>.
          </p>
        </div>
      </div>

      {/* Start Job Form */}
      <div className="flex items-center gap-2 p-4 bg-slate-900/60 border border-slate-850 rounded-2xl">
        <input
          type="text"
          value={intent}
          onChange={(e) => setIntent(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && handleStartJob()}
          placeholder="e.g. deep analysis of the codebase architecture"
          className="flex-1 px-3 py-2 bg-slate-950 border border-slate-800 focus:border-emerald-500 focus:outline-none rounded-xl text-xs font-mono text-slate-200 placeholder-slate-600"
        />
        <button
          onClick={handleStartJob}
          disabled={starting || !intent.trim()}
          className="px-4 py-2 bg-emerald-500 hover:bg-emerald-600 text-slate-950 font-bold font-mono text-xs rounded-xl flex items-center gap-1.5 transition-colors disabled:opacity-50"
        >
          <Play className="w-3.5 h-3.5" />
          Start
        </button>
      </div>

      {/* Main Grid Layout */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Jobs List (Left / Span 2) */}
        <div className="lg:col-span-2 space-y-4">
          <span className="text-xs font-bold uppercase tracking-wider font-mono text-slate-400 block">
            Tracked This Session
          </span>

          <div className="bg-slate-900/60 border border-slate-850 rounded-2xl overflow-hidden divide-y divide-slate-850">
            {jobs.length === 0 ? (
              <div className="p-12 text-center text-slate-500 font-mono text-xs">
                No background jobs started yet.
              </div>
            ) : (
              jobs.map(job => (
                <div
                  key={job.id}
                  onClick={() => setSelectedJobId(job.id)}
                  className={cn(
                    "p-4 flex flex-col md:flex-row md:items-center justify-between gap-4 transition-colors hover:bg-slate-800/20 cursor-pointer",
                    selectedJobId === job.id && "bg-slate-800/40 border-l-2 border-emerald-500"
                  )}
                >
                  <div className="flex items-start gap-3">
                    <div className="mt-1">{getStatusIcon(job.status)}</div>
                    <div>
                      <div className="flex items-center gap-2 flex-wrap">
                        <span className="text-sm font-mono font-bold text-slate-200">{job.id}</span>
                        <span className={getStatusBadge(job.status)}>{job.status}</span>
                      </div>
                      <div className="flex items-center gap-4 text-[10px] text-slate-500 font-mono mt-1 flex-wrap">
                        <span>Guild: <strong className="text-slate-350">{job.guild}</strong></span>
                        <span>Started: {new Date(job.started_at).toLocaleTimeString()}</span>
                        {job.status === 'pending' && <span>Elapsed: {job.elapsed_secs}s</span>}
                      </div>
                      <p className="text-[10px] font-mono text-slate-500 mt-1 truncate max-w-md">{job.intent}</p>
                    </div>
                  </div>

                  {job.status !== 'pending' && (
                    <button
                      onClick={(e) => { e.stopPropagation(); setSelectedJobId(job.id); }}
                      className="px-3 py-1.5 bg-slate-950 hover:bg-slate-900 border border-slate-800 hover:border-slate-700 text-slate-300 text-xs font-mono rounded-xl flex items-center gap-1.5 transition-all self-end md:self-auto"
                    >
                      <Eye className="w-3.5 h-3.5 text-emerald-400" />
                      View Result
                    </button>
                  )}
                </div>
              ))
            )}
          </div>
        </div>

        {/* Selected Job Result (Right / Span 1) */}
        <div className="lg:col-span-1 space-y-4">
          <span className="text-xs font-bold uppercase tracking-wider font-mono text-slate-400 block">Result Inspector</span>

          <div className="bg-slate-900/40 border border-slate-850 rounded-2xl p-6 min-h-[300px] flex flex-col justify-between">
            {selectedJob ? (
              <div className="space-y-4 flex-1 flex flex-col justify-between font-mono text-xs">
                <div>
                  <div className="flex items-center justify-between border-b border-slate-800 pb-3 mb-3">
                    <span className="text-xs font-bold text-slate-400">Result: {selectedJob.id}</span>
                    <span className={getStatusBadge(selectedJob.status)}>{selectedJob.status}</span>
                  </div>
                  <div className="p-3 bg-slate-950 border border-slate-850 rounded-xl text-[11px] text-slate-200 max-h-64 overflow-y-auto whitespace-pre-wrap leading-relaxed">
                    {selectedJob.result_text || (selectedJob.status === 'pending' ? 'Still running...' : '(no result text)')}
                  </div>
                </div>
                <button
                  onClick={() => setSelectedJobId(null)}
                  className="w-full mt-6 py-2 bg-slate-950 hover:bg-slate-900 border border-slate-800 hover:border-slate-700 text-slate-400 hover:text-slate-50 font-bold font-mono text-xs rounded-xl transition-all"
                >
                  Clear Selection
                </button>
              </div>
            ) : (
              <div className="flex flex-col items-center justify-center h-full py-12 flex-1 text-slate-500 font-mono text-xs gap-3">
                <HelpCircle className="w-12 h-12 text-slate-700 animate-pulse" />
                <div className="text-center">
                  <p>No job selected</p>
                  <p className="text-[10px] text-slate-600 mt-1">Select a job to inspect its result.</p>
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
