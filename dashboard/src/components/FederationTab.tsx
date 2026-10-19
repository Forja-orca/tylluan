import React, { useState, useEffect, useCallback, useRef } from 'react';
import { usePolling } from '../hooks/usePolling';
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
import { withTimeout } from '../lib/timeout';

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

export interface MeshPeerEntry {
  node_id: string;
  addr: string;
  capabilities: string[];
  last_seen: number;
}

export interface MeshStatusResponse {
  enabled: boolean;
  peer_count: number;
  buckets: number;
  peers: MeshPeerEntry[];
}

export interface FederationNodeItem {
  id: string;
  node_type?: string;
  content?: string;
  weight?: number;
  shareable?: boolean;
  topic_key?: string | null;
}

export interface FederationNodesResponse {
  source: string;
  count: number;
  nodes: FederationNodeItem[];
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
  const [meshPeers, setMeshPeers] = useState<MeshPeerEntry[]>([]);
  const [meshStatus, setMeshStatus] = useState<MeshStatusResponse | null>(null);
  const [meshError, setMeshError] = useState<string | null>(null);
  const [meshEndpointStatus, setMeshEndpointStatus] = useState<'ready' | 'forbidden' | 'error'>('ready');
  
  const [fedNodes, setFedNodes] = useState<FederationNodeItem[]>([]);
  const [fedNodesCount, setFedNodesCount] = useState<number>(0);
  const [fedNodesSource, setFedNodesSource] = useState<string>('local');
  const [fedNodesError, setFedNodesError] = useState<string | null>(null);
  const [fedNodesEndpointStatus, setFedNodesEndpointStatus] = useState<'ready' | 'forbidden' | 'error'>('ready');
  
  const [sharingStatus, setSharingStatus] = useState<SharingStatus | null>(null);
  
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

  const meshAbortRef = useRef<AbortController | null>(null);
  const nodesAbortRef = useRef<AbortController | null>(null);

  useEffect(() => {
    return () => {
      meshAbortRef.current?.abort();
      nodesAbortRef.current?.abort();
    };
  }, []);

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

  const fetchMeshPeers = useCallback(async (silent = false) => {
    if (!bridge) return;
    if (meshAbortRef.current) {
      meshAbortRef.current.abort();
    }
    const controller = new AbortController();
    meshAbortRef.current = controller;
    if (!silent) setPeersLoading(true);
    setMeshError(null);
    try {
      const data = (await withTimeout(
        bridge.fetchRaw('/api/v1/mesh/peers', { signal: controller.signal }),
        10000,
        'Request timed out after 10s. GET /api/v1/mesh/peers took too long.'
      )) as MeshStatusResponse;
      if (data && Array.isArray(data.peers)) {
        setMeshPeers(data.peers);
        setMeshStatus(data);
      } else {
        setMeshPeers([]);
        setMeshStatus(null);
      }
      setMeshEndpointStatus('ready');
    } catch (err: unknown) {
      if (err instanceof Error && err.name === 'AbortError') {
        setMeshError('Consulta Mesh cancelada por el usuario.');
        setMeshEndpointStatus('error');
        setMeshPeers([]);
        setMeshStatus(null);
        return;
      }
      const msg = err instanceof Error ? err.message : String(err || '');
      if (msg.includes('403') || msg.includes('401') || msg.toLowerCase().includes('forbidden') || msg.toLowerCase().includes('unauthorized')) {
        setMeshEndpointStatus('forbidden');
        setMeshError('Requiere credencial de operador (HTTP 403). Token maestro requerido para inspeccionar DHT routing table.');
      } else if (msg.includes('timed out') || msg.includes('timeout')) {
        setMeshEndpointStatus('error');
        setMeshError('Request timed out after 10s. La consulta a /api/v1/mesh/peers excedió el tiempo límite.');
      } else {
        setMeshEndpointStatus('error');
        setMeshError(msg || 'Error al consultar /api/v1/mesh/peers en :47004');
      }
      setMeshPeers([]);
      setMeshStatus(null);
    } finally {
      if (meshAbortRef.current === controller) {
        meshAbortRef.current = null;
      }
      if (!silent) setPeersLoading(false);
    }
  }, [bridge]);

  const fetchNodes = useCallback(async (silent = false) => {
    if (!bridge) return;
    if (nodesAbortRef.current) {
      nodesAbortRef.current.abort();
    }
    const controller = new AbortController();
    nodesAbortRef.current = controller;
    if (!silent) setNodesLoading(true);
    setFedNodesError(null);
    try {
      const data = (await withTimeout(
        bridge.fetchRaw('/api/v1/federation/nodes', { signal: controller.signal }),
        10000,
        'Request timed out after 10s. GET /api/v1/federation/nodes took too long.'
      )) as FederationNodesResponse;
      if (data && Array.isArray(data.nodes)) {
        setFedNodes(data.nodes);
        setFedNodesCount(typeof data.count === 'number' ? data.count : data.nodes.length);
        setFedNodesSource(data.source || 'local');
      } else {
        setFedNodes([]);
        setFedNodesCount(0);
      }
      setFedNodesEndpointStatus('ready');
    } catch (err: unknown) {
      if (err instanceof Error && err.name === 'AbortError') {
        setFedNodesError('Consulta de nodos cancelada por el usuario.');
        setFedNodesEndpointStatus('error');
        setFedNodes([]);
        setFedNodesCount(0);
        return;
      }
      const msg = err instanceof Error ? err.message : String(err || '');
      if (msg.includes('403') || msg.includes('401') || msg.toLowerCase().includes('forbidden') || msg.toLowerCase().includes('unauthorized')) {
        setFedNodesEndpointStatus('forbidden');
        setFedNodesError('Requiere credencial de operador (HTTP 403). Token maestro requerido para consultar nodos de federación.');
      } else if (msg.includes('timed out') || msg.includes('timeout')) {
        setFedNodesEndpointStatus('error');
        setFedNodesError('Request timed out after 10s. La consulta a /api/v1/federation/nodes excedió el tiempo límite.');
      } else {
        setFedNodesEndpointStatus('error');
        setFedNodesError(msg || 'Error al consultar /api/v1/federation/nodes en :47004');
      }
      setFedNodes([]);
      setFedNodesCount(0);
    } finally {
      if (nodesAbortRef.current === controller) {
        nodesAbortRef.current = null;
      }
      if (!silent) setNodesLoading(false);
    }
  }, [bridge]);

  const fetchSharingStatus = useCallback(async (_silent = false) => {
    if (!bridge) return;
    try {
      const data = await bridge.fetchRaw('/api/v1/federation/sharing/status') as SharingStatus;
      setSharingStatus(data);
    } catch (err) {
      console.error('Failed to fetch federation sharing status:', err);
    }
  }, [bridge]);

  const handleToggleSharing = async () => {
    if (!bridge || !sharingStatus) return;
    const isActivating = !sharingStatus.enabled;
    
    if (isActivating) {
      const confirmed = window.confirm(
        "Enable federation sharing? Only do this if you have tokens configured with approved peers."
      );
      if (!confirmed) return;
    }
    
    try {
      const endpoint = isActivating 
        ? '/api/v1/federation/sharing/enable' 
        : '/api/v1/federation/sharing/disable';
      
      const res = await bridge.fetchRaw(endpoint, { method: 'POST' }) as { status?: string; sharing_enabled?: boolean; message?: string };
      if (res.status === 'ok' || res.sharing_enabled !== undefined) {
        notify(`Federation sharing ${isActivating ? 'enabled' : 'disabled'} successfully.`, 'info');
        setSharingStatus(prev => prev ? { ...prev, enabled: isActivating } : null);
        fetchSharingStatus(true);
      } else {
        notify(`Failed to toggle sharing: ${res.message || 'Unknown error'}`, 'error');
      }
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err || '');
      notify(msg || 'Error toggling sharing state', 'error');
    }
  };

  const handleRefreshAll = useCallback(() => {
    fetchPeers();
    fetchMeshPeers();
    fetchNodes();
    fetchSharingStatus();
  }, [fetchPeers, fetchMeshPeers, fetchNodes, fetchSharingStatus]);

  useEffect(() => {
    handleRefreshAll();
  }, [bridge, handleRefreshAll, fetchPeers, fetchMeshPeers, fetchNodes]);

  // Polling via centralized coordinator (replaces 2 scattered setInterval calls)
  usePolling('fed-peers-nodes', () => { fetchPeers(true); fetchMeshPeers(true); fetchNodes(true); }, { interval: 'quick', enabled: !!bridge });
  usePolling('fed-sharing', () => fetchSharingStatus(true), { interval: 'medium', enabled: !!bridge });

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
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err || '');
      notify(msg || 'Failed to register peer node.', 'error');
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
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err || '');
      notify(msg || 'Failed to remove peer node.', 'error');
    }
  };

  const handleSyncAll = async () => {
    if (!bridge) return;
    setSyncing(true);
    try {
      // POST /api/v1/federation/sync triggers synchronization and returns total nodes_synced
      const result = await bridge.fetchRaw('/api/v1/federation/sync', { method: 'POST' }) as { nodes_synced?: number; synced?: number };
      const nodesSynced = result.nodes_synced ?? result.synced ?? 0;
      notify(`Federation sync complete. Synced ${nodesSynced} knowledge nodes!`, 'info');
      fetchPeers(true);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err || '');
      notify(msg || 'Federation sync failed', 'error');
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
      setFedNodes(prev => prev.map(n => n.id === nodeId ? { ...n, shareable: targetState } : n));
      notify(`Node ${nodeId.substring(0, 8)} set to ${targetState ? 'Shareable' : 'Private'}`, 'info');
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err || '');
      notify(msg || 'Failed to update shareable flag.', 'error');
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

  // WCAG-A pre-flight 2026-08-12 (dark theme, exact token vars):
  //  emerald-400 on emerald-500/10: 8.14:1 (slate-900) / 9.00:1 (slate-950) · emerald-400 on emerald-700/40: 6.31-6.88:1
  //  slate-950 on red-500 5.14:1 · hover red-400 7.07:1 · red-400 on red-500/10-20: 5.32-5.80:1
  return (
    <div className="space-y-8 font-sans">
      {/* Sovereign Substrate Header Panel */}
      <div className="flex items-center justify-between gap-4 flex-wrap bg-slate-900/60 p-6 rounded-lg">
        <div className="flex items-center gap-3">
          <div className="w-12 h-12 bg-amber-500/10 rounded-lg flex items-center justify-center">
            <Network className="w-6 h-6 text-amber-400" />
          </div>
          <div>
            <div className="flex items-center gap-2 mb-1">
              <span className="px-2 py-0.5 text-[11px] font-medium bg-amber-500/10 text-amber-400 rounded-md">
                Noise XK Encrypted
              </span>
              <span className="px-2 py-0.5 text-[11px] font-medium bg-emerald-500/10 text-emerald-400 rounded-md">
                STUN NAT Mesh
              </span>
            </div>
            <h2 className="text-xl font-semibold font-sans text-slate-100 tracking-tight">Cognitive Federation Mesh</h2>
            <p className="text-xs text-slate-400 mt-0.5">ChaCha20-Poly1305 encrypted peer sync &amp; Noise XK Session Pool telemetry</p>
          </div>
        </div>

        <div className="flex items-center gap-3">
          <button
            onClick={handleRefreshAll}
            disabled={peersLoading || nodesLoading}
            className="p-2.5 rounded-lg bg-slate-900/60 hover:bg-slate-800 text-slate-300 transition-all active:scale-95 disabled:opacity-50"
            title="Refresh details"
          >
            <RefreshCw className={cn("w-4 h-4 text-amber-400", (peersLoading || nodesLoading) && "animate-spin")} />
          </button>

          <button
            onClick={handleSyncAll}
            disabled={syncing}
            className="flex items-center gap-2 px-4 py-2.5 rounded-lg text-xs font-medium bg-amber-500/10 hover:bg-amber-500/20 text-amber-400 transition-all disabled:opacity-50"
          >
            <Share2 className={cn("w-4 h-4", syncing && "animate-spin")} />
            <span>{syncing ? 'Syncing Mesh...' : 'Sync Federation Mesh'}</span>
          </button>
        </div>
      </div>

      {/* Sharing Policy Panel */}
      {sharingStatus && (
        <div className={cn(
          "bg-slate-900/40 p-6 rounded-lg backdrop-blur-md transition-all duration-300",
          sharingStatus.enabled ? "border-emerald-700/40" : "border-red-500/40"
        )}>
          <div className="flex flex-col md:flex-row items-start md:items-center justify-between gap-6">
            
            {/* Left side: Status badge and info */}
            <div className="space-y-4 flex-1">
              <div className="flex items-center gap-3">
                <div className={cn(
                  "p-2.5 rounded-xl border flex items-center justify-center transition-colors",
                  sharingStatus.enabled 
                    ? "bg-emerald-500/10 text-emerald-400" 
                    : "bg-red-500/10 text-red-400"
                )}>
                  {sharingStatus.enabled ? (
                    <ShieldCheck className="w-5 h-5 text-emerald-400" />
                  ) : (
                    <ShieldAlert className="w-5 h-5 text-red-400" />
                  )}
                </div>
                <div>
                  <div className="flex items-center gap-2 flex-wrap">
                    <h3 className="text-sm font-medium text-slate-300">Sharing Policy</h3>
                    <span className={cn(
                      "px-2.5 py-0.5 rounded-full text-[11px] font-medium",
                      sharingStatus.enabled 
                        ? "bg-emerald-500/10 text-emerald-400 animate-pulse" 
                        : "bg-red-500/10 text-red-400"
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
                <div className="bg-slate-950/40 rounded-lg p-3">
                  <div className="text-[11px] text-slate-500 font-medium flex items-center gap-1.5 mb-1">
                    <Database className="w-3.5 h-3.5 text-violet-400" />
                    Shareable Nodes
                  </div>
                  <div className="text-lg font-bold text-slate-200 font-mono">
                    {sharingStatus.shareable_nodes_count}
                  </div>
                </div>

                {/* Min Weight */}
                <div className="bg-slate-950/40 rounded-lg p-3">
                  <div className="text-[11px] text-slate-500 font-medium flex items-center gap-1.5 mb-1">
                    <Sliders className="w-3.5 h-3.5 text-blue-400" />
                    Min Weight
                  </div>
                  <div className="text-lg font-bold text-slate-200 font-mono">
                    {sharingStatus.min_weight}
                  </div>
                </div>

                {/* Min Activity Window */}
                <div className="bg-slate-950/40 rounded-lg p-3">
                  <div className="text-[11px] text-slate-500 font-medium flex items-center gap-1.5 mb-1">
                    <Clock className="w-3.5 h-3.5 text-amber-400" />
                    Min Activity
                  </div>
                  <div className="text-lg font-bold text-slate-200 font-mono">
                    {sharingStatus.min_activity_hours}h
                  </div>
                </div>

                {/* Allowed Types */}
                <div className="bg-slate-950/40 rounded-lg p-3">
                  <div className="text-[11px] text-slate-500 font-medium flex items-center gap-1.5 mb-1">
                    <Filter className="w-3.5 h-3.5 text-emerald-400" />
                    Allowed Types
                  </div>
                  <div className="flex flex-wrap gap-1 mt-1">
                    {sharingStatus.node_types && sharingStatus.node_types.length > 0 ? (
                      sharingStatus.node_types.map((t: string) => (
                        <span key={t} className="px-1.5 py-0.5 rounded text-[11px] font-medium bg-slate-900 text-slate-400">
                          {t}
                        </span>
                      ))
                    ) : (
                      <span className="text-[11px] text-slate-600 italic">None</span>
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
                  "flex items-center gap-2 px-6 py-3 rounded-lg text-xs font-medium transition-all active:scale-95 text-slate-950",
                  sharingStatus.enabled
                    ? "bg-red-500 hover:bg-red-400"
                    : "bg-emerald-500 hover:bg-emerald-600"
                )}
              >
                <Power className="w-4 h-4 text-slate-950" />
                {sharingStatus.enabled ? "Disable sharing" : "Enable sharing"}
              </button>
            </div>

          </div>
        </div>
      )}

      {/* Grid Layout: Peers on the left (2/3), Registration on the right (1/3) */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        
        {/* Peers List Column */}
        <div className="lg:col-span-2 space-y-6">
          {/* Mesh DHT Peer Discovery (GET /api/v1/mesh/peers) */}
          <div className="space-y-3">
            <div className="flex items-center justify-between border-b border-slate-800/80 pb-2">
              <h3 className="text-sm font-medium font-sans text-slate-200 flex items-center gap-2">
                <Network className="w-4 h-4 text-amber-400" />
                Mesh DHT Peer Discovery ({meshPeers.length})
                {meshStatus && (
                  <span className="text-[11px] font-mono text-slate-400 font-normal">
                    (buckets: {meshStatus.buckets}, total: {meshStatus.peer_count})
                  </span>
                )}
              </h3>
              <div className="flex items-center gap-1.5">
                <button
                  type="button"
                  onClick={() => fetchMeshPeers(false)}
                  className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md bg-slate-855 hover:bg-slate-800 text-[11px] text-slate-300 transition-colors focus-visible:ring-2 focus-visible:ring-emerald-400 focus-visible:outline-none"
                >
                  <RefreshCw className={cn("w-3 h-3 text-amber-400", peersLoading && "animate-spin")} />
                  <span>Retry Mesh</span>
                </button>
                {peersLoading && (
                  <button
                    type="button"
                    onClick={() => {
                      if (meshAbortRef.current) {
                        meshAbortRef.current.abort();
                        meshAbortRef.current = null;
                      }
                      setPeersLoading(false);
                      setMeshError('Consulta Mesh cancelada por el usuario.');
                      setMeshEndpointStatus('error');
                    }}
                    className="inline-flex items-center gap-1 px-2 py-1 rounded-md bg-rose-500/10 hover:bg-rose-500/20 text-[11px] text-rose-300 border border-rose-500/20 cursor-pointer"
                  >
                    Cancel
                  </button>
                )}
              </div>
            </div>

            {meshEndpointStatus === 'forbidden' ? (
              <div className="p-6 border border-rose-500/30 bg-rose-950/20 rounded-lg text-center space-y-2">
                <ShieldAlert className="w-6 h-6 text-rose-400 mx-auto" />
                <p className="text-xs font-semibold text-rose-300">Requiere credencial de operador (HTTP 403)</p>
                <p className="text-[11px] text-slate-400 max-w-md mx-auto">{meshError}</p>
                <button
                  type="button"
                  onClick={() => fetchMeshPeers(false)}
                  className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-medium cursor-pointer transition-colors focus-visible:ring-2 focus-visible:ring-emerald-400 focus-visible:outline-none mt-1"
                >
                  <RefreshCw className="w-3.5 h-3.5 text-slate-400" />
                  <span>Reintentar</span>
                </button>
              </div>
            ) : meshEndpointStatus === 'error' ? (
              <div className="p-6 border border-red-500/30 bg-red-950/20 rounded-lg text-center space-y-2">
                <WifiOff className="w-6 h-6 text-red-400 mx-auto" />
                <p className="text-xs font-semibold text-red-300">Error en telemetría Mesh</p>
                <p className="text-[11px] text-slate-400 max-w-md mx-auto">{meshError}</p>
                <button
                  type="button"
                  onClick={() => fetchMeshPeers(false)}
                  className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-medium cursor-pointer transition-colors focus-visible:ring-2 focus-visible:ring-emerald-400 focus-visible:outline-none mt-1"
                >
                  <RefreshCw className="w-3.5 h-3.5 text-slate-400" />
                  <span>Reintentar</span>
                </button>
              </div>
            ) : meshPeers.length === 0 ? (
              <div className="p-6 border-2 border-dashed border-slate-800/80 bg-slate-900/10 rounded-lg text-center space-y-1">
                <p className="text-xs text-slate-400">No active DHT mesh peers discovered in local routing table.</p>
                <p className="text-[11px] text-slate-500">Peers appear here when discovered via mDNS LAN or DHT bootstrap.</p>
              </div>
            ) : (
              <div className="bg-slate-900/20 rounded-lg overflow-hidden backdrop-blur-md">
                <div className="overflow-x-auto">
                  <table className="w-full text-left border-collapse">
                    <thead>
                      <tr className="border-b border-slate-800/80 text-[11px] text-slate-400 font-medium bg-slate-950/60">
                        <th className="py-2.5 px-4">Node ID</th>
                        <th className="py-2.5 px-4">Socket Address</th>
                        <th className="py-2.5 px-4">Capabilities</th>
                        <th className="py-2.5 px-4 text-right">Last Seen</th>
                      </tr>
                    </thead>
                    <tbody className="divide-y divide-slate-850 text-xs font-mono">
                      {meshPeers.map((mp) => (
                        <tr key={mp.node_id} className="hover:bg-slate-900/30 transition-colors">
                          <td className="py-2.5 px-4 text-slate-200 truncate max-w-[180px]" title={mp.node_id}>
                            {mp.node_id}
                          </td>
                          <td className="py-2.5 px-4 text-slate-400">{mp.addr}</td>
                          <td className="py-2.5 px-4">
                            <div className="flex flex-wrap gap-1">
                              {mp.capabilities && mp.capabilities.length > 0 ? (
                                mp.capabilities.map((c) => (
                                  <span key={c} className="px-1.5 py-0.5 rounded text-[11px] bg-slate-800 text-slate-300 font-sans">
                                    {c}
                                  </span>
                                ))
                              ) : (
                                <span className="text-slate-600 font-sans italic text-[11px]">—</span>
                              )}
                            </div>
                          </td>
                          <td className="py-2.5 px-4 text-right text-slate-400">
                            {formatLastSync(mp.last_seen)}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </div>
            )}
          </div>

          {/* Configured Peer Nodes */}
          <div className="space-y-3">
            <div className="flex items-center justify-between border-b border-slate-800/80 pb-2">
              <h3 className="text-sm font-medium font-sans text-slate-200 flex items-center gap-2">
                <Globe className="w-4 h-4 text-emerald-400" />
                Configured Peer Nodes ({peers.length})
              </h3>
              <button
                type="button"
                onClick={() => fetchPeers(false)}
                className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md bg-slate-850 hover:bg-slate-800 text-[11px] text-slate-300 transition-colors focus-visible:ring-2 focus-visible:ring-emerald-400 focus-visible:outline-none"
              >
                <RefreshCw className="w-3 h-3 text-emerald-400" />
                <span>Reintentar Peers</span>
              </button>
            </div>

            {peersLoading && peers.length === 0 ? (
              <div className="flex flex-col items-center justify-center py-12 text-slate-500 bg-slate-900/10 rounded-lg">
                <RefreshCw className="w-8 h-8 animate-spin text-emerald-500/40 mb-3" />
                <p className="text-xs">Querying network topology...</p>
              </div>
            ) : peers.length === 0 ? (
              <div className="p-8 border-2 border-dashed border-slate-800/80 bg-slate-900/10 rounded-lg text-center flex flex-col items-center justify-center gap-2">
                <WifiOff className="w-8 h-8 text-slate-600" />
                <p className="text-xs text-slate-500">No peer nodes currently configured.</p>
                <p className="text-[11px] text-slate-600">Register a peer node on the right to begin federation.</p>
              </div>
            ) : (
            <div className="bg-slate-900/20 rounded-lg overflow-hidden backdrop-blur-md">
              <div className="overflow-x-auto">
                <table className="w-full text-left border-collapse">
                  <thead>
                    <tr className="border-b border-slate-800/80 text-[11px] text-slate-400 font-medium bg-slate-950/60">
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
                                <span className="font-mono font-semibold text-slate-200">
                                  {peer.name}
                                </span>
                                {isAuto && (
                                  <span className="px-2 py-0.5 text-[11px] font-medium text-emerald-400 bg-emerald-700/40 rounded-full animate-pulse">
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
                              <div className="inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-[11px] font-medium bg-slate-800/50 text-slate-400">
                                <RefreshCw className="w-2.5 h-2.5 animate-spin" />
                                Ping
                              </div>
                            ) : status.online ? (
                              <div className="inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-[11px] font-medium bg-emerald-500/10 text-emerald-400">
                                <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-ping" />
                                Online
                              </div>
                            ) : (
                              <div className="inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-[11px] font-medium bg-red-500/10 text-red-400">
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
                                  className="px-2 py-1 bg-red-500 hover:bg-red-400 text-slate-950 font-medium rounded-lg text-[11px] transition-all active:scale-95"
                                >
                                  OK
                                </button>
                                <button
                                  onClick={() => setConfirmDeletePeer(null)}
                                  className="px-2 py-1 bg-slate-800 hover:bg-slate-700 text-slate-300 font-medium rounded-lg text-[11px] transition-all"
                                >
                                  Cancel
                                </button>
                              </div>
                            ) : (
                              <button
                                onClick={() => setConfirmDeletePeer(peer.name)}
                                className="p-2 text-slate-500 hover:text-red-400 bg-slate-950 hover:bg-red-500/10 rounded-lg transition-all active:scale-95"
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
        </div>

        {/* Register Manual Peer Form Column */}
        <div className="space-y-4">
          <div className="flex items-center justify-between border-b border-slate-800/80 pb-2">
            <h3 className="text-sm font-medium font-sans text-slate-200 flex items-center gap-2">
              <Plus className="w-4 h-4 text-emerald-400" />
              Register Peer Node
            </h3>
          </div>

          <div className="bg-slate-900/30 rounded-lg p-5 backdrop-blur-md">
            <form onSubmit={handleAddPeer} className="space-y-4">
              {/* Peer Alias */}
              <div className="space-y-1.5">
                <label className="block text-[11px] font-medium text-slate-400">Peer Name / Alias</label>
                <input
                  type="text"
                  required
                  placeholder="e.g., node-tokyo"
                  value={peerName}
                  onChange={(e) => setPeerName(e.target.value)}
                  className="w-full px-3.5 py-2 rounded-lg bg-slate-950 focus:border-emerald-500 focus:ring-1 focus:ring-emerald-500 focus:outline-none text-sm text-slate-200 transition-all placeholder:text-slate-600"
                />
              </div>

              {/* Peer Endpoint URL */}
              <div className="space-y-1.5">
                <label className="block text-[11px] font-medium text-slate-400">Endpoint URL</label>
                <input
                  type="url"
                  required
                  placeholder="e.g., http://localhost:3032"
                  value={peerUrl}
                  onChange={(e) => setPeerUrl(e.target.value)}
                  className="w-full px-3.5 py-2 rounded-lg bg-slate-950 focus:border-emerald-500 focus:ring-1 focus:ring-emerald-500 focus:outline-none text-sm text-slate-200 transition-all placeholder:text-slate-600"
                />
              </div>

              {/* Peer Connection Token */}
              <div className="space-y-1.5">
                <div className="flex justify-between items-center">
                  <label className="block text-[11px] font-medium text-slate-400 flex items-center gap-1">
                    <Key className="w-3 h-3 text-slate-500" /> Connection Token
                  </label>
                  <span className="text-[11px] text-slate-500 font-mono">Use "mdns-auto" for discovery badge</span>
                </div>
                <input
                  type="password"
                  required
                  placeholder="Paste security key or 'mdns-auto'"
                  value={peerToken}
                  onChange={(e) => setPeerToken(e.target.value)}
                  className="w-full px-3.5 py-2 rounded-lg bg-slate-950 focus:border-emerald-500 focus:ring-1 focus:ring-emerald-500 focus:outline-none text-sm text-slate-200 transition-all placeholder:text-slate-600"
                />
              </div>

              {/* Submit Button */}
              <button
                type="submit"
                disabled={submittingPeer}
                className="w-full py-2.5 mt-2 bg-slate-800 hover:bg-slate-700 text-emerald-400 rounded-lg text-xs font-medium transition-all disabled:opacity-50 flex items-center justify-center gap-2 active:scale-[0.98]"
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
            <h3 className="text-sm font-medium font-sans text-slate-200 flex items-center gap-2">
              <Database className="w-4 h-4 text-violet-400" />
              SilvaDB Shareable Nodes Control ({fedNodesCount})
              <span className="text-[11px] font-mono text-slate-400 font-normal">
                (source: {fedNodesSource})
              </span>
            </h3>
            <p className="text-[11px] text-slate-500 mt-0.5">Federated memory candidates queried from /api/v1/federation/nodes across peer connections.</p>
          </div>
          <div className="flex items-center gap-1.5">
            <button
              type="button"
              onClick={() => fetchNodes(false)}
              className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md bg-slate-850 hover:bg-slate-800 text-[11px] text-slate-300 transition-colors focus-visible:ring-2 focus-visible:ring-emerald-400 focus-visible:outline-none"
            >
              <RefreshCw className={cn("w-3 h-3 text-violet-400", nodesLoading && "animate-spin")} />
              <span>Retry Nodes</span>
            </button>
            {nodesLoading && (
              <button
                type="button"
                onClick={() => {
                  if (nodesAbortRef.current) {
                    nodesAbortRef.current.abort();
                    nodesAbortRef.current = null;
                  }
                  setNodesLoading(false);
                  setFedNodesError('Consulta de nodos cancelada por el usuario.');
                  setFedNodesEndpointStatus('error');
                }}
                className="inline-flex items-center gap-1 px-2 py-1 rounded-md bg-rose-500/10 hover:bg-rose-500/20 text-[11px] text-rose-300 border border-rose-500/20 cursor-pointer"
              >
                Cancel
              </button>
            )}
          </div>
        </div>

        {fedNodesEndpointStatus === 'forbidden' ? (
          <div className="p-8 border border-rose-500/30 bg-rose-950/20 rounded-lg text-center space-y-2">
            <ShieldAlert className="w-8 h-8 text-rose-400 mx-auto" />
            <p className="text-sm font-semibold text-rose-300">Requiere credencial de operador (HTTP 403)</p>
            <p className="text-xs text-slate-400 max-w-md mx-auto">{fedNodesError}</p>
            <button
              type="button"
              onClick={() => fetchNodes(false)}
              className="inline-flex items-center gap-1.5 px-3.5 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-medium cursor-pointer transition-colors focus-visible:ring-2 focus-visible:ring-emerald-400 focus-visible:outline-none mt-2"
            >
              <RefreshCw className="w-3.5 h-3.5 text-slate-400" />
              <span>Reintentar</span>
            </button>
          </div>
        ) : fedNodesEndpointStatus === 'error' ? (
          <div className="p-8 border border-red-500/30 bg-red-950/20 rounded-lg text-center space-y-2">
            <WifiOff className="w-8 h-8 text-red-400 mx-auto" />
            <p className="text-sm font-semibold text-red-300">Error al cargar nodos de federación</p>
            <p className="text-xs text-slate-400 max-w-md mx-auto">{fedNodesError}</p>
            <button
              type="button"
              onClick={() => fetchNodes(false)}
              className="inline-flex items-center gap-1.5 px-3.5 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-medium cursor-pointer transition-colors focus-visible:ring-2 focus-visible:ring-emerald-400 focus-visible:outline-none mt-2"
            >
              <RefreshCw className="w-3.5 h-3.5 text-slate-400" />
              <span>Reintentar</span>
            </button>
          </div>
        ) : nodesLoading && fedNodes.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-16 text-slate-500 bg-slate-900/10 rounded-lg">
            <RefreshCw className="w-8 h-8 animate-spin text-violet-500/40 mb-3" />
            <p className="text-xs">Loading SilvaDB nodes...</p>
          </div>
        ) : fedNodes.length === 0 ? (
          <div className="p-10 border-2 border-dashed border-slate-800/80 bg-slate-900/10 rounded-lg text-center space-y-2">
            <p className="text-xs text-slate-500">No nodes found in SilvaDB.</p>
            <button
              type="button"
              onClick={() => fetchNodes(false)}
              className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-300 text-xs font-medium cursor-pointer transition-colors focus-visible:ring-2 focus-visible:ring-emerald-400 focus-visible:outline-none"
            >
              <RefreshCw className="w-3.5 h-3.5 text-slate-400" />
              <span>Reintentar</span>
            </button>
          </div>
        ) : (
          <div className="bg-slate-900/20 rounded-lg overflow-hidden backdrop-blur-md shadow-xl">
            <div className="overflow-x-auto max-h-[400px]">
              <table className="w-full text-left border-collapse">
                <thead>
                  <tr className="border-b border-slate-800/80 text-[11px] text-slate-400 font-medium bg-slate-950/60 sticky top-0 z-10 backdrop-blur">
                    <th className="py-3 px-5">Node ID / Type</th>
                    <th className="py-3 px-5">Memory Content</th>
                    <th className="py-3 px-5 text-center">Weight</th>
                    <th className="py-3 px-5 text-center">Sharing Status</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-850 text-sm">
                  {fedNodes.map((node: FederationNodeItem) => {
                    const isShareable = !!node.shareable;
                    return (
                      <tr key={node.id} className={cn(
                        "hover:bg-slate-900/30 transition-colors",
                        isShareable && "bg-emerald-500/5"
                      )}>
                        <td className="py-4 px-5">
                          <div className="space-y-1">
                            <span className="block font-mono text-[11px] font-bold text-slate-300">
                              {node.id.length > 20 ? `${node.id.substring(0, 20)}...` : node.id}
                            </span>
                            <span className={cn(
                              "inline-block text-[11px] px-1.5 py-0.5 rounded font-medium",
                              getNodeTypeColor(node.node_type)
                            )}>
                              {node.node_type || 'agnostic'}
                            </span>
                          </div>
                        </td>
                        <td className="py-4 px-5 text-xs text-slate-400 font-sans max-w-[480px] break-words line-clamp-2">
                          {node.content || <span className="italic text-slate-500">No content snippet</span>}
                        </td>
                        <td className="py-4 px-5 text-center font-mono text-xs text-slate-400">
                          {typeof node.weight === 'number'
                            ? (node.weight > 0.001 ? node.weight.toFixed(3) : node.weight.toExponential(2))
                            : '—'}
                        </td>
                        <td className="py-4 px-5 text-center">
                          <div className="flex items-center justify-center">
                            <button
                              type="button"
                              onClick={() => handleToggleShareable(node.id, isShareable)}
                              disabled={togglingNodeId === node.id}
                              className={cn(
                                "flex items-center gap-2 px-3 py-1.5 rounded-lg text-[11px] font-medium transition-all active:scale-95 disabled:opacity-50",
                                isShareable
                                  ? "bg-emerald-500/10 text-emerald-400"
                                  : "bg-slate-950 text-slate-500 hover:bg-slate-800"
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
