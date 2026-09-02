import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { NexusBridge } from '../lib/nexus-bridge'
import { useAppStore } from '../stores/useAppStore'
import { useColoquioMentions } from './useColoquioMentions'
import { useHitlGrants } from './useHitlGrants'
import { useSSEBridgeEvents } from './useSSEBridgeEvents'
import { useThemeManager } from './useThemeManager'

function resetAppStore() {
  useAppStore.setState({
    theme: 'system',
    activeTab: 'overview',
    mountedTabs: new Set(['overview']),
    toasts: [],
    kernelUptime: 0,
    coloquioUnread: 0,
    activeMentions: [],
    showMentionsDropdown: false,
    pendingGrant: null,
  })
}

function installMatchMedia(initialMatches: boolean) {
  let matches = initialMatches
  const listeners = new Set<(event: MediaQueryListEvent) => void>()
  const media = {
    get matches() {
      return matches
    },
    media: '(prefers-color-scheme: dark)',
    onchange: null,
    addEventListener: (_type: string, listener: (event: MediaQueryListEvent) => void) => {
      listeners.add(listener)
    },
    removeEventListener: (_type: string, listener: (event: MediaQueryListEvent) => void) => {
      listeners.delete(listener)
    },
    dispatchEvent: () => true,
  } as unknown as MediaQueryList

  vi.stubGlobal('matchMedia', vi.fn(() => media))

  return {
    emit(nextMatches: boolean) {
      matches = nextMatches
      const event = { matches, media: media.media } as MediaQueryListEvent
      listeners.forEach((listener) => listener(event))
    },
  }
}

beforeEach(() => {
  resetAppStore()
  localStorage.clear()
  document.documentElement.className = ''
  vi.clearAllMocks()
  vi.unstubAllGlobals()
})

describe('useThemeManager', () => {
  it('applies system theme and reacts to OS theme changes', () => {
    const media = installMatchMedia(true)
    const { result } = renderHook(() => useThemeManager())

    expect(result.current.theme).toBe('system')
    expect(document.documentElement.classList.contains('dark')).toBe(true)

    act(() => media.emit(false))
    expect(document.documentElement.classList.contains('light')).toBe(true)

    act(() => result.current.setTheme('dark'))
    expect(document.documentElement.classList.contains('dark')).toBe(true)
    expect(localStorage.getItem('tylluan_theme')).toBe('dark')
  })
})

describe('useColoquioMentions', () => {
  it('notifies and increments unread count only for addressed agents', () => {
    const notify = vi.fn()
    const { unmount } = renderHook(() => useColoquioMentions({ notify }))

    act(() => {
      window.dispatchEvent(new CustomEvent('nexus_mention', {
        detail: {
          agent_id: 'jose',
          channel: 'mision-activa',
          message: 'Revisa el frontend',
          sender: 'claude-code',
        },
      }))
    })

    expect(notify).toHaveBeenCalledWith(
      'claude-code in #mision-activa: "Revisa el frontend"',
      'info',
      'Mention Received',
    )
    expect(useAppStore.getState().coloquioUnread).toBe(1)
    expect(useAppStore.getState().activeMentions[0]).toMatchObject({
      sender: 'claude-code',
      channel: 'mision-activa',
      message: 'Revisa el frontend',
    })

    act(() => {
      window.dispatchEvent(new CustomEvent('nexus_mention', {
        detail: { agent_id: 'qwen', channel: 'mision-activa', message: 'ignore me', sender: 'agent' },
      }))
    })
    expect(useAppStore.getState().coloquioUnread).toBe(1)

    unmount()
    act(() => {
      window.dispatchEvent(new CustomEvent('nexus_mention', {
        detail: { agent_id: 'jose', channel: 'mision-activa', message: 'after unmount', sender: 'agent' },
      }))
    })
    expect(notify).toHaveBeenCalledTimes(1)
  })
})

describe('useSSEBridgeEvents', () => {
  it('routes dream-cycle and federation events to notifications and cleans up', () => {
    const notify = vi.fn()
    const { unmount } = renderHook(() => useSSEBridgeEvents({ notify }))

    act(() => {
      window.dispatchEvent(new CustomEvent('nexus_event_dream_cycle_complete', {
        detail: { duplicates_merged: 2, clusters_consolidated: 3, nodes_decayed: 4 },
      }))
      window.dispatchEvent(new CustomEvent('nexus_event_federation_sync', {
        detail: { peer: 'node-b', count: 7 },
      }))
    })

    expect(notify).toHaveBeenNthCalledWith(
      1,
      'Cognitive consolidation cycle (NREM) completed. Merged duplicates: 2, consolidated clusters: 3, decayed nodes: 4.',
      'info',
      'NREM Consolidation',
    )
    expect(notify).toHaveBeenNthCalledWith(
      2,
      'Federation sync completed with node-b. Synchronized 7 knowledge nodes.',
      'info',
      'P2P Federation',
    )

    unmount()
    notify.mockClear()
    act(() => {
      window.dispatchEvent(new CustomEvent('nexus_event_federation_sync', {
        detail: { peer: 'node-c', count: 1 },
      }))
    })
    expect(notify).not.toHaveBeenCalled()
  })
})

describe('useHitlGrants', () => {
  it('shows a grant request and persists an approved session grant', async () => {
    const notify = vi.fn()
    const fetchRaw = vi.fn().mockResolvedValue({ content: [{ text: 'approved' }], is_error: false })
    const bridge = { fetchRaw } as unknown as NexusBridge
    const { result } = renderHook(() => useHitlGrants({ bridge, notify }))

    act(() => {
      window.dispatchEvent(new CustomEvent('nexus_event_grant_required', {
        detail: {
          id: 'grant-123',
          guild: 'bash',
          agent_id: 'claude-code',
          tool_name: 'bash_execute',
          reason: 'Requires shell access',
        },
      }))
    })

    expect(result.current.pendingGrant).toMatchObject({
      requestId: 'grant-123',
      guild: 'bash',
      agentId: 'claude-code',
      tool: 'bash_execute',
    })

    await act(async () => {
      await result.current.handleApproveGrant('session')
    })

    expect(fetchRaw).toHaveBeenCalledWith('/api/v1/do', expect.objectContaining({ method: 'POST' }))
    const request = fetchRaw.mock.calls[0][1] as RequestInit
    expect(JSON.parse(String(request.body))).toMatchObject({
      tool: 'approve_action',
      arguments: {
        requestId: 'grant-123',
        approved: true,
        grant_level: 'this_session',
      },
    })
    expect(JSON.parse(localStorage.getItem('tylluan_sandbox_grants') || '[]')).toMatchObject([
      { id: 'grant-123', guild: 'bash', scope: 'this_session' },
    ])
    expect(useAppStore.getState().pendingGrant).toBeNull()
    expect(notify).toHaveBeenCalledWith(
      "Grant approved for 'bash' (this_session).",
      'info',
      'HITL Authorization',
    )
  })

  it('clears an expired grant when the backend returns an MCP error flag', async () => {
    const notify = vi.fn()
    const fetchRaw = vi.fn().mockResolvedValue({
      content: [{ text: 'request not found' }],
      is_error: true,
    })
    const bridge = { fetchRaw } as unknown as NexusBridge
    const { result } = renderHook(() => useHitlGrants({ bridge, notify }))

    act(() => {
      window.dispatchEvent(new CustomEvent('nexus_event_grant_required', {
        detail: { id: 'expired-grant', guild: 'code', tool_name: 'run' },
      }))
    })
    await act(async () => {
      await result.current.handleApproveGrant('once')
    })

    expect(useAppStore.getState().pendingGrant).toBeNull()
    expect(localStorage.getItem('tylluan_sandbox_grants')).toBeNull()
    expect(notify).toHaveBeenCalledWith(
      "Grant 'expired-grant' is no longer pending (expired or already resolved).",
      'error',
    )
  })
})
