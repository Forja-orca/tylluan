import { useEffect } from 'react'
import type { NexusBridge } from '../lib/nexus-bridge'
import { useAppStore } from '../stores/useAppStore'

const GRANT_LEVEL_MAP = {
  once: 'this_time',
  session: 'this_session',
  always: 'always_for_guild',
} as const

interface GrantRequiredDetail {
  id: string
  guild?: string
  agent_id?: string
  tool_name?: string
  reason?: string
}

interface ApprovalResult {
  content?: Array<{ text?: string }>
  isError?: boolean
  is_error?: boolean
}

interface StoredGrant {
  id: string
  guild: string
  tool: string
  scope: string
  approved_by: string
  ts: number
}

interface UseHitlGrantsOptions {
  bridge: NexusBridge | null
  notify: (msg: string, type?: 'info' | 'error', guild?: string) => void
}

export function useHitlGrants({ bridge, notify }: UseHitlGrantsOptions) {
  const { pendingGrant, setPendingGrant } = useAppStore()

  const handleApproveGrant = async (scope: 'once' | 'session' | 'always') => {
    if (!pendingGrant || !bridge) return
    const grant_level = GRANT_LEVEL_MAP[scope]
    try {
      const result = await bridge.fetchRaw('/api/v1/do', {
        method: 'POST',
        body: JSON.stringify({
          tool: 'approve_action',
          arguments: {
            requestId: pendingGrant.requestId,
            approved: true,
            grant_level,
          },
        }),
      }) as ApprovalResult
      const isError =
        (Array.isArray(result?.content) && result.content.some((content) => content.text?.includes('not found'))) ||
        result?.isError ||
        result?.is_error
      if (isError) {
        notify(
          `Grant '${pendingGrant.requestId}' ya no está pendiente (expiró o fue resuelto).`,
          'error'
        )
        setPendingGrant(null)
        return
      }

      notify(
        `Grant aprobado para '${pendingGrant.guild}' (${grant_level}).`,
        'info',
        'HITL Authorization'
      )

      const currentGrants = JSON.parse(
        localStorage.getItem('tylluan_sandbox_grants') || '[]'
      ) as StoredGrant[]
      const newGrant: StoredGrant = {
        id: pendingGrant.requestId,
        guild: pendingGrant.guild,
        tool: pendingGrant.tool,
        scope: grant_level,
        approved_by: 'Dashboard UI (HITL)',
        ts: Date.now(),
      }
      localStorage.setItem(
        'tylluan_sandbox_grants',
        JSON.stringify([newGrant, ...currentGrants].slice(0, 10))
      )
      window.dispatchEvent(new CustomEvent('tylluan_grant_updated'))

      setPendingGrant(null)
    } catch (e: unknown) {
      notify(
        `Failed to approve grant: ${e instanceof Error ? e.message : String(e)}`,
        'error'
      )
    }
  }

  useEffect(() => {
    const handleCapabilityGrant = (e: Event) => {
      const detail = (e as CustomEvent<GrantRequiredDetail>).detail
      if (!detail?.id) return
      setPendingGrant({
        requestId: detail.id,
        guild: detail.guild || 'unknown',
        agentId: detail.agent_id || 'unknown',
        tool: detail.tool_name || 'unknown',
        blockedReason: detail.reason || 'Requisito de seguridad del sandbox',
        options: ['once', 'session', 'always'],
      })
    }

    window.addEventListener('nexus_event_grant_required', handleCapabilityGrant)
    return () => {
      window.removeEventListener(
        'nexus_event_grant_required',
        handleCapabilityGrant
      )
    }
  }, [setPendingGrant])

  return {
    pendingGrant,
    setPendingGrant,
    handleApproveGrant,
  }
}
