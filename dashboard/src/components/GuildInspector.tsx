import React, { useState, useEffect } from 'react';
import { 
  Play, 
  Terminal, 
  Cpu, 
  Activity, 
  Layers, 
  Plus,
  Trash2,
  CheckCircle2,
  XCircle,
  Clock,
  Search,
  ShieldAlert,
  Eye
} from 'lucide-react';
import type { Guild, NexusBridge } from '../lib/nexus-bridge';
import { cn } from '../lib/utils';

const GUILD_ARG_DEFAULTS: Record<string, { required: string[]; optional: string[]; placeholder: Record<string, string> }> = {
  bash: {
    required: ['command'],
    optional: [],
    placeholder: { command: 'cargo test' }
  },
  filesystem: {
    required: ['path'],
    optional: ['content', 'query'],
    placeholder: { path: 'crates/tylluan-kernel/src/main.rs', content: '', query: 'main' }
  },
  git: {
    required: ['command'],
    optional: ['message'],
    placeholder: { command: 'status', message: 'feat: add new feature' }
  },
  search: {
    required: ['query'],
    optional: [],
    placeholder: { query: 'Rust async execution best practices' }
  },
  memory: {
    required: ['query'],
    optional: ['content', 'remember'],
    placeholder: { query: 'recall last session state', content: '', remember: 'false' }
  },
  codebase_memory: {
    required: ['intent'],
    optional: ['project_path'],
    placeholder: { intent: 'get architecture', project_path: 'E:/myproject' }
  },
  web_research: {
    required: ['query'],
    optional: ['max_results'],
    placeholder: { query: 'Model Context Protocol specification', max_results: '3' }
  }
};

interface GuildInspectorProps {
  bridge: NexusBridge | null;
  notify: (msg: string, type?: 'info' | 'error') => void;
  guilds: Guild[];
}

export function GuildInspector({ bridge, notify, guilds }: GuildInspectorProps) {
  const [selectedGuild, setSelectedGuild] = useState<Guild | null>(guilds[0] || null);
  const [searchFilter, setSearchFilter] = useState('');
  const [isPlanMode, setIsPlanMode] = useState(true); // Default to dry-run pre-flight
  const [args, setArgs] = useState<Record<string, string>>({});
  const [customArgs, setCustomArgs] = useState<Array<{ key: string; val: string }>>([]);
  const [loading, setLoading] = useState(false);
  const [response, setResponse] = useState<any | null>(null);
  const [latencyMs, setLatencyMs] = useState<number | null>(null);

  const filteredGuilds = guilds.filter(g => 
    g.name.toLowerCase().includes(searchFilter.toLowerCase()) ||
    (g.description && g.description.toLowerCase().includes(searchFilter.toLowerCase()))
  );

  useEffect(() => {
    if (guilds.length > 0 && !selectedGuild) {
      setSelectedGuild(guilds[0]);
    }
  }, [guilds, selectedGuild]);

  useEffect(() => {
    if (!selectedGuild) return;
    setResponse(null);
    setLatencyMs(null);
    setCustomArgs([]);

    const defaults = (GUILD_ARG_DEFAULTS[selectedGuild.name] || ((selectedGuild as any).required_args ? {
      required: (selectedGuild as any).required_args || [],
      optional: (selectedGuild as any).optional_args || [],
      placeholder: {} as Record<string, string>
    } : {
      required: ['intent'],
      optional: [],
      placeholder: { intent: '' } as Record<string, string>
    })) as { required: string[]; optional: string[]; placeholder: Record<string, string> };

    const initialArgs: Record<string, string> = {};
    defaults.required.forEach((arg: string) => {
      initialArgs[arg] = defaults.placeholder[arg] || '';
    });
    defaults.optional.forEach((arg: string) => {
      initialArgs[arg] = defaults.placeholder[arg] || '';
    });

    if (Object.keys(initialArgs).length === 0) {
      initialArgs['intent'] = '';
    }

    setArgs(initialArgs);
  }, [selectedGuild]);

  if (!selectedGuild) {
    return (
      <div className="flex flex-col items-center justify-center py-12 text-slate-500 font-mono">
        <Cpu className="w-12 h-12 mb-4 opacity-20 text-[#00F5D4]" />
        <p className="text-sm">No guilds available for inspection</p>
      </div>
    );
  }

  const handleArgChange = (key: string, value: string) => {
    setArgs(prev => ({ ...prev, [key]: value }));
  };

  const addCustomArg = () => {
    setCustomArgs(prev => [...prev, { key: '', val: '' }]);
  };

  const removeCustomArg = (index: number) => {
    setCustomArgs(prev => prev.filter((_, i) => i !== index));
  };

  const handleCustomArgKeyChange = (index: number, key: string) => {
    setCustomArgs(prev => prev.map((item, i) => i === index ? { ...item, key } : item));
  };

  const handleCustomArgValChange = (index: number, val: string) => {
    setCustomArgs(prev => prev.map((item, i) => i === index ? { ...item, val } : item));
  };

  const handleTry = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!bridge) return;

    setLoading(true);
    setResponse(null);
    setLatencyMs(null);

    const mergedArgs: Record<string, any> = { ...args };
    customArgs.forEach(item => {
      if (item.key.trim()) {
        mergedArgs[item.key.trim()] = item.val;
      }
    });

    const payload: Record<string, any> = {
      tool: 'tylluan_do',
      arguments: {
        guild: selectedGuild.name,
        plan: isPlanMode,
        ...mergedArgs
      }
    };

    const startTime = performance.now();
    try {
      const res = await bridge.fetchRaw('/api/v1/do', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload)
      });
      const duration = Math.round(performance.now() - startTime);
      setLatencyMs(duration);
      setResponse(res);
      notify(`Execution completed in ${duration}ms`, 'info');
    } catch (err: any) {
      const duration = Math.round(performance.now() - startTime);
      setLatencyMs(duration);
      setResponse({ error: err.message || 'Execution failed' });
      notify(`Execution error: ${err.message}`, 'error');
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="space-y-6 font-sans">
      {/* Header */}
      <div className="p-5 bg-[#0B0F17]/90 border border-slate-800/80 rounded-2xl flex flex-col md:flex-row items-start md:items-center justify-between gap-4 font-mono">
        <div>
          <div className="flex items-center gap-2">
            <span className="px-2 py-0.5 text-[9px] font-bold tracking-wider uppercase bg-[#00F5D4]/10 text-[#00F5D4] border border-[#00F5D4]/30 rounded">
              Interactive Workbench
            </span>
            <span className="px-2 py-0.5 text-[9px] font-bold tracking-wider uppercase bg-emerald-500/10 text-emerald-400 border border-emerald-500/30 rounded">
              FastMCP Tools
            </span>
          </div>
          <h2 className="text-xl font-bold tracking-tight text-slate-100 mt-2 flex items-center gap-2">
            <Layers className="w-5 h-5 text-[#00F5D4]" />
            42-Guild Inspector &amp; FastMCP Tool Tester
          </h2>
          <p className="text-xs text-slate-400 mt-0.5">
            Test and inspect parameters of registered guilds and tools directly on the kernel.
          </p>
        </div>

        {/* Plan Mode Toggle */}
        <div className="flex items-center gap-2 px-3 py-1.5 bg-slate-950 border border-slate-850 rounded-xl">
          <span className="text-xs text-slate-400 font-bold">Execution Mode:</span>
          <button
            onClick={() => setIsPlanMode(!isPlanMode)}
            className={cn(
              "flex items-center gap-1.5 px-3 py-1 text-xs font-bold rounded-lg border transition-all",
              isPlanMode 
                ? "bg-[#00F5D4]/10 border-[#00F5D4]/40 text-[#00F5D4]" 
                : "bg-[#FF2E93]/10 border-[#FF2E93]/40 text-[#FF2E93]"
            )}
          >
            {isPlanMode ? <Eye className="w-3.5 h-3.5" /> : <Play className="w-3.5 h-3.5" />}
            <span>{isPlanMode ? 'Pre-Flight Dry-Run' : 'Real Execution'}</span>
          </button>
        </div>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-3 gap-6 font-mono">
        {/* Guild Selection Column */}
        <div className="bg-[#0B0F17]/90 rounded-2xl border border-slate-800/80 p-4 space-y-3">
          <div className="flex items-center justify-between">
            <h3 className="text-xs font-bold text-slate-400 uppercase tracking-widest flex items-center gap-2">
              <Cpu className="w-4 h-4 text-[#00F5D4]" /> Guilds ({guilds.length})
            </h3>
          </div>

          <div className="relative">
            <Search className="w-3.5 h-3.5 text-slate-500 absolute left-3 top-2.5" />
            <input
              type="text"
              value={searchFilter}
              onChange={(e) => setSearchFilter(e.target.value)}
              placeholder="Search 42 guilds..."
              className="w-full pl-9 pr-3 py-1.5 bg-slate-950 border border-slate-850 focus:border-[#00F5D4]/50 text-slate-100 text-xs rounded-xl outline-none"
            />
          </div>

          <div className="max-h-[420px] overflow-y-auto space-y-1 pr-1">
            {filteredGuilds.map((g) => (
              <button
                key={g.name}
                onClick={() => setSelectedGuild(g)}
                className={cn(
                  "w-full text-left p-2.5 rounded-xl border transition-all text-xs font-bold flex items-center justify-between",
                  selectedGuild.name === g.name
                    ? "bg-[#00F5D4]/10 border-[#00F5D4]/40 text-[#00F5D4]"
                    : "bg-slate-950/60 border-slate-850/80 text-slate-300 hover:border-slate-700"
                )}
              >
                <span>{g.name}</span>
                <span className={cn("w-2 h-2 rounded-full", g.running ? "bg-emerald-400" : "bg-slate-600")} />
              </button>
            ))}
          </div>
        </div>

        {/* Argument Form & Execution Output Column */}
        <div className="md:col-span-2 space-y-6">
          <div className="bg-[#0B0F17]/90 rounded-2xl border border-slate-800/80 p-5 space-y-4">
            <div className="flex items-center justify-between pb-3 border-b border-slate-800/80">
              <div>
                <h3 className="text-sm font-bold text-slate-100 flex items-center gap-2">
                  <Terminal className="w-4 h-4 text-[#00F5D4]" />
                  Guild: <span className="text-[#00F5D4]">{selectedGuild.name}</span>
                </h3>
                <p className="text-xs text-slate-400 mt-0.5">{selectedGuild.description || 'No description available'}</p>
              </div>
              <span className={cn(
                "px-2 py-1 text-[10px] font-bold rounded uppercase border",
                selectedGuild.running ? "bg-emerald-500/10 text-emerald-400 border-emerald-500/30" : "bg-slate-800 text-slate-400 border-slate-700"
              )}>
                {selectedGuild.running ? 'Running' : 'Stopped'}
              </span>
            </div>

            <form onSubmit={handleTry} className="space-y-4">
              <div className="space-y-3">
                <label className="text-xs font-bold text-slate-400 uppercase tracking-widest">Calculated Arguments:</label>
                {Object.keys(args).map((key) => (
                  <div key={key} className="space-y-1">
                    <label className="text-xs text-slate-300 font-bold">{key}:</label>
                    <input
                      type="text"
                      value={args[key] || ''}
                      onChange={(e) => handleArgChange(key, e.target.value)}
                      className="w-full px-3 py-2 bg-slate-950 border border-slate-850 focus:border-[#00F5D4]/50 text-slate-100 text-xs rounded-xl outline-none"
                    />
                  </div>
                ))}

                {customArgs.map((item, idx) => (
                  <div key={idx} className="flex gap-2 items-center">
                    <input
                      type="text"
                      value={item.key}
                      onChange={(e) => handleCustomArgKeyChange(idx, e.target.value)}
                      placeholder="key"
                      className="w-1/3 px-3 py-2 bg-slate-950 border border-slate-850 text-slate-100 text-xs rounded-xl outline-none"
                    />
                    <input
                      type="text"
                      value={item.val}
                      onChange={(e) => handleCustomArgValChange(idx, e.target.value)}
                      placeholder="value"
                      className="flex-1 px-3 py-2 bg-slate-950 border border-slate-850 text-slate-100 text-xs rounded-xl outline-none"
                    />
                    <button
                      type="button"
                      onClick={() => removeCustomArg(idx)}
                      className="p-2 text-red-400 hover:text-red-300"
                    >
                      <Trash2 className="w-4 h-4" />
                    </button>
                  </div>
                ))}

                <button
                  type="button"
                  onClick={addCustomArg}
                  className="flex items-center gap-1.5 text-xs text-[#00F5D4] hover:underline pt-1"
                >
                  <Plus className="w-3.5 h-3.5" /> Add custom parameter
                </button>
              </div>

              <div className="flex justify-end pt-2">
                <button
                  type="submit"
                  disabled={loading}
                  className="flex items-center gap-2 px-5 py-2.5 bg-[#00F5D4]/10 hover:bg-[#00F5D4]/20 border border-[#00F5D4]/40 text-[#00F5D4] text-xs font-bold rounded-xl transition-all disabled:opacity-40"
                >
                  {isPlanMode ? <Eye className="w-4 h-4" /> : <Play className="w-4 h-4" />}
                  <span>{loading ? 'Executing...' : (isPlanMode ? 'Run Pre-Flight Inspection' : 'Execute Guild Action')}</span>
                </button>
              </div>
            </form>
          </div>

          {/* Response Output */}
          {response && (
            <div className="bg-[#0B0F17]/90 rounded-2xl border border-slate-800/80 p-5 space-y-3 font-mono">
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2 text-xs font-bold text-[#00F5D4]">
                  {response.error ? <XCircle className="w-4 h-4 text-red-400" /> : <CheckCircle2 className="w-4 h-4 text-emerald-400" />}
                  <span>Execution Output</span>
                </div>
                {latencyMs !== null && (
                  <span className="text-[10px] text-slate-400 flex items-center gap-1">
                    <Clock className="w-3 h-3 text-[#00F5D4]" /> {latencyMs}ms
                  </span>
                )}
              </div>

              <pre className="p-3.5 bg-slate-950 border border-slate-850 text-emerald-400 rounded-xl text-xs overflow-x-auto">
                {JSON.stringify(response, null, 2)}
              </pre>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
