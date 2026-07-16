import React, { useState, useEffect } from 'react';
import { Database, Terminal, MessageSquare, PlusCircle, CheckCircle2, Circle, Copy, Check, User } from 'lucide-react';
import { cn } from '../lib/utils';
import { useNexus } from '../hooks/useNexus';

interface WelcomeEmptyStateProps {
  bridge: any;
  sysStatus: any;
  sessions: any[];
  memoryStats: any;
  notify: (msg: string, type?: 'info' | 'error') => void;
  onRefresh: () => void;
}

export function WelcomeEmptyState({
  bridge,
  sysStatus,
  sessions,
  memoryStats,
  notify,
  onRefresh
}: WelcomeEmptyStateProps) {
  const { agentProfiles } = useNexus();
  const [firstQueryText, setFirstQueryText] = useState('What is Tylluan?');
  const [noteText, setNoteText] = useState('Tylluan is running local RAG.');
  const [copied, setCopied] = useState(false);
  const [addingNote, setAddingNote] = useState(false);
  const [querying, setQuerying] = useState(false);

  // States for checklist (using localStorage and api telemetry)
  const isInstalled = true; // By definition if they see the dashboard
  const isProfileResolved = agentProfiles && agentProfiles.length > 0;
  const isModelLoaded = !!(sysStatus?.embeddings_loaded || sysStatus?.silva_healthy);
  
  // MCP is connected if there is at least one session, or if they checked the box
  const [mcpChecked, setMcpChecked] = useState(() => localStorage.getItem('tylluan_wizard_mcp') === 'true');
  const isMcpConnected = sessions?.length > 0 || mcpChecked;

  // First query completed state
  const [firstQueryDone, setFirstQueryDone] = useState(() => localStorage.getItem('tylluan_wizard_query') === 'true');

  useEffect(() => {
    if (sessions?.length > 0 && !mcpChecked) {
      setMcpChecked(true);
      localStorage.setItem('tylluan_wizard_mcp', 'true');
    }
  }, [sessions, mcpChecked]);

  const handleCopyMcp = () => {
    const config = {
      mcpServers: {
        tylluan: {
          type: "sse",
          url: `http://127.0.0.1:3030/sse`
        }
      }
    };
    navigator.clipboard.writeText(JSON.stringify(config, null, 2));
    setCopied(true);
    notify('MCP Configuration copied to clipboard', 'info');
    setMcpChecked(true);
    localStorage.setItem('tylluan_wizard_mcp', 'true');
    setTimeout(() => setCopied(false), 2000);
  };

  const handleAddNote = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!noteText.trim() || !bridge) return;
    setAddingNote(true);
    try {
      await bridge.fetchRaw('/api/v1/memory/remember', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ content: noteText })
      });
      notify('First note stored in SilvaDB!', 'info');
      setNoteText('');
      onRefresh();
    } catch (err: any) {
      notify(`Failed to add note: ${err.message || 'Unknown error'}`, 'error');
    } finally {
      setAddingNote(false);
    }
  };

  const handleQuery = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!firstQueryText.trim() || !bridge) return;
    setQuerying(false);
    setFirstQueryDone(true);
    localStorage.setItem('tylluan_wizard_query', 'true');
    notify('Simulation of query completed!', 'info');
  };

  const mcpConfigJson = `{
  "mcpServers": {
    "tylluan": {
      "type": "sse",
      "url": "http://127.0.0.1:3030/sse"
    }
  }
}`;

  return (
    <div className="max-w-5xl mx-auto space-y-8 py-8 animate-in fade-in duration-500">
      {/* Hero Section */}
      <div className="text-center space-y-3">
        <div className="w-16 h-16 mx-auto bg-gradient-to-br from-emerald-400 to-teal-500 rounded-2xl flex items-center justify-center shadow-lg shadow-emerald-500/20">
          <Database className="w-8 h-8 text-slate-950" />
        </div>
        <h2 className="text-3xl font-extrabold tracking-tight text-slate-50 uppercase">
          Welcome to Tylluan
        </h2>
        <p className="text-slate-400 text-sm max-w-md mx-auto">
          Your sovereign AI memory is ready. SilvaDB is initialized and listening local.
        </p>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-4 gap-6">
        {/* Onboarding Tasks Checklist */}
        <div className="lg:col-span-1 bg-slate-900/40 border border-slate-800/80 rounded-2xl p-5 space-y-4">
          <h3 className="text-[10px] font-bold text-slate-400 uppercase tracking-widest border-b border-slate-800 pb-2">
            Onboarding Checklist
          </h3>
          <div className="space-y-3">
            {[
              { id: 'install', label: 'Install Tylluan', done: isInstalled },
              { id: 'profile', label: 'Create Agent Profile', done: isProfileResolved },
              { id: 'model', label: 'Embeddings Engine', done: isModelLoaded },
              { id: 'mcp', label: 'Connect MCP Client', done: isMcpConnected },
              { id: 'query', label: 'First Query / Action', done: firstQueryDone }
            ].map(task => (
              <div key={task.id} className="flex items-center gap-3 text-xs">
                {task.done ? (
                  <CheckCircle2 className="w-4 h-4 text-emerald-400 shrink-0" />
                ) : (
                  <Circle className="w-4 h-4 text-slate-600 shrink-0" />
                )}
                <span className={cn(task.done ? "text-slate-300 line-through decoration-slate-600" : "text-slate-400 font-medium")}>
                  {task.label}
                </span>
              </div>
            ))}
          </div>

          <div className="pt-2">
            <button
              onClick={() => {
                localStorage.setItem('tylluan_wizard_query', 'true');
                localStorage.setItem('tylluan_wizard_mcp', 'true');
                setFirstQueryDone(true);
                setMcpChecked(true);
                notify('Skipped onboarding tutorial', 'info');
              }}
              className="w-full text-center text-[10px] text-slate-500 hover:text-slate-300 uppercase tracking-wider font-bold mt-2 cursor-pointer transition-colors"
            >
              Skip Tutorial
            </button>
          </div>
        </div>

        {/* 3 Core Action Cards */}
        <div className="lg:col-span-3 grid grid-cols-1 md:grid-cols-3 gap-6">
          
          {/* Card 1: Add First Note */}
          <div className="bg-slate-900/60 border border-slate-800/50 rounded-2xl p-5 flex flex-col justify-between hover:border-slate-700/50 transition-all group">
            <div className="space-y-3">
              <div className="p-2 w-9 h-9 rounded-xl bg-emerald-500/10 border border-emerald-500/20 text-emerald-400 flex items-center justify-center">
                <PlusCircle className="w-5 h-5" />
              </div>
              <h3 className="text-sm font-bold text-slate-50 uppercase tracking-wider">1. Add Note</h3>
              <p className="text-[11px] text-slate-400 leading-relaxed">
                Save a key-value or raw memory into your database.
              </p>
              
              <form onSubmit={handleAddNote} className="space-y-2 pt-2">
                <input
                  type="text"
                  value={noteText}
                  onChange={(e) => setNoteText(e.target.value)}
                  placeholder="e.g. Tylluan is RAG."
                  className="w-full bg-slate-950 border border-slate-800 rounded-lg px-2.5 py-1.5 text-xs text-slate-200 focus:outline-none focus:border-emerald-500 font-sans"
                  required
                />
                <button
                  type="submit"
                  disabled={addingNote}
                  className="w-full py-1.5 bg-emerald-500/10 border border-emerald-500/30 text-emerald-400 hover:bg-emerald-500 hover:text-slate-950 rounded-lg text-[10px] font-bold uppercase tracking-wider transition-all duration-200 cursor-pointer"
                >
                  {addingNote ? 'Saving...' : 'Remember'}
                </button>
              </form>
            </div>
          </div>

          {/* Card 2: Connect Tools */}
          <div className="bg-slate-900/60 border border-slate-800/50 rounded-2xl p-5 flex flex-col justify-between hover:border-slate-700/50 transition-all">
            <div className="space-y-3">
              <div className="p-2 w-9 h-9 rounded-xl bg-blue-500/10 border border-blue-500/20 text-blue-400 flex items-center justify-center">
                <Terminal className="w-5 h-5" />
              </div>
              <h3 className="text-sm font-bold text-slate-50 uppercase tracking-wider">2. Connect Client</h3>
              <p className="text-[11px] text-slate-400 leading-relaxed">
                Configure your MCP workspace client to use Tylluan.
              </p>

              <div className="relative mt-2">
                <pre className="bg-slate-950 border border-slate-800 rounded-lg p-2.5 text-[9px] font-mono text-slate-300 leading-normal overflow-x-auto select-all max-h-[85px]">
                  {mcpConfigJson}
                </pre>
                <button
                  onClick={handleCopyMcp}
                  className="absolute top-1.5 right-1.5 p-1 bg-slate-900 border border-slate-800 hover:border-slate-700 text-slate-400 hover:text-slate-200 rounded transition-colors cursor-pointer"
                  title="Copy configuration"
                >
                  {copied ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Copy className="w-3.5 h-3.5" />}
                </button>
              </div>
            </div>
          </div>

          {/* Card 3: Query Memory */}
          <div className="bg-slate-900/60 border border-slate-800/50 rounded-2xl p-5 flex flex-col justify-between hover:border-slate-700/50 transition-all">
            <div className="space-y-3">
              <div className="p-2 w-9 h-9 rounded-xl bg-violet-500/10 border border-violet-500/20 text-violet-400 flex items-center justify-center">
                <MessageSquare className="w-5 h-5" />
              </div>
              <h3 className="text-sm font-bold text-slate-50 uppercase tracking-wider">3. Test Query</h3>
              <p className="text-[11px] text-slate-400 leading-relaxed">
                Run a simulation test query against SilvaDB.
              </p>

              <form onSubmit={handleQuery} className="space-y-2 pt-2">
                <input
                  type="text"
                  value={firstQueryText}
                  onChange={(e) => setFirstQueryText(e.target.value)}
                  placeholder="Ask anything..."
                  className="w-full bg-slate-950 border border-slate-800 rounded-lg px-2.5 py-1.5 text-xs text-slate-200 focus:outline-none focus:border-violet-500 font-sans"
                  required
                />
                <button
                  type="submit"
                  disabled={querying}
                  className="w-full py-1.5 bg-violet-500/10 border border-violet-500/30 text-violet-400 hover:bg-violet-500 hover:text-slate-950 rounded-lg text-[10px] font-bold uppercase tracking-wider transition-all duration-200 cursor-pointer"
                >
                  {querying ? 'Querying...' : 'Ask'}
                </button>
              </form>
            </div>
          </div>

        </div>
      </div>
    </div>
  );
}
