import React, { useState, useEffect, useCallback } from 'react';
import { RefreshCw, Send, Radio, Users } from 'lucide-react';

interface NodeInfo {
  agent_id: string;
  inbox_pending: number;
  rules: number;
  registered_at: number;
  last_active: number;
}

export function NodesTab({ bridge, notify }: { bridge: unknown; notify: (msg: string, t?: any) => void }) {
  const [nodes, setNodes] = useState<NodeInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [targetId, setTargetId] = useState('');
  const [message, setMessage] = useState('');

  const fetchNodes = useCallback(async () => {
    try {
      const res = await fetch('/api/v1/nodes');
      const data = await res.json();
      setNodes(data.nodes || []);
    } catch (e) {
      notify(`Error fetching nodes: ${e}`, 'error');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { fetchNodes(); const iv = setInterval(fetchNodes, 15000); return () => clearInterval(iv); }, [fetchNodes]);

  const sendMessage = async () => {
    if (!targetId || !message) return;
    try {
      const res = await fetch(`/api/v1/nodes/${encodeURIComponent(targetId)}/send`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ from: 'dashboard', payload: message, msg_type: 'direct' }),
      });
      const data = await res.json();
      if (data.delivered) {
        notify(`Mensaje enviado a ${targetId}`, 'info');
        setMessage('');
      } else {
        notify(`Error: ${JSON.stringify(data)}`, 'error');
      }
    } catch (e) {
      notify(`Error: ${e}`, 'error');
    }
  };

  const registerNode = async () => {
    try {
      const res = await fetch('/api/v1/nodes/dashboard/register', { method: 'POST' });
      const data = await res.json();
      notify(`Nodo registrado: ${data.status || 'ok'}`, 'info');
      fetchNodes();
    } catch (e) {
      notify(`Error: ${e}`, 'error');
    }
  };

  return (
    <div className="flex-1 min-h-0 p-6 flex flex-col space-y-6">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Radio className="w-5 h-5 text-emerald-400" />
          <h2 className="text-xl font-bold text-white">Agent Nodes</h2>
        </div>
        <div className="flex gap-2">
          <button onClick={fetchNodes} className="px-3 py-1.5 text-xs bg-slate-800 text-slate-300 rounded hover:bg-slate-700 flex items-center gap-1">
            <RefreshCw className="w-3 h-3" /> Refresh
          </button>
          <button onClick={registerNode} className="px-3 py-1.5 text-xs bg-emerald-700 text-white rounded hover:bg-emerald-600 flex items-center gap-1">
            <Radio className="w-3 h-3" /> Register
          </button>
        </div>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        <div className="bg-slate-900 rounded-xl border border-slate-800 p-4 space-y-3">
          <h3 className="text-sm font-semibold text-slate-300 flex items-center gap-2">
            <Users className="w-4 h-4 text-emerald-400" /> Nodes ({nodes.length})
          </h3>
          {loading ? (
            <p className="text-xs text-slate-500">Loading...</p>
          ) : nodes.length === 0 ? (
            <div className="text-center py-8 text-slate-600">
              <Radio className="w-8 h-8 mx-auto mb-2 opacity-30" />
              <p className="text-xs">No connected nodes</p>
              <p className="text-xs mt-1">Register a node from tylluan_do with "node register"</p>
            </div>
          ) : (
            <div className="space-y-2">
              {nodes.map((n) => (
                <div key={n.agent_id} className="flex items-center justify-between bg-slate-800/50 rounded-lg p-3">
                  <div>
                    <p className="text-sm font-medium text-white">{n.agent_id}</p>
                    <p className="text-xs text-slate-400">
                      {n.inbox_pending} pending · {n.rules} rules
                    </p>
                  </div>
                  <span className={`text-xs px-2 py-0.5 rounded-full ${n.inbox_pending > 0 ? 'bg-amber-900/50 text-amber-300' : 'bg-emerald-900/50 text-emerald-300'}`}>
                    {n.inbox_pending > 0 ? `${n.inbox_pending} msgs` : 'idle'}
                  </span>
                </div>
              ))}
            </div>
          )}
        </div>

        <div className="bg-slate-900 rounded-xl border border-slate-800 p-4 space-y-3">
          <h3 className="text-sm font-semibold text-slate-300 flex items-center gap-2">
            <Send className="w-4 h-4 text-violet-400" /> Quick Send
          </h3>
          <div className="space-y-3">
            <div>
              <label className="text-xs text-slate-400 mb-1 block">Target Agent</label>
              <select
                value={targetId}
                onChange={(e) => setTargetId(e.target.value)}
                className="w-full bg-slate-800 text-white text-sm rounded-lg px-3 py-2 border border-slate-700"
              >
                <option value="">Select agent...</option>
                {nodes.filter(n => n.agent_id !== 'dashboard').map((n) => (
                  <option key={n.agent_id} value={n.agent_id}>{n.agent_id}</option>
                ))}
              </select>
            </div>
            <div>
              <label className="text-xs text-slate-400 mb-1 block">Message</label>
              <textarea
                value={message}
                onChange={(e) => setMessage(e.target.value)}
                placeholder="Type a message..."
                className="w-full bg-slate-800 text-white text-sm rounded-lg px-3 py-2 border border-slate-700 h-20 resize-none"
              />
            </div>
            <button
              onClick={sendMessage}
              disabled={!targetId || !message}
              className="w-full px-3 py-2 text-sm bg-violet-700 text-white rounded-lg hover:bg-violet-600 disabled:opacity-40 disabled:cursor-not-allowed flex items-center justify-center gap-2"
            >
              <Send className="w-3 h-3" /> Send Message
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
