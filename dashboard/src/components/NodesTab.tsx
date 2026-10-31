import React, { useState, useEffect, useCallback } from 'react';
import { RefreshCw, Send, Radio, Users, Cpu, Inbox } from 'lucide-react';
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
    <div className="flex-1 min-h-0 p-3 flex flex-col space-y-3 font-sans">
      {/* Header - Windows 2000/XP Wizard Style */}
      <div className="p-3 bg-[#ECE9D8] dark:bg-card border border-[#ACA899] dark:border-border rounded-sm flex flex-col md:flex-row items-start md:items-center justify-between gap-3">
        <div>
          <div className="flex items-center gap-2">
            <span className="px-1.5 py-0.2 text-[10px] font-mono font-bold bg-white dark:bg-muted text-[#0A246A] dark:text-primary border border-[#7F9DB9] dark:border-border">
              Topología P2P
            </span>
            <span className="px-1.5 py-0.2 text-[10px] font-mono font-bold bg-white dark:bg-muted text-[#008000] dark:text-emerald-400 border border-[#7F9DB9] dark:border-border">
              Pizarra Activa
            </span>
          </div>
          <h2 className="text-xs font-bold text-[#0A246A] dark:text-primary mt-1 flex items-center gap-1.5">
            <Radio className="w-4 h-4 text-[#0A246A] dark:text-primary" />
            Nodos del Sustrato y Mensajería Directa
          </h2>
          <p className="text-[11px] text-[#444444] dark:text-muted-foreground mt-0.5">
            Agentes registrados en la flota, colas de buzón y canales de comunicación directa punto a punto.
          </p>
        </div>

        <div className="flex items-center gap-1.5">
          <button 
            type="button"
            aria-label="Sincronizar nodos de agentes"
            onClick={fetchNodes} 
            className="flex items-center gap-1 px-2.5 py-1 bg-white dark:bg-muted hover:bg-[#F5F4EA] text-[11px] font-bold text-[#000000] dark:text-foreground rounded-sm border border-[#ACA899] dark:border-border transition-all cursor-pointer focus-visible:outline-1 focus-visible:outline-[#0A246A]"
          >
            <RefreshCw className={cn('w-3 h-3 text-[#0A246A]', loading && 'animate-spin')} aria-hidden="true" /> 
            Sincronizar
          </button>
          <button 
            type="button"
            aria-label="Registrar nodo del panel"
            onClick={registerNode} 
            className="flex items-center gap-1 px-2.5 py-1 bg-[#0A246A] hover:bg-[#081C52] text-white text-[11px] font-bold rounded-sm border border-[#0A246A] transition-all cursor-pointer focus-visible:outline-1 focus-visible:outline-[#0A246A]"
          >
            <Radio className="w-3 h-3" aria-hidden="true" /> 
            Registrar nodo del panel
          </button>
        </div>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
        {/* Active Nodes List */}
        <fieldset className="border border-[#ACA899] dark:border-border p-3 bg-[#ECE9D8] dark:bg-card rounded-sm space-y-2">
          <legend className="text-[11px] font-bold text-[#0A246A] dark:text-primary px-1">
            Nodos Conectados a la Flota ({nodes.length})
          </legend>

          {loading ? (
            <div role="status" aria-live="polite" className="text-center py-6 text-[11px] text-[#555555]">
              Consultando registro de nodos...
            </div>
          ) : nodes.length === 0 ? (
            <div className="text-center py-6 text-[11px] text-[#555555] bg-white dark:bg-card border border-[#ACA899] p-3 rounded-none">
              <p className="font-bold text-[#0A246A] dark:text-foreground">No hay nodos activos registrados</p>
              <p className="mt-0.5">Registre un nodo mediante la herramienta tylluan_do o haga clic en 'Registrar nodo del panel'.</p>
            </div>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full border-collapse bg-white dark:bg-card border border-[#7F9DB9] dark:border-border text-[11px]">
                <thead>
                  <tr className="bg-[#ECE9D8] dark:bg-muted text-left border-b border-[#ACA899]">
                    <th className="p-1.5 font-bold border-r border-[#ACA899]">Agente</th>
                    <th className="p-1.5 font-bold border-r border-[#ACA899] text-center">Reglas</th>
                    <th className="p-1.5 font-bold border-r border-[#ACA899] text-center">Buzón</th>
                    <th className="p-1.5 font-bold text-center">Acción</th>
                  </tr>
                </thead>
                <tbody>
                  {nodes.map((n) => (
                    <tr key={n.agent_id} className="border-b border-[#E0E0E0] dark:border-border hover:bg-[#F5F4EA] dark:hover:bg-muted/40">
                      <td className="p-1.5 font-mono font-bold text-[#0A246A] dark:text-primary border-r border-[#E0E0E0]">
                        @{n.agent_id}
                      </td>
                      <td className="p-1.5 font-mono text-center border-r border-[#E0E0E0]">
                        {n.rules}
                      </td>
                      <td className="p-1.5 font-mono text-center border-r border-[#E0E0E0]">
                        <span className={n.inbox_pending > 0 ? "font-bold text-[#B8860B]" : "text-[#555555]"}>
                          {n.inbox_pending}
                        </span>
                      </td>
                      <td className="p-1.5 text-center">
                        <button
                          type="button"
                          aria-label={`Seleccionar ${n.agent_id} como destino`}
                          onClick={() => setTargetId(n.agent_id)}
                          className="px-2 py-0.5 bg-[#ECE9D8] hover:bg-[#F5F4EA] text-[#000000] dark:text-foreground text-[10px] font-bold rounded-sm border border-[#ACA899] cursor-pointer"
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
        <fieldset className="border border-[#ACA899] dark:border-border p-3 bg-[#ECE9D8] dark:bg-card rounded-sm space-y-2">
          <legend className="text-[11px] font-bold text-[#0A246A] dark:text-primary px-1">
            Despacho de Mensaje Directo
          </legend>

          <div className="space-y-2">
            <div>
              <label htmlFor="target-agent-id" className="text-[11px] font-bold text-[#01285F] dark:text-foreground block mb-0.5">
                Agente destinatario:
              </label>
              <input
                id="target-agent-id"
                type="text"
                value={targetId}
                onChange={(e) => setTargetId(e.target.value)}
                placeholder="ej. claude-code, deep, antigravity"
                className="w-full px-2 py-1 bg-white dark:bg-card border border-[#7F9DB9] dark:border-border text-[#000000] dark:text-foreground text-[11px] font-mono rounded-none outline-none"
              />
              {/* Quick Select Badges */}
              <div role="group" aria-label="Selección rápida de agente" className="flex flex-wrap gap-1 mt-1">
                {quickAgents.map((ag) => (
                  <button
                    key={ag}
                    type="button"
                    aria-pressed={targetId === ag}
                    onClick={() => setTargetId(ag)}
                    className={cn(
                      "px-1.5 py-0.5 text-[10px] font-mono font-bold rounded-none border transition-all cursor-pointer",
                      targetId === ag
                        ? "bg-[#0A246A] text-white border-[#0A246A]"
                        : "bg-white dark:bg-muted text-[#444444] dark:text-muted-foreground border-[#ACA899] hover:bg-[#F5F4EA]"
                    )}
                  >
                    @{ag}
                  </button>
                ))}
              </div>
            </div>

            <div>
              <label htmlFor="substrate-payload-msg" className="text-[11px] font-bold text-[#01285F] dark:text-foreground block mb-0.5">
                Carga útil / Mensaje directo:
              </label>
              <textarea
                id="substrate-payload-msg"
                value={message}
                onChange={(e) => setMessage(e.target.value)}
                placeholder="Escriba el mensaje directo para el nodo seleccionado..."
                rows={4}
                className="w-full p-2 bg-white dark:bg-card border-t border-l border-[#808080] border-b border-r border-white dark:border-border text-[#000000] dark:text-foreground text-[11px] font-sans rounded-none outline-none resize-none"
              />
            </div>

            <button
              type="button"
              onClick={sendMessage}
              disabled={!targetId || !message}
              className="w-full flex items-center justify-center gap-1.5 py-1.5 bg-[#0A246A] hover:bg-[#081C52] text-white text-[11px] font-bold rounded-sm border border-[#0A246A] transition-all disabled:opacity-40 cursor-pointer focus-visible:outline-1 focus-visible:outline-[#0A246A]"
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
