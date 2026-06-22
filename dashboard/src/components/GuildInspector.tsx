import React, { useState, useEffect } from 'react';
import { 
  Play, 
  Terminal, 
  Cpu, 
  Settings, 
  HelpCircle, 
  Activity, 
  Layers, 
  Send,
  Plus,
  Trash2,
  CheckCircle2,
  XCircle,
  Clock
} from 'lucide-react';
import type { Guild, NexusBridge } from '../lib/nexus-bridge';

// Static fallbacks for arguments of common guilds
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
    placeholder: { intent: 'get architecture', project_path: 'E:/TylluanMCPo3' }
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
  const [args, setArgs] = useState<Record<string, string>>({});
  const [customArgs, setCustomArgs] = useState<Array<{ key: string; val: string }>>([]);
  const [loading, setLoading] = useState(false);
  const [response, setResponse] = useState<any | null>(null);

  // Sync selected guild if list changes
  useEffect(() => {
    if (guilds.length > 0 && !selectedGuild) {
      setSelectedGuild(guilds[0]);
    }
  }, [guilds, selectedGuild]);

  // Load defaults when selected guild changes
  useEffect(() => {
    if (!selectedGuild) return;
    
    // Clear responses
    setResponse(null);
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

    // Make sure intent is present if not already
    if (Object.keys(initialArgs).length === 0) {
      initialArgs['intent'] = '';
    }

    setArgs(initialArgs);
  }, [selectedGuild]);

  if (!selectedGuild) {
    return (
      <div className="flex flex-col items-center justify-center py-12 text-slate-500">
        <Cpu className="w-12 h-12 mb-4 opacity-20" />
        <p className="text-sm font-mono">No guilds available for inspection</p>
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

    // Build payload
    const payload: Record<string, any> = {
      tool: 'tylluan_do',
      guild: selectedGuild.name
    };

    // Add standard args
    Object.entries(args).forEach(([k, v]) => {
      if (v.trim() !== '') {
        // Parse booleans or numbers if applicable
        if (v.toLowerCase() === 'true') payload[k] = true;
        else if (v.toLowerCase() === 'false') payload[k] = false;
        else if (!isNaN(Number(v)) && v.trim() !== '') payload[k] = Number(v);
        else payload[k] = v;
      }
    });

    // Add custom key-value pairs
    customArgs.forEach(arg => {
      if (arg.key.trim() !== '') {
        const k = arg.key.trim();
        const v = arg.val;
        if (v.toLowerCase() === 'true') payload[k] = true;
        else if (v.toLowerCase() === 'false') payload[k] = false;
        else if (!isNaN(Number(v)) && v.trim() !== '') payload[k] = Number(v);
        else payload[k] = v;
      }
    });

    // Ensure we have an intent field. If not present, default to the main argument
    if (!payload.intent) {
      payload.intent = payload.command || payload.query || payload.path || 'execute action';
    }

    try {
      notify(`Testing guild ${selectedGuild.name}...`, 'info');
      const result = await bridge.fetchRaw('/api/v1/do', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload)
      });
      setResponse(result);
      notify(`Execution completed for ${selectedGuild.name}`, 'info');
    } catch (err: any) {
      setResponse({ error: err.message || 'Unknown network error' });
      notify(`Execution failed: ${err.message || 'Error'}`, 'error');
    } finally {
      setLoading(false);
    }
  };

  const defaults = GUILD_ARG_DEFAULTS[selectedGuild.name] || (selectedGuild as any).required_args ? {
    required: (selectedGuild as any).required_args || [],
    optional: (selectedGuild as any).optional_args || []
  } : {
    required: ['intent'],
    optional: []
  };

  return (
    <div className="grid grid-cols-1 lg:grid-cols-12 gap-6 bg-slate-900/20 p-6 rounded-xl border border-slate-800">
      
      {/* Sidebar - Guild Selector */}
      <div className="lg:col-span-4 space-y-3">
        <h4 className="text-xs font-bold text-slate-400 uppercase tracking-wider font-mono">Select Guild</h4>
        <div className="max-h-[500px] overflow-y-auto pr-2 space-y-1.5 scrollbar-thin">
          {guilds.map(g => {
            const isSelected = selectedGuild.name === g.name;
            return (
              <button
                key={g.name}
                type="button"
                onClick={() => setSelectedGuild(g)}
                className={`w-full text-left p-3 rounded-lg border transition-all flex items-center justify-between ${
                  isSelected 
                    ? 'bg-blue-500/10 border-blue-500/30 text-blue-300' 
                    : 'bg-slate-900/40 border-slate-800/80 text-slate-400 hover:border-slate-700 hover:text-slate-200'
                }`}
              >
                <div className="flex items-center gap-2 min-w-0">
                  <Terminal className={`w-3.5 h-3.5 ${isSelected ? 'text-blue-400' : 'text-slate-500'}`} />
                  <span className="font-mono text-xs font-semibold truncate">{g.name}</span>
                </div>
                <div className="flex items-center gap-1.5 flex-shrink-0">
                  <span className={`w-1.5 h-1.5 rounded-full ${g.running ? 'bg-emerald-500' : 'bg-slate-600'}`} />
                  <span className="text-[9px] font-mono tracking-tighter uppercase font-semibold">
                    {g.running ? 'online' : 'lazy'}
                  </span>
                </div>
              </button>
            );
          })}
        </div>
      </div>

      {/* Main Form + Response Panel */}
      <div className="lg:col-span-8 space-y-6">
        
        {/* Guild Details Header */}
        <div className="bg-slate-900/60 p-4 rounded-xl border border-slate-800 flex justify-between items-start gap-4 flex-wrap">
          <div className="space-y-1">
            <div className="flex items-center gap-2">
              <h3 className="text-base font-bold font-mono text-slate-100">{selectedGuild.name}</h3>
              <span className={`px-2 py-0.5 rounded text-[9px] font-mono font-bold tracking-widest uppercase border ${
                selectedGuild.running 
                  ? 'bg-emerald-500/15 text-emerald-400 border-emerald-500/20' 
                  : 'bg-slate-800 text-slate-500 border-slate-700'
              }`}>
                {selectedGuild.running ? 'Running' : 'Offline'}
              </span>
            </div>
            <p className="text-xs text-slate-400">{(selectedGuild.launcher_type as string) === 'external' ? 'External process executor' : 'Python drop-in system tool'}</p>
          </div>
          
          <div className="flex gap-4 font-mono text-[10px]">
            <div className="bg-slate-950/60 px-3 py-1.5 rounded border border-slate-800">
              <span className="text-slate-500 block uppercase text-[8px] tracking-wider mb-0.5">Latency</span>
              <span className="text-slate-300 font-semibold">{selectedGuild.last_latency_ms ? `${selectedGuild.last_latency_ms}ms` : 'N/A'}</span>
            </div>
            <div className="bg-slate-950/60 px-3 py-1.5 rounded border border-slate-800">
              <span className="text-slate-500 block uppercase text-[8px] tracking-wider mb-0.5">Total Calls</span>
              <span className="text-slate-300 font-semibold">{selectedGuild.total_calls || 0}</span>
            </div>
            <div className="bg-slate-950/60 px-3 py-1.5 rounded border border-slate-800">
              <span className="text-slate-500 block uppercase text-[8px] tracking-wider mb-0.5">Tools</span>
              <span className="text-slate-300 font-semibold">{selectedGuild.tools_count || 0} API</span>
            </div>
          </div>
        </div>

        {/* Input Parameters Form */}
        <form onSubmit={handleTry} className="bg-slate-950/40 p-5 rounded-xl border border-slate-800/80 space-y-4">
          <div className="flex justify-between items-center border-b border-slate-800/60 pb-2">
            <h4 className="text-xs font-mono font-bold text-slate-300 uppercase tracking-wider flex items-center gap-1.5">
              <Settings className="w-3.5 h-3.5 text-blue-400" />
              Guild Input Parameters
            </h4>
            <span className="text-[10px] text-slate-500 font-mono">Required arguments are flagged with *</span>
          </div>

          <div className="space-y-3">
            {/* Standard inputs derived from defaults */}
            {defaults.required.map((arg: string) => (
              <div key={arg} className="space-y-1">
                <label className="flex items-center gap-1 text-[11px] font-mono font-semibold text-slate-300">
                  <span>{arg}</span>
                  <span className="text-red-400 font-bold">*</span>
                </label>
                <input
                  type="text"
                  required
                  value={args[arg] || ''}
                  onChange={(e) => handleArgChange(arg, e.target.value)}
                  placeholder={`e.g. ${GUILD_ARG_DEFAULTS[selectedGuild.name]?.placeholder[arg] || 'value'}`}
                  className="w-full bg-slate-900 border border-slate-800 rounded px-3 py-1.5 text-xs text-slate-200 focus:outline-none focus:border-blue-500 font-mono"
                />
              </div>
            ))}

            {defaults.optional.map((arg: string) => (
              <div key={arg} className="space-y-1">
                <label className="text-[11px] font-mono font-semibold text-slate-400">{arg}</label>
                <input
                  type="text"
                  value={args[arg] || ''}
                  onChange={(e) => handleArgChange(arg, e.target.value)}
                  placeholder={`e.g. ${GUILD_ARG_DEFAULTS[selectedGuild.name]?.placeholder[arg] || 'optional'}`}
                  className="w-full bg-slate-900 border border-slate-800 rounded px-3 py-1.5 text-xs text-slate-300 focus:outline-none focus:border-slate-700 font-mono"
                />
              </div>
            ))}

            {/* Custom Arguments */}
            {customArgs.length > 0 && (
              <div className="space-y-2 pt-2 border-t border-slate-800/40">
                <span className="text-[10px] font-bold font-mono text-slate-500 uppercase tracking-widest block">Custom Arguments</span>
                {customArgs.map((arg, idx) => (
                  <div key={idx} className="flex gap-2 items-center">
                    <input
                      type="text"
                      placeholder="key"
                      value={arg.key}
                      onChange={(e) => handleCustomArgKeyChange(idx, e.target.value)}
                      className="w-1/3 bg-slate-900 border border-slate-800 rounded px-3 py-1.5 text-xs text-slate-300 focus:outline-none font-mono"
                    />
                    <input
                      type="text"
                      placeholder="value"
                      value={arg.val}
                      onChange={(e) => handleCustomArgValChange(idx, e.target.value)}
                      className="flex-1 bg-slate-900 border border-slate-800 rounded px-3 py-1.5 text-xs text-slate-300 focus:outline-none font-mono"
                    />
                    <button
                      type="button"
                      onClick={() => removeCustomArg(idx)}
                      className="p-1.5 text-slate-500 hover:text-red-400 transition-colors"
                    >
                      <Trash2 className="w-4 h-4" />
                    </button>
                  </div>
                ))}
              </div>
            )}
          </div>

          {/* Form Actions */}
          <div className="flex justify-between items-center pt-2">
            <button
              type="button"
              onClick={addCustomArg}
              className="flex items-center gap-1 text-[10px] font-mono text-slate-400 hover:text-slate-200 transition-colors px-2 py-1 border border-slate-800 rounded hover:border-slate-700"
            >
              <Plus className="w-3.5 h-3.5" /> Add Param
            </button>
            <button
              type="submit"
              disabled={loading}
              className="flex items-center gap-1.5 px-4 py-2 bg-blue-600 hover:bg-blue-500 text-slate-100 rounded text-xs font-bold transition-all disabled:opacity-50 cursor-pointer shadow-lg shadow-blue-500/10 font-mono uppercase tracking-wider"
            >
              {loading ? 'Running...' : 'Execute Try'}
              <Send className="w-3.5 h-3.5" />
            </button>
          </div>
        </form>

        {/* Execution Response */}
        {response && (
          <div className="space-y-2 bg-slate-950 p-4 rounded-xl border border-slate-800/80">
            <div className="flex items-center justify-between border-b border-slate-800/60 pb-2">
              <span className="text-xs font-mono font-bold text-slate-400 uppercase tracking-wider flex items-center gap-1.5">
                <Clock className="w-3.5 h-3.5 text-blue-400" />
                Execution Result
              </span>
              <div className="flex items-center gap-1">
                {response.error ? (
                  <>
                    <XCircle className="w-3.5 h-3.5 text-red-500" />
                    <span className="text-[10px] text-red-400 font-mono uppercase font-black">Failed</span>
                  </>
                ) : (
                  <>
                    <CheckCircle2 className="w-3.5 h-3.5 text-emerald-500" />
                    <span className="text-[10px] text-emerald-400 font-mono uppercase font-black">Success</span>
                  </>
                )}
              </div>
            </div>
            <pre className="text-xs font-mono text-slate-300 overflow-x-auto p-3 bg-slate-900/60 border border-slate-900 rounded max-h-[250px] scrollbar-thin">
              {JSON.stringify(response, null, 2)}
            </pre>
          </div>
        )}

      </div>
    </div>
  );
}
