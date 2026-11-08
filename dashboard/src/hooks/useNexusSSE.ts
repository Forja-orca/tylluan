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
  standbyCountdownSecs: number;
  currentStepSecs: number;
  isProbing: boolean;
  triggerImmediateReconnect: () => void;
}

const BACKOFF_STEPS_SECS = [5, 10, 15, 30, 60] as const;

export function useNexusSSE(
  bridge: NexusBridge | null,
  { onError, onGuildStatus, maxEvents = 200 }: UseNexusSSEOptions = {}
): SSEState {
  const [events, setEvents] = useState<NexusEvent[]>([]);
  const [online, setOnline] = useState(false);
  const [lastHeartbeat, setLastHeartbeat] = useState<number>(Date.now());
  const [reconnectAttempts, setReconnectAttempts] = useState(0);
  const [connectionStatus, setConnectionStatus] = useState<SSEState['connectionStatus']>('offline');
  const [standbyCountdownSecs, setStandbyCountdownSecs] = useState<number>(BACKOFF_STEPS_SECS[0]);
  const [currentStepSecs, setCurrentStepSecs] = useState<number>(BACKOFF_STEPS_SECS[0]);
  const [isProbing, setIsProbing] = useState<boolean>(false);
  const nextProbeSecsRef = useRef<number>(BACKOFF_STEPS_SECS[0]);
  const attemptCountRef = useRef<number>(0);

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
      const d = (ev.data ?? {}) as Record<string, unknown>;
      const msg = (typeof d.message === 'string' && d.message)
        || (typeof d.error === 'string' && d.error)
        || 'Kernel reported an error';
      const guild = (typeof d.guild === 'string' ? d.guild : undefined)
        || (typeof d.tool === 'string' ? d.tool : undefined);
      onError?.(msg, guild);
    }

    // ── Guild status updates (kernel sends 'guild_status', not 'guild_killed') ──
    if (ev.type === 'guild_status') {
      const d = (ev.data ?? {}) as Record<string, unknown>;
      const guild = typeof d.guild === 'string' ? d.guild : 'unknown';
      const status = typeof d.status === 'string' ? d.status : 'unknown';
      const pid = typeof d.pid === 'number' ? d.pid : null;
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
      const data = (ev.data ?? {}) as Record<string, unknown>;
      const targetAgent = ev.type.startsWith('mention:') 
        ? ev.type.slice(8) 
        : (typeof data.agent_id === 'string' ? data.agent_id : 'all');
      const channel = typeof data.channel_id === 'string' ? data.channel_id : 'coloquio';
      const message = (typeof data.message === 'string' ? data.message : undefined)
        || (typeof data.content === 'string' ? data.content : 'Has sido mencionado');
      const sender = (typeof data.sender_id === 'string' ? data.sender_id : undefined)
        || (typeof data.author_id === 'string' ? data.author_id : 'alguien');
      
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
      attemptCountRef.current = 0;
      nextProbeSecsRef.current = BACKOFF_STEPS_SECS[0];
      setStandbyCountdownSecs(BACKOFF_STEPS_SECS[0]);
      setCurrentStepSecs(BACKOFF_STEPS_SECS[0]);
      setIsProbing(false);
      resetHeartbeatTimer();
    } else {
      const att = attempts ?? attemptCountRef.current;
      attemptCountRef.current = att;
      setReconnectAttempts(att);
      setConnectionStatus(att > 0 ? 'reconnecting' : 'offline');
    }
  }, [resetHeartbeatTimer]);

  const probeHealthOnce = useCallback(async () => {
    if (!bridge) return false;
    setIsProbing(true);
    try {
      const health = await bridge.getHealth();
      if (health) {
        attemptCountRef.current = 0;
        setReconnectAttempts(0);
        nextProbeSecsRef.current = BACKOFF_STEPS_SECS[0];
        setStandbyCountdownSecs(BACKOFF_STEPS_SECS[0]);
        setCurrentStepSecs(BACKOFF_STEPS_SECS[0]);
        setIsProbing(false);
        handleEvent({
          type: 'heartbeat',
          data: { uptime_secs: health.uptime_secs || 0 },
          source: 'dashboard' as const,
          ts: Date.now()
        });
        setConnectionStatus('connected');
        window.dispatchEvent(new CustomEvent('nexus_polling_refresh'));
        return true;
      }
    } catch {
      // Expected while kernel is in standby / restarting
    }
    const nextAttempt = attemptCountRef.current + 1;
    attemptCountRef.current = nextAttempt;
    setReconnectAttempts(nextAttempt);
    setConnectionStatus('reconnecting');
    const backoffIdx = Math.min(nextAttempt, BACKOFF_STEPS_SECS.length - 1);
    const nextStep = BACKOFF_STEPS_SECS[backoffIdx];
    nextProbeSecsRef.current = nextStep;
    setStandbyCountdownSecs(nextStep);
    setCurrentStepSecs(nextStep);
    setIsProbing(false);
    return false;
  }, [bridge, handleEvent]);

  const triggerImmediateReconnect = useCallback(() => {
    nextProbeSecsRef.current = 0;
    setStandbyCountdownSecs(0);
    void probeHealthOnce();
  }, [probeHealthOnce]);

  // Real countdown to NEXT probe + backoff probe when SSE is offline or reconnecting
  useEffect(() => {
    if (!bridge || connectionStatus === 'connected') return;

    const tickInterval = setInterval(() => {
      // If window is hidden, hold or avoid busy spinning
      if (typeof document !== 'undefined' && document.hidden) {
        return;
      }

      if (nextProbeSecsRef.current > 1) {
        nextProbeSecsRef.current -= 1;
        setStandbyCountdownSecs(nextProbeSecsRef.current);
      } else if (nextProbeSecsRef.current === 1) {
        nextProbeSecsRef.current = 0;
        setStandbyCountdownSecs(0);
        void probeHealthOnce();
      }
    }, 1000);

    return () => clearInterval(tickInterval);
  }, [bridge, connectionStatus, probeHealthOnce]);

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

  return {
    events,
    online,
    lastHeartbeat,
    reconnectAttempts,
    connectionStatus,
    standbyCountdownSecs,
    currentStepSecs,
    isProbing,
    triggerImmediateReconnect,
  };
}

