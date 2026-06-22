/**
 * useNexusSSE — Resilient SSE client hook for TylluanNexus Dashboard V4.
 *
 * Features:
 *  - Typed NexusEvent stream with full discriminated union support
 *  - Exponential backoff reconnection (1.5x factor, max 30s cap)
 *  - Graceful error_result SSE events routed to notify() toast system
 *  - Stale-heartbeat detection: marks offline if no heartbeat for 45s
 *  - Capped event buffer (200 events, newest first)
 */
import { useEffect, useState, useCallback, useRef } from 'react';
import { NexusEvent, NexusBridge } from '../lib/nexus-bridge';

interface UseNexusSSEOptions {
  /** Called when an error_result arrives in the event stream */
  onError?: (msg: string, guild?: string) => void;
  /** Called when a guild_status event arrives (guild name, status, pid) */
  onGuildStatus?: (guild: string, status: string, pid: number | null) => void;
  /** Max events to keep in buffer (default: 200) */
  maxEvents?: number;
}

export interface SSEState {
  events: NexusEvent[];
  online: boolean;
  lastHeartbeat: number;
  reconnectAttempts: number;
  connectionStatus: 'connected' | 'reconnecting' | 'offline';
}

export function useNexusSSE(
  bridge: NexusBridge | null,
  { onError, onGuildStatus, maxEvents = 200 }: UseNexusSSEOptions = {}
): SSEState {
  const [events, setEvents] = useState<NexusEvent[]>([]);
  const [online, setOnline] = useState(false);
  const [lastHeartbeat, setLastHeartbeat] = useState<number>(Date.now());
  const [reconnectAttempts, setReconnectAttempts] = useState(0);
  const [connectionStatus, setConnectionStatus] = useState<SSEState['connectionStatus']>('offline');

  // Stale-heartbeat detector: if no heartbeat for 45s → mark offline
  const heartbeatTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const HEARTBEAT_TIMEOUT = 45_000;

  const resetHeartbeatTimer = useCallback(() => {
    if (heartbeatTimerRef.current) clearTimeout(heartbeatTimerRef.current);
    heartbeatTimerRef.current = setTimeout(() => {
      setOnline(false);
      setConnectionStatus('offline');
    }, HEARTBEAT_TIMEOUT);
  }, []);

  // Stable callback — categorizes and handles each event type
  const handleEvent = useCallback((ev: NexusEvent) => {
    // ── Append to buffer ──────────────────────────────────────────────────
    setEvents(prev => [ev, ...prev].slice(0, maxEvents));

    // ── Heartbeat ─────────────────────────────────────────────────────────
    if (ev.type === 'heartbeat') {
      setLastHeartbeat(Date.now());
      setOnline(true);
      if (ev.source !== 'dashboard') {
        setConnectionStatus('connected');
      }
      resetHeartbeatTimer();
    }

    // ── Error result from kernel tools ────────────────────────────────────
    if (ev.type === 'error_result' || ev.type === 'tool_error') {
      const msg = (ev.data as any)?.message
        || (ev.data as any)?.error
        || 'Kernel reported an error';
      const guild = (ev.data as any)?.guild || (ev.data as any)?.tool;
      onError?.(msg, guild);
    }

    // ── Guild status updates (kernel sends 'guild_status', not 'guild_killed') ──
    if (ev.type === 'guild_status') {
      const guild = (ev.data as any)?.guild || 'unknown';
      const status = (ev.data as any)?.status || 'unknown';
      const pid = (ev.data as any)?.pid ?? null;
      if (status === 'crashed') {
        onError?.(`Guild crashed: ${guild}`, guild);
      }
      onGuildStatus?.(guild, status, pid);
    }

    // ── Tool call lifecycle events ────────────────────────────────────────
    if (ev.type === 'tool_call_start' || ev.type === 'tool_call_end') {
      // events already appended above; dispatch for component subscriptions
    }

    // ── Mention event detection ──────────────────────────────────────────
    if (ev.type === 'mention' || ev.type.startsWith('mention:')) {
      const data = ev.data as any;
      const targetAgent = ev.type.startsWith('mention:') 
        ? ev.type.slice(8) 
        : data?.agent_id || 'all';
      const channel = data?.channel_id || 'coloquio';
      const message = data?.message || data?.content || 'Has sido mencionado';
      const sender = data?.sender_id || data?.author_id || 'alguien';
      
      window.dispatchEvent(
        new CustomEvent('nexus_mention', { 
          detail: { agent_id: targetAgent, channel, message, sender } 
        })
      );
    }

    // ── Dispatch for component-level subscriptions ────────────────────────
    window.dispatchEvent(
      new CustomEvent(`nexus_event_${ev.type}`, { detail: ev.data })
    );
  }, [maxEvents, onError, onGuildStatus, resetHeartbeatTimer]);

  // Status callback — updates reconnect state
  const handleStatus = useCallback((isOnline: boolean, attempts?: number) => {
    setOnline(isOnline);
    if (isOnline) {
      setConnectionStatus('connected');
      setReconnectAttempts(0);
      resetHeartbeatTimer();
    } else {
      const att = attempts ?? 0;
      setReconnectAttempts(att);
      setConnectionStatus(att > 0 ? 'reconnecting' : 'offline');
    }
  }, [resetHeartbeatTimer]);

  // Polling fallback when SSE is offline or reconnecting
  useEffect(() => {
    if (!bridge || connectionStatus === 'connected') return;

    const pollInterval = setInterval(async () => {
      try {
        const health = await bridge.getHealth();
        if (health) {
          handleEvent({
            type: 'heartbeat',
            data: { uptime_secs: health.uptime_secs || 0 },
            source: 'dashboard' as const,
            ts: Date.now()
          });
          // Dispatch event to trigger refreshData in useNexus
          window.dispatchEvent(new CustomEvent('nexus_polling_refresh'));
        }
      } catch (err) {
        console.warn("[Polling Fallback] Health check failed:", err);
      }
    }, 5000);

    return () => clearInterval(pollInterval);
  }, [bridge, connectionStatus, handleEvent]);

  useEffect(() => {
    if (!bridge) return;

    // Use the public clone() API (BUG-03 fix — no unsafe any casts)
    const sseBridge = bridge.clone(handleEvent, handleStatus);
    sseBridge.connectEvents();
    resetHeartbeatTimer();

    return () => {
      sseBridge.disconnect();
      if (heartbeatTimerRef.current) clearTimeout(heartbeatTimerRef.current);
    };
  }, [bridge, handleEvent, handleStatus, resetHeartbeatTimer]);

  return { events, online, lastHeartbeat, reconnectAttempts, connectionStatus };
}

