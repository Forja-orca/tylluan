import sys
import re

with open('App.tsx', 'r', encoding='utf-8') as f:
    content = f.read()

start_marker = '// ============ SANDBOX TAB ============'
end_marker = '// ============ AGENTS TAB ============'

start_idx = content.find(start_marker)
end_idx = content.find(end_marker, start_idx)

if start_idx != -1 and end_idx != -1:
    new_content = """// ============ SANDBOX TAB ============
function SandboxTab({ bridge, notify }: { bridge: NexusBridge | null, notify: (msg: string, type?: 'info' | 'error') => void }) {
  const { events } = useNexus();
  const [sessionStart, setSessionStart] = useState<number | null>(null);
  const [activityLog, setActivityLog] = useState<{ ts: number; guild: string; tool: string; status: 'started'|'finished'; ok?: boolean; intent?: string }[]>([]);
  const [workspaceNodes, setWorkspaceNodes] = useState<{ id: string; type: string; name: string; content: string; createdAt: string }[]>([]);
  const [sessionStopped, setSessionStopped] = useState(false);
  const [cmdInput, setCmdInput] = useState('');
  const [cmdOutput, setCmdOutput] = useState<string[]>([]);
  const [showTerminal, setShowTerminal] = useState(false);
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const startSession = () => {
    setSessionStart(Date.now());
    setActivityLog([]);
    setWorkspaceNodes([]);
    setSessionStopped(false);
  };

  const stopSession = () => {
    setSessionStopped(true);
    if (intervalRef.current) clearInterval(intervalRef.current);
  };

  const exportReport = () => {
    const report = `# Sandbox Mission Report\\n\\n## Activity Log\\n${activityLog.map(a => `- [${new Date(a.ts).toLocaleTimeString()}] ${a.guild}::${a.tool} (${a.status}${a.ok !== undefined ? ` ok:${a.ok}` : ''}) ${a.intent ? `intent: ${a.intent}` : ''}`).join('\\n')}\\n\\n## Workspace Nodes\\n${workspaceNodes.map(n => `- ${n.type} | ${n.name}\\n  ${n.content.substring(0, 200)}`).join('\\n\\n')}`;
    navigator.clipboard.writeText(report).then(() => notify('Report copied to clipboard', 'info'));
  };

  const runCommand = async () => {
    if (!cmdInput.trim() || !bridge) return;
    setCmdOutput(prev => [...prev, `$ ${cmdInput}`]);
    try {
      const res = await bridge.fetchRaw('/api/v1/bash/execute', {
        method: 'POST',
        body: JSON.stringify({ command: cmdInput })
      });
      setCmdOutput(prev => [...prev, res.output || res.error || 'Done']);
      if (res.error) notify('Command finished with error', 'error');
    } catch (e) {
      setCmdOutput(prev => [...prev, `Error: ${e instanceof Error ? e.message : 'Unknown error'}`]);
      notify('Execution failed', 'error');
    }
    setCmdInput('');
  };

  // Event Collection
  useEffect(() => {
    if (!sessionStart || sessionStopped) return;
    const ev = events[0];
    if (!ev || ev.type !== 'tool_call') return;
    if (ev.ts < sessionStart) return;

    setActivityLog(prev => {
      const entry = {
        ts: ev.ts,
        guild: ev.data.guild || 'unknown',
        tool: ev.data.tool || 'unknown',
        status: ev.data.status as 'started'|'finished',
        ok: ev.data.ok,
        intent: ev.data.intent
      };
      
      // deduplicate by ts + tool
      if (prev.some(p => p.ts === entry.ts && p.tool === entry.tool)) return prev;
      
      return [entry, ...prev].slice(0, 100);
    });
  }, [events, sessionStart, sessionStopped]);

  // SilvaDB Polling
  useEffect(() => {
    if (!sessionStart || sessionStopped || !bridge) return;
    
    const fetchNodes = async () => {
      try {
        const data = await bridge.getSilvaGraph(300);
        const newNodes = (data.nodes || [])
          .filter((n: any) => new Date(n.created_at).getTime() > sessionStart)
          .map((n: any) => ({
            id: n.id,
            type: n.type || n.node_type || 'node',
            name: n.label || n.content || n.id,
            content: n.content || '',
            createdAt: n.created_at
          }));
        setWorkspaceNodes(newNodes);
      } catch (e) {
        console.error('Failed to fetch workspace nodes', e);
      }
    };

    fetchNodes();
    intervalRef.current = setInterval(fetchNodes, 5000);

    return () => {
      if (intervalRef.current) clearInterval(intervalRef.current);
    };
  }, [sessionStart, sessionStopped, bridge]);

  const elapsedSeconds = sessionStart ? Math.floor((Date.now() - sessionStart) / 1000) : 0;
  const elapsedText = elapsedSeconds > 60 ? `${Math.floor(elapsedSeconds/60)}m ${elapsedSeconds%60}s` : `${elapsedSeconds}s`;

  return (
    <div className="h-full flex flex-col gap-3">
      {/* ROW 1 — Header bar */}
      <div className="flex items-center justify-between bg-slate-900/50 p-3 rounded-lg border border-slate-800">
        <div className="flex items-center gap-4">
          <span className="font-bold tracking-widest text-slate-300">SANDBOX LABORATORY</span>
          {sessionStart && !sessionStopped && (
            <span className="text-emerald-400 font-mono text-sm flex items-center gap-1">
              ⧗ {elapsedText}
            </span>
          )}
        </div>
        <div className="flex items-center gap-3">
          {!sessionStart || sessionStopped ? (
            <button onClick={startSession} className="px-4 py-1.5 bg-emerald-500/20 hover:bg-emerald-500/30 text-emerald-400 rounded text-sm font-medium transition-colors">
              Start Session
            </button>
          ) : (
            <button onClick={stopSession} className="px-4 py-1.5 bg-red-500/20 hover:bg-red-500/30 text-red-400 rounded text-sm font-medium transition-colors">
              Stop Session
            </button>
          )}
          <button 
            onClick={exportReport} 
            disabled={!sessionStart || activityLog.length === 0}
            className="px-4 py-1.5 bg-slate-800 hover:bg-slate-700 disabled:opacity-50 disabled:cursor-not-allowed rounded text-sm text-slate-300 transition-colors"
          >
            Export
          </button>
        </div>
      </div>

      {/* ROW 2 — Main body */}
      <div className="flex-1 flex gap-3 min-h-0">
        
        {/* COLUMN A: Activity Stream */}
        <div className="w-64 flex-shrink-0 border-r border-slate-800 pr-3 overflow-y-auto custom-scrollbar">
          <h3 className="text-xs font-bold text-slate-500 uppercase tracking-widest mb-3 sticky top-0 bg-slate-950 pb-2">Activity Stream</h3>
          {!sessionStart ? (
            <div className="text-center text-slate-600 text-xs mt-10">Waiting for session...</div>
          ) : activityLog.length === 0 ? (
            <div className="text-center text-slate-600 text-xs mt-10">No tool calls yet</div>
          ) : (
            <div className="space-y-2">
              {activityLog.map((entry, idx) => (
                <div key={idx} className="p-2 rounded bg-slate-900/40 border border-slate-800/50">
                  <div className="flex items-center gap-2 mb-1">
                    <div className={cn("w-1.5 h-1.5 rounded-full flex-shrink-0", 
                      entry.status === 'started' ? 'bg-amber-500' : (entry.ok ? 'bg-emerald-500' : 'bg-red-500')
                    )} />
                    <span className="text-[10px] text-slate-500 font-mono flex-1">{new Date(entry.ts).toLocaleTimeString()}</span>
                    <span className={cn("text-[9px] px-1.5 py-0.5 rounded font-bold uppercase", 
                      entry.guild === 'bash' ? 'bg-violet-500/20 text-violet-400' :
                      entry.guild === 'filesystem' ? 'bg-blue-500/20 text-blue-400' :
                      entry.guild === 'memory' ? 'bg-emerald-500/20 text-emerald-400' :
                      entry.guild === 'search' ? 'bg-amber-500/20 text-amber-400' :
                      'bg-slate-700 text-slate-400'
                    )}>
                      {entry.guild}
                    </span>
                  </div>
                  <div className="font-mono text-[10px] text-slate-300 mb-1">{entry.tool}</div>
                  {entry.intent && (
                    <div className="text-[10px] text-slate-500 italic truncate max-w-[200px]" title={entry.intent}>
                      "{entry.intent}"
                    </div>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>

        {/* COLUMN B: Workspace */}
        <div className="flex-1 overflow-y-auto pr-3 custom-scrollbar">
          <div className="flex items-center justify-between mb-3 sticky top-0 bg-slate-950 pb-2">
            <h3 className="text-xs font-bold text-slate-500 uppercase tracking-widest">Workspace</h3>
            <span className="text-xs font-mono text-slate-500">{workspaceNodes.length} nodes</span>
          </div>
          
          {workspaceNodes.length === 0 ? (
            <div className="flex items-center justify-center h-full text-slate-600 text-xs">
              Knowledge nodes written by agents appear here
            </div>
          ) : (
            <div className="grid grid-cols-1 gap-2">
              {workspaceNodes.map((node, i) => (
                <div key={node.id + i} className="p-3 rounded-lg border border-slate-800 bg-slate-900/60">
                  <div className="flex items-center justify-between mb-2">
                    <div className="flex items-center gap-2 overflow-hidden">
                      <span className="text-[9px] px-2 py-0.5 rounded bg-slate-800 text-slate-400 font-bold uppercase flex-shrink-0">
                        {node.type}
                      </span>
                      <span className="text-xs font-medium text-slate-200 truncate max-w-[300px]" title={node.name}>
                        {node.name}
                      </span>
                    </div>
                    <span className="text-[10px] text-slate-500 whitespace-nowrap">
                      {new Date(node.createdAt).toLocaleTimeString()}
                    </span>
                  </div>
                  <p className="font-mono text-[10px] text-slate-400 line-clamp-2 leading-relaxed">
                    {node.content}
                  </p>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* COLUMN C: Session Report */}
        <div className="w-72 flex-shrink-0 border-l border-slate-800 pl-3 overflow-y-auto custom-scrollbar">
          <h3 className="text-xs font-bold text-slate-500 uppercase tracking-widest mb-3 sticky top-0 bg-slate-950 pb-2">Session Report</h3>
          
          {!sessionStart ? (
            <div className="text-center text-slate-600 text-xs mt-10">Start a session to generate a report</div>
          ) : (
            <div className="space-y-4">
              <div className="p-3 rounded-lg bg-slate-900 border border-slate-800">
                <div className="flex items-center justify-between mb-3">
                  <span className="text-xs text-slate-400">Status</span>
                  {!sessionStopped ? (
                    <div className="flex items-center gap-1.5">
                      <div className="w-1.5 h-1.5 rounded-full bg-emerald-500 animate-pulse" />
                      <span className="text-xs font-bold text-emerald-400">ACTIVE</span>
                    </div>
                  ) : (
                    <span className="text-xs font-bold text-slate-500">STOPPED</span>
                  )}
                </div>
                <div className="flex items-center justify-between mb-3 text-xs">
                  <span className="text-slate-400">Duration</span>
                  <span className="font-mono text-slate-300">{elapsedText}</span>
                </div>
                
                <div className="border-t border-slate-800/50 my-2 pt-2" />
                
                <div className="flex items-center justify-between text-xs mb-1">
                  <span className="text-slate-400">Tool Calls</span>
                  <span className="font-mono text-slate-300">{activityLog.length}</span>
                </div>
                <div className="flex items-center justify-between text-[10px] text-slate-500 mb-1">
                  <span>Success</span>
                  <span>{activityLog.filter(a => a.ok === true).length}</span>
                </div>
                <div className="flex items-center justify-between text-[10px] text-slate-500 mb-2">
                  <span>Error</span>
                  <span>{activityLog.filter(a => a.ok === false).length}</span>
                </div>
                
                <div className="border-t border-slate-800/50 my-2 pt-2" />
                
                <div className="flex items-center justify-between text-xs mb-1">
                  <span className="text-slate-400">Knowledge Nodes</span>
                  <span className="font-mono text-slate-300">{workspaceNodes.length}</span>
                </div>
                {workspaceNodes.length > 0 && (
                  <div className="flex flex-wrap gap-1 mt-2">
                    {Array.from(new Set(workspaceNodes.map(n => n.type))).map(type => (
                      <span key={type as string} className="text-[9px] px-1.5 py-0.5 rounded bg-slate-800 text-slate-400 uppercase">
                        {type as string}
                      </span>
                    ))}
                  </div>
                )}
                
                <button 
                  onClick={exportReport} 
                  className="w-full mt-4 py-2 bg-slate-800 hover:bg-slate-700 rounded text-xs text-slate-300 transition-colors flex items-center justify-center gap-2"
                >
                  📋 Export Markdown
                </button>
              </div>

              <div className="border-t border-slate-800 my-4" />

              <div>
                <button 
                  onClick={() => setShowTerminal(!showTerminal)}
                  className="flex items-center gap-2 text-xs text-slate-400 hover:text-slate-200 transition-colors w-full p-2 bg-slate-900/50 rounded border border-slate-800"
                >
                  <Terminal className="w-3 h-3" /> 
                  <span className="font-bold tracking-wider">DEBUG TERMINAL</span>
                </button>
                
                {showTerminal && (
                  <div className="mt-3 space-y-2 animate-in fade-in slide-in-from-top-2 duration-200">
                    <div className="flex gap-2">
                      <input
                        type="text"
                        value={cmdInput}
                        onChange={(e) => setCmdInput(e.target.value)}
                        onKeyDown={(e) => e.key === 'Enter' && runCommand()}
                        placeholder="Bash command..."
                        className="flex-1 px-2 py-1.5 bg-slate-900 border border-slate-800 rounded text-xs font-mono text-slate-300"
                      />
                      <button
                        onClick={runCommand}
                        disabled={!cmdInput.trim()}
                        className="px-3 py-1.5 bg-slate-800 hover:bg-slate-700 rounded text-xs text-slate-300"
                      >
                        Run
                      </button>
                    </div>
                    <div className="bg-slate-900 border border-slate-800 p-2 rounded text-[10px] font-mono text-slate-400 max-h-32 overflow-y-auto flex flex-col gap-1">
                      {cmdOutput.length === 0 ? (
                        <span className="italic opacity-50">Terminal output...</span>
                      ) : (
                        cmdOutput.map((line, i) => (
                          <div key={i} className="whitespace-pre-wrap">{line}</div>
                        ))
                      )}
                    </div>
                  </div>
                )}
              </div>
            </div>
          )}
        </div>

      </div>
    </div>
  );
}
"""
    final_content = content[:start_idx] + new_content + content[end_idx:]
    with open('App.tsx', 'w', encoding='utf-8') as f:
        f.write(final_content)
    print('Successfully updated via Python.')
else:
    print(f'start_idx: {start_idx}, end_idx: {end_idx}')
