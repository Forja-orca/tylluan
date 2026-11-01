import React, { useState, useEffect, useCallback } from 'react';
import { RefreshCw, Send, Radio } from 'lucide-react';
import { cn } from '../lib/utils';
import { usePolling } from '../hooks/usePolling';
import { useNexus } from '../hooks/useNexus';
import type { NexusBridge } from '../lib/api-client';

interface NodeInfo {
  agent_id: string;
  inbox_pending: number;
  rules: number;
  registered_at: number;
  last_active: number;
}

export function NodesTab({ bridge: _bridgeProp, notify }: { bridge: unknown; notify: (msg: string, t?: 'info' | 'error') => void }) {
  const { bridge } = useNexus();
  const effectiveBridge: NexusBridge | null = bridge ?? (_bridgeProp as NexusBridge | null);
  const [nodes, setNodes] = useState<NodeInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [targetId, setTargetId] = useState('');
  const [message, setMessage] = useState('');

  const fetchNodes = useCallback(async () => {
    if (!effectiveBridge) return;
    try {
      const data = await effectiveBridge.fetchRaw('/api/v1/nodes');
      setNodes(data.nodes || []);
    } catch (e) {
      notify(`Error al consultar nodos: ${e}`, 'error');
    } finally {
      setLoading(false);
    }
  }, [effectiveBridge, notify]);

  useEffect(() => { 
    fetchNodes(); 
  }, [fetchNodes]);

  usePolling('nodes-fetch', fetchNodes, { interval: 'standard', enabled: true });

  const sendMessage = async () => {
    if (!targetId || !message || !effectiveBridge) return;
    try {
      const data = await effectiveBridge.fetchRaw(`/api/v1/nodes/${encodeURIComponent(targetId)}/send`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ from: 'dashboard', payload: message, msg_type: 'direct' }),
      });
      if (data.delivered) {
        notify(`Mensaje entregado al nodo ${targetId}`, 'info');
        setMessage('');
      } else {
        notify(`Error al entregar mensaje: ${JSON.stringify(data)}`, 'error');
      }
    } catch (e) {
      notify(`Error al enviar mensaje: ${e}`, 'error');
    }
  };

  const registerNode = async () => {
    if (!effectiveBridge) return;
    try {
      const data = await effectiveBridge.fetchRaw('/api/v1/nodes/dashboard/register', { method: 'POST' });
      notify(`Nodo registrado: ${data.status || 'correcto'}`, 'info');
      fetchNodes();
    } catch (e) {
      notify(`Error al registrar nodo: ${e}`, 'error');
    }
  };

  const quickAgents = ['claude-code', 'deep', 'antigravity', 'buffy'];

  return (
    <div className="flex-1 min-h-0 p-1 flex flex-col space-y-4 font-sans">
      {/* Header - Sovereign Card */}
      <div className="p-4 bg-card border border-border rounded-lg shadow-sm flex flex-col md:flex-row items-start md:items-center justify-between gap-3">
        <div>
          <div className="flex items-center gap-2">
            <span className="px-2 py-0.5 rounded-md text-[10px] font-mono font-bold bg-primary/10 text-primary border border-primary/30">
              Topología P2P
            </span>
            <span className="px-2 py-0.5 rounded-md text-[10px] font-mono font-bold bg-emerald-500/10 text-emerald-700 dark:text-emerald-400 border border-emerald-500/30">
              Pizarra Activa
            </span>
          </div>
          <h2 className="text-sm font-bold text-foreground mt-1.5 flex items-center gap-1.5">
            <Radio className="w-4 h-4 text-primary" />
            Nodos del Sustrato y Mensajería Directa
          </h2>
          <p className="text-xs text-muted-foreground mt-0.5">
            Agentes registrados en la flota, colas de buzón y canales de comunicación directa punto a punto.
          </p>
        </div>

        <div className="flex items-center gap-2">
          <button 
            type="button"
            aria-label="Sincronizar nodos de agentes"
            onClick={fetchNodes} 
            className="flex items-center gap-1.5 px-3 py-1.5 bg-muted/60 hover:bg-muted text-xs font-semibold text-foreground rounded-md border border-border shadow-sm transition-all cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
          >
            <RefreshCw className={cn('w-3.5 h-3.5 text-primary', loading && 'animate-spin')} aria-hidden="true" /> 
            Sincronizar
          </button>
          <button 
            type="button"
            aria-label="Registrar nodo del panel"
            onClick={registerNode} 
            className="flex items-center gap-1.5 px-3 py-1.5 bg-primary hover:bg-primary/90 text-primary-foreground text-xs font-semibold rounded-md border border-primary shadow-sm transition-all cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
          >
            <Radio className="w-3.5 h-3.5" aria-hidden="true" /> 
            Registrar nodo del panel
          </button>
        </div>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        {/* Active Nodes List */}
        <fieldset className="border border-border p-4 bg-card rounded-lg shadow-sm space-y-3">
          <legend className="text-xs font-bold text-primary px-1">
            Nodos Conectados a la Flota ({nodes.length})
          </legend>

          {loading ? (
            <div role="status" aria-live="polite" className="text-center py-6 text-xs text-muted-foreground">
              Consultando registro de nodos...
            </div>
          ) : nodes.length === 0 ? (
            <div className="text-center py-6 text-xs text-muted-foreground bg-muted/30 border border-border p-4 rounded-lg">
              <p className="font-bold text-foreground">No hay nodos activos registrados</p>
              <p className="mt-1">Registre un nodo mediante la herramienta tylluan_do o haga clic en 'Registrar nodo del panel'.</p>
            </div>
          ) : (
            <div className="overflow-x-auto rounded-lg border border-border">
              <table className="w-full border-collapse bg-card text-xs">
                <thead>
                  <tr className="bg-muted/60 text-left border-b border-border text-foreground">
                    <th className="p-2 font-semibold border-r border-border/60">Agente</th>
                    <th className="p-2 font-semibold border-r border-border/60 text-center">Reglas</th>
                    <th className="p-2 font-semibold border-r border-border/60 text-center">Buzón</th>
                    <th className="p-2 font-semibold text-center">Acción</th>
                  </tr>
                </thead>
                <tbody>
                  {nodes.map((n) => (
                    <tr key={n.agent_id} className="border-b border-border hover:bg-muted/40">
                      <td className="p-2 font-mono font-bold text-primary border-r border-border/60">
                        @{n.agent_id}
                      </td>
                      <td className="p-2 font-mono text-center border-r border-border/60">
                        {n.rules}
                      </td>
                      <td className="p-2 font-mono text-center border-r border-border/60">
                        <span className={n.inbox_pending > 0 ? "font-bold text-amber-700 dark:text-amber-400" : "text-muted-foreground"}>
                          {n.inbox_pending}
                        </span>
                      </td>
                      <td className="p-2 text-center">
                        <button
                          type="button"
                          aria-label={`Seleccionar ${n.agent_id} como destino`}
                          onClick={() => setTargetId(n.agent_id)}
                          className="px-2.5 py-1 bg-muted/60 hover:bg-muted text-foreground text-[11px] font-semibold rounded-md border border-border cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
                        >
                          Seleccionar
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </fieldset>

        {/* Direct Messaging Dispatcher */}
        <fieldset className="border border-border p-4 bg-card rounded-lg shadow-sm space-y-3">
          <legend className="text-xs font-bold text-primary px-1">
            Despacho de Mensaje Directo
          </legend>

          <div className="space-y-3">
            <div>
              <label htmlFor="target-agent-id" className="text-xs font-bold text-foreground block mb-1">
                Agente destinatario:
              </label>
              <input
                id="target-agent-id"
                type="text"
                value={targetId}
                onChange={(e) => setTargetId(e.target.value)}
                placeholder="ej. claude-code, deep, antigravity"
                className="w-full px-2.5 py-1.5 bg-muted/40 border border-border text-foreground text-xs font-mono rounded-md focus:ring-2 focus:ring-primary outline-none"
              />
              {/* Quick Select Badges */}
              <div role="group" aria-label="Selección rápida de agente" className="flex flex-wrap gap-1.5 mt-1.5">
                {quickAgents.map((ag) => (
                  <button
                    key={ag}
                    type="button"
                    aria-pressed={targetId === ag}
                    onClick={() => setTargetId(ag)}
                    className={cn(
                      "px-2 py-0.5 text-[10px] font-mono font-bold rounded-md border transition-all cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none",
                      targetId === ag
                        ? "bg-primary text-primary-foreground border-primary"
                        : "bg-muted/50 text-muted-foreground border-border hover:bg-muted hover:text-foreground"
                    )}
                  >
                    @{ag}
                  </button>
                ))}
              </div>
            </div>

            <div>
              <label htmlFor="substrate-payload-msg" className="text-xs font-bold text-foreground block mb-1">
                Carga útil / Mensaje directo:
              </label>
              <textarea
                id="substrate-payload-msg"
                value={message}
                onChange={(e) => setMessage(e.target.value)}
                placeholder="Escriba el mensaje directo para el nodo seleccionado..."
                rows={4}
                className="w-full p-2.5 bg-muted/40 border border-border text-foreground text-xs font-sans rounded-md focus:ring-2 focus:ring-primary outline-none resize-none"
              />
            </div>

            <button
              type="button"
              onClick={sendMessage}
              disabled={!targetId || !message}
              className="w-full flex items-center justify-center gap-1.5 py-2 bg-primary hover:bg-primary/90 text-primary-foreground text-xs font-semibold rounded-md border border-primary shadow-sm transition-all disabled:opacity-40 cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
            >
              <Send className="w-3.5 h-3.5" aria-hidden="true" />
              <span>Despachar Mensaje al Nodo</span>
            </button>
          </div>
        </fieldset>
      </div>
    </div>
  );
}
