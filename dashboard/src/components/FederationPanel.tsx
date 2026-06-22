import React, { useState, useEffect } from 'react';
import { 
  Network, 
  Plus, 
  Trash2, 
  RefreshCw, 
  Share2, 
  Lock, 
  Database,
  Key,
  ShieldCheck,
  ArrowRightLeft
} from 'lucide-react';
import { NexusBridge } from '../lib/nexus-bridge';
import { cn } from '../lib/utils';

interface PeerInfo {
  name: string;
  url: string;
  token?: string;
  last_sync?: string;
}

interface SilvaNode {
  id: string;
  node_type?: string;
  content?: string;
  shareable?: boolean;
}

interface FederationPanelProps {
  bridge: NexusBridge | null;
  notify: (msg: string, type?: 'info' | 'error') => void;
}

export function FederationPanel({ bridge, notify }: FederationPanelProps) {
  const [peers, setPeers] = useState<PeerInfo[]>([]);
  const [nodes, setNodes] = useState<SilvaNode[]>([]);
  
  // Loaders
  const [peersLoading, setPeersLoading] = useState(true);
  const [nodesLoading, setNodesLoading] = useState(true);
  const [syncingPeer, setSyncingPeer] = useState<string | null>(null);
  const [togglingNodeId, setTogglingNodeId] = useState<string | null>(null);
  
  // Modals
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [peerName, setPeerName] = useState('');
  const [peerUrl, setPeerUrl] = useState('');
  const [peerToken, setPeerToken] = useState('');
  const [submittingPeer, setSubmittingPeer] = useState(false);
  
  // Delete confirm state
  const [confirmDeletePeer, setConfirmDeletePeer] = useState<string | null>(null);

  const fetchPeers = async (silent = false) => {
    if (!bridge) return;
    if (!silent) setPeersLoading(true);
    try {
      const data = await bridge.listFederationPeers();
      setPeers(Array.isArray(data) ? data : []);
    } catch (err) {
      console.error('Failed to list federation peers:', err);
    } finally {
      if (!silent) setPeersLoading(false);
    }
  };

  const fetchNodes = async (silent = false) => {
    if (!bridge) return;
    if (!silent) setNodesLoading(true);
    try {
      const data = await bridge.fetchRaw('/api/v1/silva/graph?limit=100');
      if (data && Array.isArray(data.nodes)) {
        setNodes(data.nodes);
      } else {
        setNodes([]);
      }
    } catch (err) {
      console.error('Failed to list silva nodes for sharing:', err);
    } finally {
      if (!silent) setNodesLoading(false);
    }
  };

  const handleRefreshAll = () => {
    fetchPeers();
    fetchNodes();
  };

  useEffect(() => {
    handleRefreshAll();
    const interval = setInterval(() => {
      fetchPeers(true);
      fetchNodes(true);
    }, 30000);
    return () => clearInterval(interval);
  }, [bridge]);

  const handleAddPeer = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!bridge) return;
    if (!peerName.trim() || !peerUrl.trim() || !peerToken.trim()) {
      notify('All peer details (Name, URL, Token) are required.', 'error');
      return;
    }

    setSubmittingPeer(true);
    try {
      await bridge.addFederationPeer({
        name: peerName.trim(),
        url: peerUrl.trim(),
        token: peerToken.trim()
      });
      notify(`Successfully added federation peer: ${peerName}`, 'info');
      
      // Reset & close
      setPeerName('');
      setPeerUrl('');
      setPeerToken('');
      setIsModalOpen(false);
      
      fetchPeers();
    } catch (err: any) {
      notify(err.message || 'Failed to register peer node.', 'error');
    } finally {
      setSubmittingPeer(false);
    }
  };

  const handleDeletePeer = async (name: string) => {
    if (!bridge) return;
    try {
      await bridge.removeFederationPeer(name);
      notify(`Removed peer node: ${name}`, 'info');
      setConfirmDeletePeer(null);
      fetchPeers();
    } catch (err: any) {
      notify(err.message || 'Failed to remove peer node.', 'error');
    }
  };

  const handleSyncPeer = async (name: string) => {
    if (!bridge) return;
    setSyncingPeer(name);
    try {
      const result = await bridge.federationSync(name);
      notify(`Sync completed with ${name}. Synced items: ${result.synced ?? 0}`, 'info');
      fetchPeers(true);
    } catch (err: any) {
      notify(err.message || `Federation sync failed with ${name}`, 'error');
    } finally {
      setSyncingPeer(null);
    }
  };

  const handleToggleShareable = async (nodeId: string, currentShareable: boolean) => {
    if (!bridge) return;
    setTogglingNodeId(nodeId);
    const targetState = !currentShareable;
    try {
      await bridge.setSilvaShareable(nodeId, targetState);
      
      // Optimistically update local nodes list
      setNodes(prev => prev.map(n => n.id === nodeId ? { ...n, shareable: targetState } : n));
      notify(`Updated node ${nodeId} to ${targetState ? 'Shareable' : 'Private'}`, 'info');
    } catch (err: any) {
      notify(err.message || 'Failed to update shareable flag.', 'error');
    } finally {
      setTogglingNodeId(null);
    }
  };

  const formatLastSync = (dateStr?: string) => {
    if (!dateStr) return 'Never Synced';
    try {
      const d = new Date(dateStr);
      if (isNaN(d.getTime())) return dateStr;
      return d.toLocaleString();
    } catch {
      return dateStr;
    }
  };

  const getNodeTypeColor = (type?: string) => {
    const t = (type || 'agnostic').toLowerCase();
    if (t === 'episode') return 'bg-blue-500/15 text-blue-400 border-blue-500/25';
    if (t === 'document') return 'bg-violet-500/15 text-violet-400 border-violet-500/25';
    if (t === 'system') return 'bg-amber-500/15 text-amber-400 border-amber-500/25';
    return 'bg-slate-700/30 text-slate-400 border-slate-700/50';
  };

  return (
    <div className="space-y-8">
      {/* Header Panel */}
      <div className="flex items-center justify-between gap-4 flex-wrap">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 bg-emerald-500/10 border border-emerald-500/20 rounded-xl flex items-center justify-center">
            <Network className="w-5 h-5 text-emerald-400" />
          </div>
          <div>
            <h2 className="text-lg font-bold text-white tracking-tight uppercase">Cognitive Federation Hub</h2>
            <p className="text-xs text-slate-500 font-mono">Synchronize shareable knowledge nodes across autonomous peer nodes</p>
          </div>
        </div>

        <div className="flex items-center gap-3">
          <button
            onClick={handleRefreshAll}
            disabled={peersLoading || nodesLoading}
            className="p-2.5 rounded-xl border border-slate-800 hover:border-slate-700 bg-slate-900/50 hover:bg-slate-900 text-slate-400 hover:text-slate-200 transition-colors"
            title="Refresh tables"
          >
            <RefreshCw className={cn("w-4 h-4", (peersLoading || nodesLoading) && "animate-spin")} />
          </button>

          <button
            onClick={() => setIsModalOpen(true)}
            className="flex items-center gap-2 px-4 py-2 bg-emerald-500 hover:bg-emerald-600 text-slate-950 rounded-xl text-xs font-bold transition-all shadow-lg active:scale-95"
          >
            <Plus className="w-4 h-4" /> Add Peer Node
          </button>
        </div>
      </div>

      {/* Grid: Peers Section */}
      <div className="space-y-4">
        <div className="flex items-center justify-between border-b border-slate-850 pb-2">
          <h3 className="text-sm font-bold text-slate-300 uppercase font-mono tracking-wider flex items-center gap-2">
            <ArrowRightLeft className="w-4 h-4 text-emerald-400" />
            Federated Peer Connections ({peers.length})
          </h3>
        </div>

        {peersLoading && peers.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-12 text-slate-600">
            <RefreshCw className="w-6 h-6 animate-spin text-emerald-500/40 mb-3" />
            <p className="text-xs font-mono">Querying federation peers...</p>
          </div>
        ) : peers.length === 0 ? (
          <div className="p-8 border border-dashed border-slate-800 bg-slate-900/10 rounded-2xl text-center">
            <p className="text-xs text-slate-500 font-mono">No peer nodes configured yet.</p>
          </div>
        ) : (
          <div className="border border-slate-800/80 bg-slate-900/30 rounded-2xl overflow-hidden backdrop-blur-md">
            <table className="w-full text-left border-collapse">
              <thead>
                <tr className="border-b border-slate-800/80 text-[10px] uppercase tracking-wider text-slate-500 font-mono font-bold bg-slate-950/40">
                  <th className="py-3 px-5">Peer Name</th>
                  <th className="py-3 px-5">Target URL</th>
                  <th className="py-3 px-5">Last Synced</th>
                  <th className="py-3 px-5 text-right">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-800/40 text-sm">
                {peers.map((peer) => (
                  <tr key={peer.name} className="hover:bg-slate-900/25 transition-colors">
                    <td className="py-4 px-5 font-mono font-bold text-slate-200">
                      {peer.name}
                    </td>
                    <td className="py-4 px-5 font-mono text-xs text-slate-400 truncate max-w-[280px]">
                      {peer.url}
                    </td>
                    <td className="py-4 px-5 text-xs text-slate-400 font-mono">
                      {formatLastSync(peer.last_sync)}
                    </td>
                    <td className="py-4 px-5 text-right">
                      <div className="flex items-center justify-end gap-2">
                        <button
                          onClick={() => handleSyncPeer(peer.name)}
                          disabled={syncingPeer !== null}
                          className="flex items-center gap-1.5 px-3 py-1.5 bg-emerald-500/10 hover:bg-emerald-500/20 text-emerald-400 border border-emerald-500/20 rounded-xl text-xs font-bold transition-all disabled:opacity-50"
                        >
                          {syncingPeer === peer.name ? (
                            <RefreshCw className="w-3.5 h-3.5 animate-spin" />
                          ) : (
                            <Share2 className="w-3.5 h-3.5" />
                          )}
                          Sync Now
                        </button>
                        
                        {confirmDeletePeer === peer.name ? (
                          <div className="flex items-center gap-1 bg-slate-900/80 p-0.5 rounded-lg border border-slate-800">
                            <button
                              onClick={() => handleDeletePeer(peer.name)}
                              className="px-2 py-1 bg-red-500 text-slate-950 font-bold rounded text-[10px] uppercase"
                            >
                              Confirm
                            </button>
                            <button
                              onClick={() => setConfirmDeletePeer(null)}
                              className="px-2 py-1 text-slate-400 font-bold rounded text-[10px] uppercase"
                            >
                              Cancel
                            </button>
                          </div>
                        ) : (
                          <button
                            onClick={() => setConfirmDeletePeer(peer.name)}
                            disabled={syncingPeer !== null}
                            className="p-2 text-slate-500 hover:text-red-400 bg-slate-850 hover:bg-red-500/10 border border-slate-800 rounded-xl transition-all"
                            title="Remove peer node"
                          >
                            <Trash2 className="w-3.5 h-3.5" />
                          </button>
                        )}
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* Grid: SilvaDB Shareable Nodes Section */}
      <div className="space-y-4">
        <div className="flex items-center justify-between border-b border-slate-850 pb-2">
          <div>
            <h3 className="text-sm font-bold text-slate-300 uppercase font-mono tracking-wider flex items-center gap-2">
              <Database className="w-4 h-4 text-violet-400" />
              SilvaDB Shareable Nodes Control
            </h3>
            <p className="text-[10px] text-slate-500 font-mono mt-0.5">Toggle sharing flags. Only flagged memories are federated.</p>
          </div>
        </div>

        {nodesLoading && nodes.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-12 text-slate-600">
            <RefreshCw className="w-6 h-6 animate-spin text-violet-500/40 mb-3" />
            <p className="text-xs font-mono">Loading SilvaDB nodes...</p>
          </div>
        ) : nodes.length === 0 ? (
          <div className="p-8 border border-dashed border-slate-800 bg-slate-900/10 rounded-2xl text-center">
            <p className="text-xs text-slate-500 font-mono">No nodes found in SilvaDB.</p>
          </div>
        ) : (
          <div className="border border-slate-800/80 bg-slate-900/30 rounded-2xl overflow-hidden backdrop-blur-md">
            <div className="overflow-x-auto max-h-[480px]">
              <table className="w-full text-left border-collapse">
                <thead>
                  <tr className="border-b border-slate-800/80 text-[10px] uppercase tracking-wider text-slate-500 font-mono font-bold bg-slate-950/40 sticky top-0 z-10 backdrop-blur">
                    <th className="py-3 px-5">Node ID / Type</th>
                    <th className="py-3 px-5">Memory Content</th>
                    <th className="py-3 px-5 text-center">Sharing Status</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-800/40 text-sm">
                  {nodes.map((node) => {
                    const isShareable = !!node.shareable;
                    return (
                      <tr key={node.id} className={cn(
                        "hover:bg-slate-900/25 transition-colors",
                        isShareable && "bg-emerald-500/[0.01]"
                      )}>
                        <td className="py-4 px-5">
                          <div className="space-y-1">
                            <span className="block font-mono text-[11px] font-bold text-slate-300">
                              {node.id.length > 20 ? `${node.id.substring(0, 20)}...` : node.id}
                            </span>
                            <span className={cn(
                              "inline-block text-[8px] px-1.5 py-0.5 rounded font-bold border uppercase tracking-tighter",
                              getNodeTypeColor(node.node_type)
                            )}>
                              {node.node_type || 'agnostic'}
                            </span>
                          </div>
                        </td>
                        <td className="py-4 px-5 text-xs text-slate-400 font-sans max-w-[480px] break-words line-clamp-3">
                          {node.content || <span className="italic text-slate-600">No content snippet</span>}
                        </td>
                        <td className="py-4 px-5 text-center">
                          <div className="flex items-center justify-center">
                            <button
                              type="button"
                              onClick={() => handleToggleShareable(node.id, isShareable)}
                              disabled={togglingNodeId === node.id}
                              className={cn(
                                "flex items-center gap-2 px-3 py-1.5 rounded-xl text-[10px] font-bold border transition-all active:scale-95 disabled:opacity-50",
                                isShareable
                                  ? "bg-emerald-500/10 text-emerald-400 border-emerald-500/20"
                                  : "bg-slate-850 text-slate-500 border-slate-800 hover:border-slate-700"
                              )}
                            >
                              {togglingNodeId === node.id ? (
                                <RefreshCw className="w-3.5 h-3.5 animate-spin" />
                              ) : isShareable ? (
                                <ShieldCheck className="w-3.5 h-3.5 text-emerald-400 animate-pulse" />
                              ) : (
                                <Lock className="w-3.5 h-3.5 text-slate-500" />
                              )}
                              {isShareable ? 'Federated / Shareable' : 'Private'}
                            </button>
                          </div>
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

      {/* Add Peer Modal */}
      {isModalOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-slate-950/80 backdrop-blur-sm animate-in fade-in duration-200">
          <div 
            className="w-full max-w-md bg-slate-900 border border-slate-850 rounded-2xl shadow-2xl p-6 relative flex flex-col space-y-4 animate-in zoom-in-95 duration-200"
            onClick={(e) => e.stopPropagation()}
          >
            {/* Modal Header */}
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <Network className="w-5 h-5 text-emerald-400" />
                <h3 className="text-md font-bold text-white uppercase tracking-tight">Register Federated Peer</h3>
              </div>
              <button
                onClick={() => setIsModalOpen(false)}
                className="text-slate-500 hover:text-slate-300 font-mono text-sm px-2 py-1 rounded hover:bg-slate-800"
              >
                ✕
              </button>
            </div>

            {/* Modal Form */}
            <form onSubmit={handleAddPeer} className="space-y-4">
              {/* Peer Alias */}
              <div className="space-y-1.5">
                <label className="block text-[10px] font-bold text-slate-500 uppercase tracking-wider font-mono">Peer Alias</label>
                <input
                  type="text"
                  required
                  placeholder="e.g., node-tokyo"
                  value={peerName}
                  onChange={(e) => setPeerName(e.target.value)}
                  className="w-full px-3.5 py-2 rounded-xl bg-slate-950 border border-slate-800 focus:border-emerald-500 focus:outline-none text-sm text-slate-200 font-mono"
                />
              </div>

              {/* Peer Endpoint URL */}
              <div className="space-y-1.5">
                <label className="block text-[10px] font-bold text-slate-500 uppercase tracking-wider font-mono">Peer Endpoint URL</label>
                <input
                  type="url"
                  required
                  placeholder="http://192.168.0.42:3030"
                  value={peerUrl}
                  onChange={(e) => setPeerUrl(e.target.value)}
                  className="w-full px-3.5 py-2 rounded-xl bg-slate-950 border border-slate-800 focus:border-emerald-500 focus:outline-none text-sm text-slate-200 font-mono"
                />
              </div>

              {/* Peer Connection Token */}
              <div className="space-y-1.5">
                <div className="flex justify-between items-center">
                  <label className="block text-[10px] font-bold text-slate-500 uppercase tracking-wider font-mono flex items-center gap-1">
                    <Key className="w-3 h-3 text-slate-500" /> Secure Connection Token
                  </label>
                </div>
                <input
                  type="password"
                  required
                  placeholder="Paste connection key (e.g. malamadre)"
                  value={peerToken}
                  onChange={(e) => setPeerToken(e.target.value)}
                  className="w-full px-3.5 py-2 rounded-xl bg-slate-950 border border-slate-800 focus:border-emerald-500 focus:outline-none text-sm text-slate-200 font-mono"
                />
              </div>

              {/* Action Buttons */}
              <div className="flex gap-3 justify-end pt-4 border-t border-slate-850">
                <button
                  type="button"
                  onClick={() => setIsModalOpen(false)}
                  className="px-4 py-2 border border-slate-800 hover:border-slate-700 bg-slate-950 hover:bg-slate-900 text-slate-400 hover:text-slate-200 rounded-xl text-xs font-bold transition-colors"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={submittingPeer}
                  className="px-5 py-2 bg-emerald-500 hover:bg-emerald-600 text-slate-950 rounded-xl text-xs font-bold transition-all disabled:opacity-50 flex items-center gap-2"
                >
                  {submittingPeer && <RefreshCw className="w-3 h-3 animate-spin" />}
                  Register Peer
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  );
}
