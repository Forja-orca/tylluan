import React, { useState, useEffect } from 'react';
import type { NexusBridge, GraphNode } from '../lib/nexus-bridge';
import { Search, ShieldAlert, Layers, Folder, User, Terminal, Cpu } from 'lucide-react';
import { cn } from '../lib/utils';

interface Props {
  bridge: NexusBridge | null;
  notify: (msg: string, type?: 'info' | 'error') => void;
}

export function ScopesPanel({ bridge, notify }: Props) {
  const [prefix, setPrefix] = useState('');
  const [nodes, setNodes] = useState<GraphNode[]>([]);
  const [loading, setLoading] = useState(false);
  const [simulated, setSimulated] = useState(false);

  const fetchNodes = async (searchPrefix: string) => {
    if (!bridge) return;
    setLoading(true);
    try {
      const res = await bridge.getNodesByScopePrefix(searchPrefix);
      setNodes(res || []);
      setSimulated(false);
    } catch (e: any) {
      console.error("Error consultando nodos por ámbito:", e.message);
      setSimulated(false);
      setNodes([]);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchNodes(prefix);
  }, [bridge, prefix]);

  const renderScopeBreadcrumbs = (scope?: string) => {
    if (!scope) return <span className="text-slate-600 italic">No scope owner</span>;
    const parts = scope.split('/');
    return (
      <div className="flex items-center gap-1.5 flex-wrap">
        {parts.map((part, index) => {
          const [key, value] = part.split(':');
          let Icon = Folder;
          let badgeColor = "bg-slate-800 text-slate-300 border-slate-700";

          if (key === 'user') {
            Icon = User;
            badgeColor = "bg-sky-500/10 text-sky-400 border-sky-500/20";
          } else if (key === 'session') {
            Icon = Terminal;
            badgeColor = "bg-purple-500/10 text-purple-400 border-purple-500/20";
          } else if (key === 'agent') {
            Icon = Cpu;
            badgeColor = "bg-emerald-500/10 text-emerald-400 border-emerald-500/20";
          }

          return (
            <React.Fragment key={index}>
              {index > 0 && <span className="text-slate-700 text-xs font-bold">/</span>}
              <span className={cn("px-2 py-0.5 rounded text-[10px] font-mono font-semibold border flex items-center gap-1", badgeColor)}>
                <Icon className="w-3 h-3" />
                <span className="opacity-60">{key}:</span>
                <span className="font-bold">{value}</span>
              </span>
            </React.Fragment>
          );
        })}
      </div>
    );
  };

  return (
    <div className="flex flex-col space-y-4 h-full">
      {/* Header & Controls */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 p-4 rounded-xl bg-slate-900 border border-slate-800">
        <div className="space-y-1">
          <div className="flex items-center gap-2">
            <h3 className="text-sm font-bold text-slate-50 uppercase tracking-wider">Multi-Tenant Hierarchical Scopes (J-8)</h3>
            {simulated && (
              <span className="px-2 py-0.5 bg-amber-500/10 border border-amber-500/20 text-amber-500 text-[10px] font-mono font-bold rounded flex items-center gap-1">
                <ShieldAlert className="w-3 h-3" /> [SIMULADO]
              </span>
            )}
          </div>
          <p className="text-[11px] text-slate-500 font-mono">Query lightweight nodes by hierarchical owner scope prefix</p>
        </div>

        <div className="flex gap-2 max-w-md w-full sm:w-80">
          <div className="relative flex-1">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-slate-500" />
            <input
              type="text"
              value={prefix}
              onChange={(e) => setPrefix(e.target.value)}
              placeholder="e.g. user:alice"
              className="w-full pl-10 pr-4 py-2 bg-slate-950/80 border border-slate-800 rounded-lg text-xs focus:ring-1 ring-emerald-500 text-slate-200 transition-all placeholder:text-slate-600"
            />
          </div>
          <button
            onClick={() => fetchNodes(prefix)}
            className="px-3 py-2 bg-slate-800 border border-slate-700 hover:bg-slate-700 text-xs font-bold text-slate-300 rounded-lg transition-colors flex items-center gap-1 shrink-0 cursor-pointer"
          >
            {loading ? <Layers className="w-3.5 h-3.5 animate-spin text-emerald-400" /> : <Search className="w-3.5 h-3.5" />}
            Buscar
          </button>
        </div>
      </div>

      {/* Nodes List */}
      <div className="flex-1 min-h-0 bg-slate-900/50 rounded-xl border border-slate-800 p-4 flex flex-col overflow-hidden">
        {loading ? (
          <div className="flex-grow flex items-center justify-center text-xs text-slate-500 font-mono gap-2">
            <Layers className="w-4 h-4 animate-spin text-emerald-400" /> Cargando nodos por scope...
          </div>
        ) : nodes.length === 0 ? (
          <div className="flex-grow flex flex-col items-center justify-center text-slate-600 py-12">
            <Folder className="w-8 h-8 opacity-30 mb-2" />
            <p className="text-xs font-medium">No se encontraron nodos para el scope "{prefix || 'root'}"</p>
            <p className="text-[10px] opacity-60 mt-1">Prueba con "user:alice" o deja vacío para ver todos.</p>
          </div>
        ) : (
          <div className="flex-grow overflow-y-auto">
            <div className="rounded-lg border border-slate-800/80 bg-slate-950 overflow-hidden">
              <table className="w-full text-left border-collapse">
                <thead>
                  <tr className="bg-slate-900 border-b border-slate-800 text-[10px] uppercase tracking-widest text-slate-500 font-bold">
                    <th className="px-4 py-3 font-bold w-1/4">Identifier</th>
                    <th className="px-4 py-3 font-bold w-12 text-center">Type</th>
                    <th className="px-4 py-3 font-bold w-1/3">Hierarchy (owner_scope)</th>
                    <th className="px-4 py-3 font-bold">Content Preview</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-800/50">
                  {nodes.map((node) => {
                    const nodeType = node.node_type || node.type || 'entity';
                    return (
                      <tr key={node.id} className="hover:bg-slate-900/40 transition-colors">
                        <td className="px-4 py-3.5 text-[10px] font-mono text-violet-400 truncate max-w-[150px]" title={node.id}>
                          {node.id}
                        </td>
                        <td className="px-4 py-3.5 text-center">
                          <span className={cn(
                            "px-1.5 py-0.5 rounded text-[9px] font-bold uppercase border",
                            nodeType === 'lesson' ? "bg-violet-500/10 text-violet-400 border-violet-500/20" :
                            nodeType === 'identity' ? "bg-emerald-500/10 text-emerald-400 border-emerald-500/20" :
                            nodeType === 'concept' ? "bg-blue-500/10 text-blue-400 border-blue-500/20" :
                            "bg-amber-500/10 text-amber-400 border-amber-500/20"
                          )}>
                            {nodeType}
                          </span>
                        </td>
                        <td className="px-4 py-3.5">
                          {renderScopeBreadcrumbs(node.owner_scope)}
                        </td>
                        <td className="px-4 py-3.5 text-xs text-slate-400 max-w-xs truncate" title={node.content}>
                          {node.content || <span className="italic text-slate-600">— sin contenido —</span>}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
