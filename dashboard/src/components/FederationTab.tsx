import React, { useState, useEffect, useCallback } from 'react';
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
  Globe,
  WifiOff,
  ShieldAlert,
  Power,
  Sliders,
  Clock,
  Filter
} from 'lucide-react';
import { NexusBridge } from '../lib/nexus-bridge';
import { cn } from '../lib/utils';

interface PeerInfo {
  name: string;
  url: string;
  last_sync?: number | string | null;
}

interface PeerStatus {
  online: boolean;
  nodesCount: number | null;
  checking: boolean;
}

interface SilvaNode {
  id: string;
  node_type?: string;
  content?: string;
  shareable?: boolean;
}

interface SharingStatus {
  enabled: boolean;
  node_types: string[];
  min_weight: number;
  min_activity_hours: number;
  shareable_nodes_count: number;
}

interface FederationTabProps {
  bridge: NexusBridge | null;
  notify: (msg: string, type?: 'info' | 'error') => void;
}

export function FederationTab({ bridge, notify }: FederationTabProps) {
  const [peers, setPeers] = useState<PeerInfo[]>([]);
  const [peerStatuses, setPeerStatuses] = useState<Record<string, PeerStatus>>({});
  const [nodes, setNodes] = useState<SilvaNode[]>([]);
  const [sharingStatus, setSharingStatus] = useState<SharingStatus | null>(null);
  const [sharingStatusLoading, setSharingStatusLoading] = useState(true);
  
  // Loaders
  const [peersLoading, setPeersLoading] = useState(true);
  const [nodesLoading, setNodesLoading] = useState(true);
  const [syncing, setSyncing] = useState(false);
  const [togglingNodeId, setTogglingNodeId] = useState<string | null>(null);
  
  // Registration Form State
  const [peerName, setPeerName] = useState('');
  const [peerUrl, setPeerUrl] = useState('');
  const [peerToken, setPeerToken] = useState('');
  const [submittingPeer, setSubmittingPeer] = useState(false);
  
  // Delete confirm state
  const [confirmDeletePeer, setConfirmDeletePeer] = useState<string | null>(null);

  // Helper to check peer health status & get node count
  const checkSinglePeerStatus = useCallback(async (url: string): Promise<{ online: boolean; nodesCount: number | null }> => {
    // Try detailed health first to get nodes count
    try {
      const token = localStorage.getItem('tylluan_token') || '';
      const headers: HeadersInit = {};
      if (token) {
        headers['Authorization'] = `Bearer ${token}`;
      }
      
      const controller = new AbortController();
      const id = setTimeout(() => controller.abort(), 2000); // 2s timeout
      
      const resp = await fetch(`${url.replace(/\/+$/, '')}/api/v1/health/detailed`, { 
        headers,
        signal: controller.signal
      });
      clearTimeout(id);
      
      if (resp.ok) {
        const data = await resp.json();
        return {
          online: true,
          nodesCount: data.components?.silva?.nodes ?? 0
        };
      }
    } catch {
      // Ignore and fallback
    }

    // Fallback to simple health ping
    try {
      const controller = new AbortController();
      const id = setTimeout(() => controller.abort(), 2000);
      
      const resp = await fetch(`${url.replace(/\/+$/, '')}/health`, {
        signal: controller.signal
      });
      clearTimeout(id);
      
      if (resp.ok) {
        return {
          online: true,
          nodesCount: null
        };
      }
    } catch {
      // Ignore
    }

    return {
      online: false,
      nodesCount: null
    };
  }, []);

  const fetchPeers = useCallback(async (silent = false) => {
    if (!bridge) return;
    if (!silent) setPeersLoading(true);
    try {
      const data = await bridge.listFederationPeers();
      const peerList = Array.isArray(data) ? data : [];
      setPeers(peerList);
      
      // Ping statuses asynchronously
      peerList.forEach(async (peer) => {
        setPeerStatuses((prev: Record<string, PeerStatus>) => ({
          ...prev,
          [peer.name]: prev[peer.name] ? { ...prev[peer.name], checking: true } : { online: false, nodesCount: null, checking: true }
        }));
        const status = await checkSinglePeerStatus(peer.url);
        setPeerStatuses((prev: Record<string, PeerStatus>) => ({
          ...prev,
          [peer.name]: { online: status.online, nodesCount: status.nodesCount, checking: false }
        }));
      });
    } catch (err) {
      console.error('Failed to list federation peers:', err);
    } finally {
      if (!silent) setPeersLoading(false);
    }
  }, [bridge, checkSinglePeerStatus]);

  const fetchNodes = useCallback(async (silent = false) => {
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
  }, [bridge]);

  const fetchSharingStatus = useCallback(async (silent = false) => {
    if (!bridge) return;
    if (!silent) setSharingStatusLoading(true);
    try {
      const data = await bridge.fetchRaw('/api/v1/federation/sharing/status');
      setSharingStatus(data);
    } catch (err) {
      console.error('Failed to fetch federation sharing status:', err);
    } finally {
      if (!silent) setSharingStatusLoading(false);
    }
  }, [bridge]);

  const handleToggleSharing = async () => {
    if (!bridge || !sharingStatus) return;
    const isActivating = !sharingStatus.enabled;
    
    if (isActivating) {
      const confirmed = window.confirm(
        "¿Activar sharing federado? Solo hazlo si tienes tokens configurados con peers aprobados."
      );
      if (!confirmed) return;
    }
    
    try {
      const endpoint = isActivating 
        ? '/api/v1/federation/sharing/enable' 
        : '/api/v1/federation/sharing/disable';
      
      const res = await bridge.fetchRaw(endpoint, { method: 'POST' });
      if (res.status === 'ok' || res.sharing_enabled !== undefined) {
        notify(`Federation sharing ${isActivating ? 'enabled' : 'disabled'} successfully.`, 'info');
        setSharingStatus(prev => prev ? { ...prev, enabled: isActivating } : null);
        fetchSharingStatus(true);
      } else {
        notify(`Failed to toggle sharing: ${res.message || 'Unknown error'}`, 'error');
      }
    } catch (err: any) {
      notify(err.message || 'Error toggling sharing state', 'error');
    }
  };

  const handleRefreshAll = useCallback(() => {
    fetchPeers();
    fetchNodes();
    fetchSharingStatus();
  }, [fetchPeers, fetchNodes, fetchSharingStatus]);

  useEffect(() => {
    handleRefreshAll();
    // Poll every 5 seconds as requested by the user
    const interval = setInterval(() => {
      fetchPeers(true);
      fetchNodes(true);
    }, 5000);
    return () => clearInterval(interval);
  }, [bridge, handleRefreshAll, fetchPeers, fetchNodes]);

  useEffect(() => {
    fetchSharingStatus();
    // Poll sharing status every 10 seconds (M6 specification)
    const interval = setInterval(() => {
      fetchSharingStatus(true);
    }, 10000);
    return () => clearInterval(interval);
  }, [bridge, fetchSharingStatus]);

  const handleAddPeer = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!bridge) return;
    if (!peerName.trim() || !peerUrl.trim() || !peerToken.trim()) {
      notify('All fields (Name, URL, Token) are required.', 'error');
      return;
    }

    setSubmittingPeer(true);
    try {
      await bridge.addFederationPeer({
        name: peerName.trim(),
        url: peerUrl.trim(),
        token: peerToken.trim()
      });
      
      // Store token locally in frontend to display mDNS auto badge if applicable
      localStorage.setItem(`peer_token_${peerName.trim()}`, peerToken.trim());
      
      notify(`Successfully registered peer: ${peerName}`, 'info');
      
      // Reset form
      setPeerName('');
      setPeerUrl('');
      setPeerToken('');
      
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
      localStorage.removeItem(`peer_token_${name}`);
      notify(`Removed peer node: ${name}`, 'info');
      setConfirmDeletePeer(null);
      fetchPeers();
    } catch (err: any) {
      notify(err.message || 'Failed to remove peer node.', 'error');
    }
  };

  const handleSyncAll = async () => {
    if (!bridge) return;
    setSyncing(true);
    try {
      // POST /api/v1/federation/sync triggers synchronization and returns total nodes_synced
      const result = await bridge.fetchRaw('/api/v1/federation/sync', { method: 'POST' });
      const nodesSynced = result.nodes_synced ?? result.synced ?? 0;
      notify(`Federation sync complete. Synced ${nodesSynced} knowledge nodes!`, 'info');
      fetchPeers(true);
    } catch (err: any) {
      notify(err.message || 'Federation sync failed', 'error');
    } finally {
      setSyncing(false);
    }
  };

  const handleToggleShareable = async (nodeId: string, currentShareable: boolean) => {
    if (!bridge) return;
    setTogglingNodeId(nodeId);
    const targetState = !currentShareable;
    try {
      await bridge.setSilvaShareable(nodeId, targetState);
      setNodes((prev: SilvaNode[]) => prev.map((n: SilvaNode) => n.id === nodeId ? { ...n, shareable: targetState } : n));
      notify(`Node ${nodeId.substring(0, 8)} set to ${targetState ? 'Shareable' : 'Private'}`, 'info');
    } catch (err: any) {
      notify(err.message || 'Failed to update shareable flag.', 'error');
    } finally {
      setTogglingNodeId(null);
    }
  };

  const formatLastSync = (timestamp?: number | string | null) => {
    if (!timestamp) return 'Never';
    try {
      const val = typeof timestamp === 'string' ? parseInt(timestamp) : timestamp;
      if (isNaN(val) || val <= 0) return 'Never';
      // timestamp is typically in seconds from UNIX epoch in backend
      const d = new Date(val * 1000);
      return d.toLocaleTimeString();
    } catch {
      return 'Never';
    }
  };

  const getNodeTypeColor = (type?: string) => {
    const t = (type || 'agnostic').toLowerCase();
    if (t === 'episode') return 'bg-blue-500/15 text-blue-400 border-blue-500/25';
    if (t === 'document') return 'bg-violet-500/15 text-violet-400 border-violet-500/25';
    if (t === 'system') return 'bg-amber-500/15 text-amber-400 border-amber-500/25';
    return 'bg-slate-700/30 text-slate-400 border-slate-700/50';
  };

  const isPeerMdnsAuto = (peer: PeerInfo) => {
    const localToken = localStorage.getItem(`peer_token_${peer.name}`);
    return localToken === 'mdns-auto' || 
           peer.name.toLowerCase().includes('mdns') || 
           peer.name.toLowerCase().startsWith('mdns-');
  };

  return (
    <div className="space-y-8">
      {/* Header Panel */}
      <div className="flex items-center justify-between gap-4 flex-wrap bg-slate-900/40 p-6 rounded-2xl border border-slate-800/80 backdrop-blur-md">
        <div className="flex items-center gap-3">
          <div className="w-12 h-12 bg-emerald-500/10 border border-emerald-500/20 rounded-xl flex items-center justify-center">
            <Network className="w-6 h-6 text-emerald-400" />
          </div>
          <div>
            <h2 className="text-xl font-bold text-white tracking-tight uppercase">Cognitive Federation Hub</h2>
            <p className="text-xs text-slate-400 font-mono mt-0.5">Synchronize shareable knowledge nodes across autonomous peer nodes</p>
          </div>
        </div>

        <div className="flex items-center gap-3">
          <button
            onClick={handleRefreshAll}
            disabled={peersLoading || nodesLoading}
            className="p-2.5 rounded-xl border border-slate-800 hover:border-slate-700 bg-slate-950/50 hover:bg-slate-900 text-slate-400 hover:text-slate-200 transition-all active:scale-95 disabled:opacity-50"
            title="Refresh details"
          >
            <RefreshCw className={cn("w-4 h-4", (peersLoading || nodesLoading) && "animate-spin")} />
          </button>

          <button
            onClick={handleSyncAll}
            disabled={syncing || peers.length === 0}
            className="flex items-center gap-2 px-5 py-2.5 bg-gradient-to-r from-emerald-500 to-teal-600 hover:from-emerald-400 hover:to-teal-500 text-slate-950 rounded-xl text-xs font-bold transition-all shadow-lg shadow-emerald-500/10 active:scale-95 disabled:opacity-50 disabled:pointer-events-none"
          >
            {syncing ? (
              <RefreshCw className="w-4 h-4 animate-spin text-slate-950" />
            ) : (
              <Share2 className="w-4 h-4 text-slate-950" />
            )}
            Sync Peer Network
          </button>
        </div>
      </div>

      {/* Sharing Policy Panel */}
      {sharingStatus && (
        <div className={cn(
          "bg-slate-900/40 p-6 rounded-2xl border backdrop-blur-md transition-all duration-300",
          sharingStatus.enabled ? "border-green-700/40" : "border-red-900/40"
        )}>
          <div className="flex flex-col md:flex-row items-start md:items-center justify-between gap-6">
            
            {/* Left side: Status badge and info */}
            <div className="space-y-4 flex-1">
              <div className="flex items-center gap-3">
                <div className={cn(
                  "p-2.5 rounded-xl border flex items-center justify-center transition-colors",
                  sharingStatus.enabled 
                    ? "bg-green-500/10 border-green-500/20 text-green-400" 
                    : "bg-red-500/10 border-red-500/20 text-red-400"
                )}>
                  {sharingStatus.enabled ? (
                    <ShieldCheck className="w-5 h-5 text-green-400" />
                  ) : (
                    <ShieldAlert className="w-5 h-5 text-red-400" />
                  )}
                </div>
                <div>
                  <div className="flex items-center gap-2 flex-wrap">
                    <h3 className="text-sm font-bold uppercase tracking-wider font-mono text-slate-300">Sharing Policy</h3>
                    <span className={cn(
                      "px-2.5 py-0.5 rounded-full text-[10px] font-bold uppercase tracking-widest font-mono",
                      sharingStatus.enabled 
                        ? "bg-green-500/10 text-green-400 border border-green-500/25 animate-pulse" 
                        : "bg-red-500/10 text-red-400 border border-red-500/25"
                    )}>
                      {sharingStatus.enabled ? "SHARING ON" : "SHARING OFF (kill-switch activo)"}
                    </span>
                  </div>
                  <p className="text-xs text-slate-400 mt-1">Configure automatic federation rules and access constraints</p>
                </div>
              </div>

              {/* Metrics / Constraints details */}
              <div className="grid grid-cols-2 sm:grid-cols-4 gap-4 pt-2">
                
                {/* Shareable Nodes Count */}
                <div className="bg-slate-950/40 rounded-xl p-3 border border-slate-800/60">
                  <div className="text-[10px] uppercase font-bold tracking-wider text-slate-500 font-mono flex items-center gap-1.5 mb-1">
                    <Database className="w-3.5 h-3.5 text-violet-400" />
                    Shareable Nodes
                  </div>
                  <div className="text-lg font-bold text-slate-200 font-mono">
                    {sharingStatus.shareable_nodes_count}
                  </div>
                </div>

                {/* Min Weight */}
                <div className="bg-slate-950/40 rounded-xl p-3 border border-slate-800/60">
                  <div className="text-[10px] uppercase font-bold tracking-wider text-slate-500 font-mono flex items-center gap-1.5 mb-1">
                    <Sliders className="w-3.5 h-3.5 text-blue-400" />
                    Min Weight
                  </div>
                  <div className="text-lg font-bold text-slate-200 font-mono">
                    {sharingStatus.min_weight}
                  </div>
                </div>

                {/* Min Activity Window */}
                <div className="bg-slate-950/40 rounded-xl p-3 border border-slate-800/60">
                  <div className="text-[10px] uppercase font-bold tracking-wider text-slate-500 font-mono flex items-center gap-1.5 mb-1">
                    <Clock className="w-3.5 h-3.5 text-amber-400" />
                    Min Activity
                  </div>
                  <div className="text-lg font-bold text-slate-200 font-mono">
                    {sharingStatus.min_activity_hours}h
                  </div>
                </div>

                {/* Allowed Types */}
                <div className="bg-slate-950/40 rounded-xl p-3 border border-slate-800/60">
                  <div className="text-[10px] uppercase font-bold tracking-wider text-slate-500 font-mono flex items-center gap-1.5 mb-1">
                    <Filter className="w-3.5 h-3.5 text-cyan-400" />
                    Allowed Types
                  </div>
                  <div className="flex flex-wrap gap-1 mt-1">
                    {sharingStatus.node_types && sharingStatus.node_types.length > 0 ? (
                      sharingStatus.node_types.map((t: string) => (
                        <span key={t} className="px-1.5 py-0.5 rounded text-[8px] font-bold bg-slate-900 border border-slate-800 text-slate-400 uppercase tracking-tighter">
                          {t}
                        </span>
                      ))
                    ) : (
                      <span className="text-[10px] text-slate-600 italic">None</span>
                    )}
                  </div>
                </div>

              </div>
            </div>

            {/* Right side: Prominent Toggle Button */}
            <div className="flex-shrink-0 self-center">
              <button
                onClick={handleToggleSharing}
                className={cn(
                  "flex items-center gap-2 px-6 py-3 rounded-xl text-xs font-bold uppercase tracking-widest transition-all shadow-lg active:scale-95 text-slate-950 font-bold",
                  sharingStatus.enabled
                    ? "bg-red-500 hover:bg-red-600 shadow-red-500/10"
                    : "bg-emerald-500 hover:bg-emerald-600 shadow-emerald-500/10"
                )}
              >
                <Power className="w-4 h-4 text-slate-950" />
                {sharingStatus.enabled ? "Desactivar sharing" : "Activar sharing"}
              </button>
            </div>

          </div>
        </div>
      )}

      {/* Grid Layout: Peers on the left (2/3), Registration on the right (1/3) */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        
        {/* Peers List Column */}
        <div className="lg:col-span-2 space-y-4">
          <div className="flex items-center justify-between border-b border-slate-800/80 pb-2">
            <h3 className="text-sm font-bold text-slate-200 uppercase font-mono tracking-wider flex items-center gap-2">
              <Globe className="w-4 h-4 text-emerald-400" />
              Connected Peer Nodes ({peers.length})
            </h3>
          </div>

          {peersLoading && peers.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-16 text-slate-500 bg-slate-900/10 rounded-2xl border border-slate-850">
              <RefreshCw className="w-8 h-8 animate-spin text-emerald-500/40 mb-3" />
              <p className="text-xs font-mono">Querying network topology...</p>
            </div>
          ) : peers.length === 0 ? (
            <div className="p-12 border border-dashed border-slate-800/80 bg-slate-900/10 rounded-2xl text-center flex flex-col items-center justify-center">
              <WifiOff className="w-8 h-8 text-slate-600 mb-3" />
              <p className="text-xs text-slate-500 font-mono">No peer nodes currently configured.</p>
              <p className="text-[10px] text-slate-600 font-mono mt-1">Register a peer node on the right to begin federation.</p>
            </div>
          ) : (
            <div className="border border-slate-800/80 bg-slate-900/20 rounded-2xl overflow-hidden backdrop-blur-md">
              <div className="overflow-x-auto">
                <table className="w-full text-left border-collapse">
                  <thead>
                    <tr className="border-b border-slate-800/80 text-[10px] uppercase tracking-wider text-slate-400 font-mono font-bold bg-slate-950/60">
                      <th className="py-3.5 px-5">Node Identity</th>
                      <th className="py-3.5 px-5">Endpoint Address</th>
                      <th className="py-3.5 px-4 text-center">Status</th>
                      <th className="py-3.5 px-4 text-center">Synced Nodes</th>
                      <th className="py-3.5 px-5">Last Sync</th>
                      <th className="py-3.5 px-5 text-right">Action</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-slate-850 text-sm">
                    {peers.map((peer: PeerInfo) => {
                      const status = peerStatuses[peer.name] || { online: false, nodesCount: null, checking: true };
                      const isAuto = isPeerMdnsAuto(peer);
                      return (
                        <tr key={peer.name} className="hover:bg-slate-900/30 transition-colors">
                          {/* Peer Name */}
                          <td className="py-4 px-5">
                            <div className="flex flex-col gap-1">
                              <div className="flex items-center gap-2">
                                <span className="font-mono font-bold text-slate-200">
                                  {peer.name}
                                </span>
                                {isAuto && (
                                  <span className="px-2 py-0.5 text-[9px] font-bold text-cyan-400 bg-cyan-950/50 border border-cyan-800/40 rounded-full animate-pulse font-mono uppercase tracking-tighter">
                                    mDNS auto
                                  </span>
                                )}
                              </div>
                            </div>
                          </td>

                          {/* URL */}
                          <td className="py-4 px-5 font-mono text-xs text-slate-400 truncate max-w-[200px]" title={peer.url}>
                            {peer.url}
                          </td>

                          {/* Online Status */}
                          <td className="py-4 px-4 text-center">
                            {status.checking ? (
                              <div className="inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-[10px] font-semibold bg-slate-800/50 text-slate-400 border border-slate-700/30">
                                <RefreshCw className="w-2.5 h-2.5 animate-spin" />
                                Ping
                              </div>
                            ) : status.online ? (
                              <div className="inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-[10px] font-semibold bg-emerald-500/10 text-emerald-400 border border-emerald-500/20">
                                <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-ping" />
                                Online
                              </div>
                            ) : (
                              <div className="inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-[10px] font-semibold bg-red-500/10 text-red-400 border border-red-500/20">
                                <span className="w-1.5 h-1.5 rounded-full bg-red-500" />
                                Offline
                              </div>
                            )}
                          </td>

                          {/* Synced Nodes Count */}
                          <td className="py-4 px-4 text-center font-mono font-bold text-xs">
                            {status.checking ? (
                              <span className="text-slate-600 animate-pulse">...</span>
                            ) : !status.online ? (
                              <span className="text-slate-600">—</span>
                            ) : status.nodesCount !== null ? (
                              <span className="text-slate-300 bg-slate-850 px-2 py-0.5 rounded-md border border-slate-800">
                                {status.nodesCount}
                              </span>
                            ) : (
                              <span className="text-slate-500 italic">unknown</span>
                            )}
                          </td>

                          {/* Last Sync */}
                          <td className="py-4 px-5 text-xs text-slate-400 font-mono">
                            {formatLastSync(peer.last_sync)}
                          </td>

                          {/* Action Buttons */}
                          <td className="py-4 px-5 text-right">
                            {confirmDeletePeer === peer.name ? (
                              <div className="flex items-center justify-end gap-1.5">
                                <button
                                  onClick={() => handleDeletePeer(peer.name)}
                                  className="px-2 py-1 bg-red-500 hover:bg-red-600 text-slate-950 font-bold rounded-lg text-[10px] uppercase transition-all active:scale-95"
                                >
                                  OK
                                </button>
                                <button
                                  onClick={() => setConfirmDeletePeer(null)}
                                  className="px-2 py-1 bg-slate-800 hover:bg-slate-700 text-slate-300 font-bold rounded-lg text-[10px] uppercase transition-all"
                                >
                                  Cancel
                                </button>
                              </div>
                            ) : (
                              <button
                                onClick={() => setConfirmDeletePeer(peer.name)}
                                className="p-2 text-slate-500 hover:text-red-400 bg-slate-950 hover:bg-red-500/10 border border-slate-850 hover:border-red-500/20 rounded-xl transition-all active:scale-95"
                                title="Remove peer node"
                              >
                                <Trash2 className="w-3.5 h-3.5" />
                              </button>
                            )}
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

        {/* Register Manual Peer Form Column */}
        <div className="space-y-4">
          <div className="flex items-center justify-between border-b border-slate-800/80 pb-2">
            <h3 className="text-sm font-bold text-slate-200 uppercase font-mono tracking-wider flex items-center gap-2">
              <Plus className="w-4 h-4 text-emerald-400" />
              Register Peer Node
            </h3>
          </div>

          <div className="bg-slate-900/30 border border-slate-800/80 rounded-2xl p-5 shadow-xl backdrop-blur-md">
            <form onSubmit={handleAddPeer} className="space-y-4">
              {/* Peer Alias */}
              <div className="space-y-1.5">
                <label className="block text-[10px] font-bold text-slate-400 uppercase tracking-wider font-mono">Peer Name / Alias</label>
                <input
                  type="text"
                  required
                  placeholder="e.g., node-tokyo"
                  value={peerName}
                  onChange={(e) => setPeerName(e.target.value)}
                  className="w-full px-3.5 py-2 rounded-xl bg-slate-950 border border-slate-800 focus:border-emerald-500 focus:ring-1 focus:ring-emerald-500 focus:outline-none text-sm text-slate-200 font-mono transition-all placeholder:text-slate-600"
                />
              </div>

              {/* Peer Endpoint URL */}
              <div className="space-y-1.5">
                <label className="block text-[10px] font-bold text-slate-400 uppercase tracking-wider font-mono">Endpoint URL</label>
                <input
                  type="url"
                  required
                  placeholder="e.g., http://localhost:3032"
                  value={peerUrl}
                  onChange={(e) => setPeerUrl(e.target.value)}
                  className="w-full px-3.5 py-2 rounded-xl bg-slate-950 border border-slate-800 focus:border-emerald-500 focus:ring-1 focus:ring-emerald-500 focus:outline-none text-sm text-slate-200 font-mono transition-all placeholder:text-slate-600"
                />
              </div>

              {/* Peer Connection Token */}
              <div className="space-y-1.5">
                <div className="flex justify-between items-center">
                  <label className="block text-[10px] font-bold text-slate-400 uppercase tracking-wider font-mono flex items-center gap-1">
                    <Key className="w-3 h-3 text-slate-500" /> Connection Token
                  </label>
                  <span className="text-[8px] text-slate-500 font-mono">Use "mdns-auto" for discovery badge</span>
                </div>
                <input
                  type="password"
                  required
                  placeholder="Paste security key or 'mdns-auto'"
                  value={peerToken}
                  onChange={(e) => setPeerToken(e.target.value)}
                  className="w-full px-3.5 py-2 rounded-xl bg-slate-950 border border-slate-800 focus:border-emerald-500 focus:ring-1 focus:ring-emerald-500 focus:outline-none text-sm text-slate-200 font-mono transition-all placeholder:text-slate-600"
                />
              </div>

              {/* Submit Button */}
              <button
                type="submit"
                disabled={submittingPeer}
                className="w-full py-2.5 mt-2 bg-slate-800 hover:bg-slate-700 text-emerald-400 border border-emerald-500/20 hover:border-emerald-500/40 rounded-xl text-xs font-bold uppercase tracking-wider transition-all disabled:opacity-50 flex items-center justify-center gap-2 active:scale-[0.98]"
              >
                {submittingPeer ? (
                  <RefreshCw className="w-3.5 h-3.5 animate-spin" />
                ) : (
                  <Plus className="w-3.5 h-3.5" />
                )}
                Register Node
              </button>
            </form>
          </div>
        </div>

      </div>

      {/* Grid: SilvaDB Shareable Nodes Section */}
      <div className="space-y-4">
        <div className="flex items-center justify-between border-b border-slate-850 pb-2">
          <div>
            <h3 className="text-sm font-bold text-slate-200 uppercase font-mono tracking-wider flex items-center gap-2">
              <Database className="w-4 h-4 text-violet-400" />
              SilvaDB Shareable Nodes Control
            </h3>
            <p className="text-[10px] text-slate-500 font-mono mt-0.5">Toggle sharing flags. Only flagged memories are federated across peer connections.</p>
          </div>
        </div>

        {nodesLoading && nodes.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-16 text-slate-500 bg-slate-900/10 rounded-2xl border border-slate-850">
            <RefreshCw className="w-8 h-8 animate-spin text-violet-500/40 mb-3" />
            <p className="text-xs font-mono">Loading SilvaDB nodes...</p>
          </div>
        ) : nodes.length === 0 ? (
          <div className="p-10 border border-dashed border-slate-800/80 bg-slate-900/10 rounded-2xl text-center">
            <p className="text-xs text-slate-500 font-mono">No nodes found in SilvaDB.</p>
          </div>
        ) : (
          <div className="border border-slate-800/80 bg-slate-900/20 rounded-2xl overflow-hidden backdrop-blur-md shadow-xl">
            <div className="overflow-x-auto max-h-[400px]">
              <table className="w-full text-left border-collapse">
                <thead>
                  <tr className="border-b border-slate-800/80 text-[10px] uppercase tracking-wider text-slate-400 font-mono font-bold bg-slate-950/60 sticky top-0 z-10 backdrop-blur">
                    <th className="py-3 px-5">Node ID / Type</th>
                    <th className="py-3 px-5">Memory Content</th>
                    <th className="py-3 px-5 text-center">Sharing Status</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-850 text-sm">
                  {nodes.map((node: SilvaNode) => {
                    const isShareable = !!node.shareable;
                    return (
                      <tr key={node.id} className={cn(
                        "hover:bg-slate-900/30 transition-colors",
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
                        <td className="py-4 px-5 text-xs text-slate-400 font-sans max-w-[480px] break-words line-clamp-2">
                          {node.content || <span className="italic text-slate-650">No content snippet</span>}
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
                                  : "bg-slate-950 text-slate-500 border-slate-850 hover:border-slate-850"
                              )}
                            >
                              {togglingNodeId === node.id ? (
                                <RefreshCw className="w-3.5 h-3.5 animate-spin" />
                              ) : isShareable ? (
                                <ShieldCheck className="w-3.5 h-3.5 text-emerald-400" />
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
    </div>
  );
}
