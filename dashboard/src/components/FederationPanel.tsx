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
  ArrowRightLeft,
  Activity
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

  const [meshPeers, setMeshPeers] = useState<any[]>([]);

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

  const fetchMeshPeers = async () => {
    if (!bridge) return;
    try {
      const data = await bridge.fetchRaw('/api/v1/guilds/peers');
      if (data && Array.isArray(data.peers)) {
        setMeshPeers(data.peers);
      }
    } catch (err) {
      console.error('Failed to list mesh peers:', err);
    }
  };

  const handleRefreshAll = () => {
    fetchPeers();
    fetchNodes();
    fetchMeshPeers();
  };

  useEffect(() => {
    handleRefreshAll();
    const interval = setInterval(() => {
      fetchPeers(true);
      fetchNodes(true);
      fetchMeshPeers();
    }, 15000); // refresh every 15s
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

      {/* P2P Mesh Network Topology Map */}
      <div className="bg-slate-900/40 border border-slate-800/80 rounded-2xl p-4 backdrop-blur-md">
        <div className="flex items-center gap-2 mb-4">
          <Network className="w-4 h-4 text-emerald-400" />
          <h3 className="text-xs font-bold text-slate-300 uppercase font-mono tracking-wider">
            P2P Mesh Network Topology Map
          </h3>
        </div>
        <P2PMeshMap peers={meshPeers} />
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

interface SimNode {
  id: string;
  label: string;
  x: number;
  y: number;
  vx: number;
  vy: number;
  latency: number | null;
  reachable: boolean;
  ram: number;
  gpu: boolean;
  load: number;
  isLocal: boolean;
}

export function P2PMeshMap({ peers }: { peers: any[] }) {
  const canvasRef = React.useRef<HTMLCanvasElement | null>(null);
  const [latencies, setLatencies] = useState<Record<string, { latency: number; reachable: boolean; consecutiveFailures: number }>>({});
  const simNodesRef = React.useRef<SimNode[]>([]);
  const animationRef = React.useRef<number | null>(null);

  // Ping mechanism to measure real latencies
  useEffect(() => {
    if (!peers || peers.length === 0) return;

    const pingPeers = async () => {
      const newLatencies = { ...latencies };

      await Promise.all(peers.map(async (peer) => {
        const url = peer.addr;
        const start = performance.now();
        let success = false;
        let elapsed = 0;

        try {
          const controller = new AbortController();
          const id = setTimeout(() => controller.abort(), 2000); // 2s timeout
          const cleanUrl = url.startsWith('http') ? url : `http://${url}`;
          
          await fetch(`${cleanUrl}/health`, {
            mode: 'no-cors',
            signal: controller.signal
          });
          clearTimeout(id);
          elapsed = performance.now() - start;
          success = true;
        } catch (e) {
          // Failure
        }

        const prev = latencies[peer.node_id] || { latency: 0, reachable: true, consecutiveFailures: 0 };

        if (success) {
          newLatencies[peer.node_id] = {
            latency: Math.round(elapsed),
            reachable: true,
            consecutiveFailures: 0
          };
        } else {
          const failures = prev.consecutiveFailures + 1;
          newLatencies[peer.node_id] = {
            latency: 0,
            reachable: failures < 3, // Connectivity unreachable after 3 consecutive failures
            consecutiveFailures: failures
          };
        }
      }));

      setLatencies(newLatencies);
    };

    pingPeers();
    const interval = setInterval(pingPeers, 10000); // refresh every 10s
    return () => clearInterval(interval);
  }, [peers]);

  // Sync simulation nodes with incoming peer list and measured latencies
  useEffect(() => {
    const localNodeId = 'Local Kernel (you)';
    const existing = new Map(simNodesRef.current.map(n => [n.id, n]));
    const canvas = canvasRef.current;
    const width = canvas ? canvas.width : 800;
    const height = canvas ? canvas.height : 300;

    const nextNodes: SimNode[] = [];

    // Local Node (fixed center)
    const local = existing.get(localNodeId) || {
      id: localNodeId,
      label: 'Local Node',
      x: width / 2,
      y: height / 2,
      vx: 0,
      vy: 0,
      latency: 0,
      reachable: true,
      ram: 0,
      gpu: false,
      load: 0,
      isLocal: true
    };
    // keep it centered
    local.x = width / 2;
    local.y = height / 2;
    nextNodes.push(local);

    // Peer Nodes
    peers.forEach((peer, idx) => {
      const stats = latencies[peer.node_id] || { latency: null, reachable: true };
      
      const node = existing.get(peer.node_id) || {
        id: peer.node_id,
        label: peer.node_id.substring(0, 12),
        // spawn in circle
        x: width / 2 + Math.cos(idx * 2 * Math.PI / peers.length) * 120,
        y: height / 2 + Math.sin(idx * 2 * Math.PI / peers.length) * 120,
        vx: 0,
        vy: 0,
        latency: stats.latency,
        reachable: stats.reachable,
        ram: Math.round((peer.hardware?.ram_mb || 0) / 1024),
        gpu: !!peer.hardware?.has_gpu,
        load: peer.hardware?.load_avg || 0,
        isLocal: false
      };

      // update dynamic properties
      node.latency = stats.latency;
      node.reachable = stats.reachable;
      node.ram = Math.round((peer.hardware?.ram_mb || 0) / 1024);
      node.gpu = !!peer.hardware?.has_gpu;
      node.load = peer.hardware?.load_avg || 0;

      nextNodes.push(node);
    });

    simNodesRef.current = nextNodes;
  }, [peers, latencies]);

  // Main canvas animation loop
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    // Resizing container helper
    const resizeCanvas = () => {
      const rect = canvas.getBoundingClientRect();
      canvas.width = rect.width * window.devicePixelRatio;
      canvas.height = rect.height * window.devicePixelRatio;
      ctx.scale(window.devicePixelRatio, window.devicePixelRatio);
    };
    
    resizeCanvas();
    window.addEventListener('resize', resizeCanvas);

    let frameCount = 0;

    const runFrame = () => {
      frameCount++;
      const w = canvas.width / window.devicePixelRatio;
      const h = canvas.height / window.devicePixelRatio;

      // 1. Physics update
      const nodes = simNodesRef.current;
      const cx = w / 2;
      const cy = h / 2;

      // Force parameters
      const SPRING_K = 0.05;
      const DAMPING = 0.82;

      // Update positions
      nodes.forEach(node => {
        if (node.isLocal) {
          node.x = cx;
          node.y = cy;
          return;
        }

        // Attraction to central local node (spring link)
        // Link length changes based on latency (higher latency -> sits further out)
        const baseLength = 100 + (node.latency || 150) * 0.4;
        const dx = node.x - cx;
        const dy = node.y - cy;
        const dist = Math.sqrt(dx * dx + dy * dy) || 1.0;
        
        // Spring force
        const forceSpring = (dist - baseLength) * SPRING_K;
        node.vx -= (dx / dist) * forceSpring;
        node.vy -= (dy / dist) * forceSpring;

        // Repulsion from other peer nodes
        nodes.forEach(other => {
          if (other.id === node.id) return;
          const odx = node.x - other.x;
          const ody = node.y - other.y;
          const odist = Math.sqrt(odx * odx + ody * ody) || 1.0;
          if (odist < 140) {
            const forceRepulsion = (140 - odist) * 0.15;
            node.vx += (odx / odist) * forceRepulsion;
            node.vy += (ody / odist) * forceRepulsion;
          }
        });

        // Apply velocities & damping
        node.x += node.vx;
        node.y += node.vy;
        node.vx *= DAMPING;
        node.vy *= DAMPING;

        // Viewport bounds
        node.x = Math.max(40, Math.min(w - 40, node.x));
        node.y = Math.max(40, Math.min(h - 40, node.y));
      });

      // 2. Draw canvas frame
      ctx.clearRect(0, 0, w, h);

      // Draw background grid
      ctx.strokeStyle = 'rgba(30, 41, 59, 0.3)';
      ctx.lineWidth = 1;
      const gridSpacing = 30;
      for (let x = 0; x < w; x += gridSpacing) {
        ctx.beginPath();
        ctx.moveTo(x, 0);
        ctx.lineTo(x, h);
        ctx.stroke();
      }
      for (let y = 0; y < h; y += gridSpacing) {
        ctx.beginPath();
        ctx.moveTo(0, y);
        ctx.lineTo(w, y);
        ctx.stroke();
      }

      // Draw Spring links (Connections)
      nodes.forEach(node => {
        if (node.isLocal) return;

        // Color based on connectivity reachable/unreachable
        ctx.strokeStyle = node.reachable 
          ? 'rgba(16, 185, 129, 0.25)' 
          : 'rgba(239, 68, 68, 0.25)';
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.moveTo(cx, cy);
        ctx.lineTo(node.x, node.y);
        ctx.stroke();

        // Pulsing signals along links
        if (node.reachable) {
          ctx.fillStyle = '#10b981';
          const pulseSpeed = 0.02 + (1 / (node.latency || 50)) * 0.2;
          const t = (frameCount * pulseSpeed) % 1;
          const px = cx + (node.x - cx) * t;
          const py = cy + (node.y - cy) * t;
          ctx.beginPath();
          ctx.arc(px, py, 3, 0, 2 * Math.PI);
          ctx.fill();
        }
      });

      // Draw nodes
      nodes.forEach(node => {
        ctx.save();
        
        if (node.isLocal) {
          // Central local node resplendence
          const glow = 15 + Math.sin(frameCount * 0.05) * 5;
          ctx.shadowBlur = glow;
          ctx.shadowColor = '#10b981';
          ctx.fillStyle = '#0f172a';
          ctx.strokeStyle = '#10b981';
          ctx.lineWidth = 3;

          ctx.beginPath();
          ctx.arc(node.x, node.y, 16, 0, 2 * Math.PI);
          ctx.fill();
          ctx.stroke();

          // Label
          ctx.fillStyle = '#ffffff';
          ctx.font = 'bold 10px monospace';
          ctx.textAlign = 'center';
          ctx.fillText('LOCAL NODE', node.x, node.y - 24);
        } else {
          // Peer node
          const color = node.reachable ? '#10b981' : '#ef4444';
          ctx.shadowBlur = 10;
          ctx.shadowColor = color;
          ctx.fillStyle = '#0f172a';
          ctx.strokeStyle = color;
          ctx.lineWidth = 2;

          ctx.beginPath();
          ctx.arc(node.x, node.y, 12, 0, 2 * Math.PI);
          ctx.fill();
          ctx.stroke();

          // Labels
          ctx.fillStyle = '#e2e8f0';
          ctx.font = 'bold 9px monospace';
          ctx.textAlign = 'center';
          ctx.fillText(node.label, node.x, node.y - 20);

          // Sub-stats (Latency, RAM, GPU)
          ctx.font = '8px monospace';
          ctx.fillStyle = '#64748b';
          
          let latencyText = 'Offline';
          if (node.reachable && node.latency !== null) {
            latencyText = `${node.latency}ms`;
          }
          const capsText = `${node.ram}G${node.gpu ? '+GPU' : ''}`;
          ctx.fillText(latencyText, node.x, node.y + 20);
          ctx.fillText(capsText, node.x, node.y + 29);

          // Mini Connectivity Badge
          ctx.fillStyle = color;
          ctx.beginPath();
          ctx.arc(node.x + 8, node.y + 8, 3, 0, 2 * Math.PI);
          ctx.fill();
        }

        ctx.restore();
      });

      animationRef.current = requestAnimationFrame(runFrame);
    };

    runFrame();

    return () => {
      window.removeEventListener('resize', resizeCanvas);
      if (animationRef.current) cancelAnimationFrame(animationRef.current);
    };
  }, []);

  return (
    <div className="relative border border-slate-800 rounded-xl overflow-hidden bg-slate-950/60">
      <canvas 
        ref={canvasRef} 
        className="w-full h-[220px] block cursor-grab active:cursor-grabbing" 
      />
      {peers.length === 0 && (
        <div className="absolute inset-0 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm">
          <div className="text-slate-500 font-mono text-[10px] flex items-center gap-2">
            <Activity className="w-3.5 h-3.5 animate-pulse text-emerald-500" />
            Awaiting mesh peer discovery...
          </div>
        </div>
      )}
    </div>
  );
}
