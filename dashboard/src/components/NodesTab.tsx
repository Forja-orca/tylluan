import React, { useState, useEffect, useCallback } from 'react';
import { RefreshCw, Send, Radio, Users, Cpu, ShieldCheck, Inbox, MessageSquare } from 'lucide-react';
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

export function NodesTab({ bridge: _bridgeProp, notify }: { bridge: unknown; notify: (msg: string, t?: any) => void }) {
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
      notify(`Error fetching nodes: ${e}`, 'error');
    } finally {
      setLoading(false);
    }
  }, [effectiveBridge, notify]);

  useEffect(() => { 
    fetchNodes(); 
  }, [fetchNodes]);

  // Polling via centralized coordinator (replaces 1 scattered setInterval)
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
        notify(`Message delivered to ${targetId}`, 'info');
        setMessage('');
      } else {
        notify(`Error delivering message: ${JSON.stringify(data)}`, 'error');
      }
    } catch (e) {
      notify(`Error sending message: ${e}`, 'error');
    }
  };

  const registerNode = async () => {
    if (!effectiveBridge) return;
    try {
      const data = await effectiveBridge.fetchRaw('/api/v1/nodes/dashboard/register', { method: 'POST' });
      notify(`Node registered: ${data.status || 'ok'}`, 'info');
      fetchNodes();
    } catch (e) {
      notify(`Error registering node: ${e}`, 'error');
    }
  };

  const quickAgents = ['claude-code', 'deep', 'antigravity', 'qwen'];

  return (
    <div className="flex-1 min-h-0 p-6 flex flex-col space-y-6 font-sans">
      {/* Header */}
      <div className="p-5 bg-[#0B0F17]/90 border border-slate-800/80 rounded-2xl flex flex-col md:flex-row items-start md:items-center justify-between gap-4">
        <div>
          <div className="flex items-center gap-2 font-mono">
            <span className="px-2 py-0.5 text-[9px] font-bold tracking-wider uppercase bg-[#00F5D4]/10 text-[#00F5D4] border border-[#00F5D4]/30 rounded">
              P2P Topology
            </span>
            <span className="px-2 py-0.5 text-[9px] font-bold tracking-wider uppercase bg-emerald-500/10 text-emerald-400 border border-emerald-500/30 rounded">
              Blackboard Active
            </span>
          </div>
          <h2 className="text-xl font-bold tracking-tight text-slate-100 mt-2 flex items-center gap-2 font-mono">
            <Radio className="w-5 h-5 text-[#00F5D4]" />
            Agent Substrate Nodes &amp; Direct Messaging
          </h2>
          <p className="text-xs text-slate-400 mt-0.5">
            Registered fleet agents, inbox queues, and inter-agent communication channels.
          </p>
        </div>

        <div className="flex items-center gap-2">
          <button 
            onClick={fetchNodes} 
            className="flex items-center gap-1.5 px-3.5 py-2 bg-slate-900 border border-slate-700 hover:border-[#00F5D4]/50 text-xs text-slate-300 font-mono font-medium rounded-xl transition-all"
          >
            <RefreshCw className={cn('w-3.5 h-3.5 text-[#00F5D4]', loading && 'animate-spin')} /> 
            Sync Nodes
          </button>
          <button 
            onClick={registerNode} 
            className="flex items-center gap-1.5 px-3.5 py-2 bg-[#00F5D4]/10 hover:bg-[#00F5D4]/20 border border-[#00F5D4]/40 text-[#00F5D4] text-xs font-mono font-bold rounded-xl transition-all"
          >
            <Radio className="w-3.5 h-3.5" /> 
            Register Dashboard Node
          </button>
        </div>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        {/* Active Nodes List */}
        <div className="bg-[#0B0F17]/90 rounded-2xl border border-slate-800/80 p-5 space-y-4 font-mono">
          <div className="flex items-center justify-between">
            <h3 className="text-xs font-bold text-slate-400 uppercase tracking-widest flex items-center gap-2">
              <Users className="w-4 h-4 text-[#00F5D4]" /> 
              Connected Fleet Nodes ({nodes.length})
            </h3>
            <span className="text-[10px] text-slate-500">Live Agent Registry</span>
          </div>

          {loading ? (
            <div className="text-center py-8 text-xs text-slate-500 animate-pulse">Scanning agent substrate nodes...</div>
          ) : nodes.length === 0 ? (
            <div className="text-center py-10 text-slate-600 space-y-2 border border-dashed border-slate-800 rounded-xl">
              <Radio className="w-8 h-8 mx-auto text-slate-700" />
              <p className="text-xs font-bold text-slate-400">No active nodes registered</p>
              <p className="text-[11px] text-slate-500">Register a node via tylluan_do or click 'Register Dashboard Node' above.</p>
            </div>
          ) : (
            <div className="space-y-2.5">
              {nodes.map((n) => (
                <div key={n.agent_id} className="flex items-center justify-between bg-slate-950 border border-slate-850 rounded-xl p-3.5 hover:border-slate-700 transition-all">
                  <div className="space-y-1">
                    <div className="flex items-center gap-2">
                      <Cpu className="w-3.5 h-3.5 text-[#00F5D4]" />
                      <span className="text-sm font-bold text-slate-100">{n.agent_id}</span>
                    </div>
                    <div className="flex items-center gap-3 text-[10px] text-slate-500">
                      <span>Rules: <strong className="text-slate-300">{n.rules}</strong></span>
                      <span>Registered: <strong className="text-slate-300">{new Date(n.registered_at * 1000).toLocaleTimeString()}</strong></span>
                    </div>
                  </div>

                  <div className="flex items-center gap-3">
                    <div className="flex items-center gap-1.5 px-2.5 py-1 bg-slate-900 border border-slate-800 rounded-lg text-xs">
                      <Inbox className="w-3 h-3 text-amber-400" />
                      <span className="text-slate-300 font-bold">{n.inbox_pending}</span>
                      <span className="text-[9px] text-slate-500">pending</span>
                    </div>
                    <button
                      onClick={() => setTargetId(n.agent_id)}
                      className="px-2.5 py-1 bg-[#00F5D4]/10 hover:bg-[#00F5D4]/20 border border-[#00F5D4]/30 text-[#00F5D4] text-xs font-bold rounded-lg transition-all"
                    >
                      Target
                    </button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Direct Messaging Dispatcher */}
        <div className="bg-[#0B0F17]/90 rounded-2xl border border-slate-800/80 p-5 space-y-4 font-mono">
          <div className="flex items-center justify-between">
            <h3 className="text-xs font-bold text-slate-400 uppercase tracking-widest flex items-center gap-2">
              <MessageSquare className="w-4 h-4 text-[#00F5D4]" /> 
              Direct Node Dispatcher
            </h3>
            <span className="text-[10px] text-slate-500">Substrate Message Bus</span>
          </div>

          <div className="space-y-4">
            <div className="space-y-1.5">
              <label className="text-xs text-slate-400 font-bold">Target Agent ID:</label>
              <div className="flex gap-2">
                <input
                  type="text"
                  value={targetId}
                  onChange={(e) => setTargetId(e.target.value)}
                  placeholder="e.g. claude-code, deep, antigravity"
                  className="flex-1 px-3.5 py-2 bg-slate-950 border border-slate-800 focus:border-[#00F5D4]/60 text-slate-100 text-xs rounded-xl outline-none"
                />
              </div>
              {/* Quick Select Badges */}
              <div className="flex flex-wrap gap-1.5 pt-1">
                {quickAgents.map((ag) => (
                  <button
                    key={ag}
                    onClick={() => setTargetId(ag)}
                    className={cn(
                      "px-2 py-0.5 text-[10px] font-bold rounded border transition-all",
                      targetId === ag
                        ? "bg-[#00F5D4]/20 border-[#00F5D4]/50 text-[#00F5D4]"
                        : "bg-slate-950 border-slate-800 text-slate-400 hover:text-slate-200"
                    )}
                  >
                    @{ag}
                  </button>
                ))}
              </div>
            </div>

            <div className="space-y-1.5">
              <label className="text-xs text-slate-400 font-bold">Payload Message:</label>
              <textarea
                value={message}
                onChange={(e) => setMessage(e.target.value)}
                placeholder="Enter direct substrate payload for targeted agent node..."
                rows={4}
                className="w-full px-3.5 py-2.5 bg-slate-950 border border-slate-800 focus:border-[#00F5D4]/60 text-slate-100 text-xs rounded-xl outline-none placeholder:text-slate-600 resize-none"
              />
            </div>

            <button
              onClick={sendMessage}
              disabled={!targetId || !message}
              className="w-full flex items-center justify-center gap-2 py-2.5 bg-[#00F5D4]/10 hover:bg-[#00F5D4]/20 border border-[#00F5D4]/40 text-[#00F5D4] text-xs font-bold rounded-xl transition-all disabled:opacity-40"
            >
              <Send className="w-3.5 h-3.5" />
              <span>Dispatch Substrate Message</span>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
