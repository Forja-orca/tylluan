import { useEffect } from 'react'

interface DreamCycleDetail {
  duplicates_merged?: number
  clusters_consolidated?: number
  nodes_decayed?: number
}

interface FederationSyncDetail {
  peer?: string
  count?: number
}

interface UseSSEBridgeEventsOptions {
  notify: (msg: string, type?: 'info' | 'error', guild?: string) => void
}

export function useSSEBridgeEvents({ notify }: UseSSEBridgeEventsOptions) {
  useEffect(() => {
    const handleDreamCycle = (e: Event) => {
      const data = (e as CustomEvent<DreamCycleDetail>).detail
      notify(
        `Cognitive consolidation cycle (NREM) completed. Merged duplicates: ${data?.duplicates_merged ?? 0}, consolidated clusters: ${data?.clusters_consolidated ?? 0}, decayed nodes: ${data?.nodes_decayed ?? 0}.`,
        'info',
        'NREM Consolidation'
      )
    }

    const handleFederationSync = (e: Event) => {
      const data = (e as CustomEvent<FederationSyncDetail>).detail
      const peer = data?.peer || 'peer'
      const count = data?.count || 0
      notify(
        `Federation sync completed with ${peer}. Synchronized ${count} knowledge nodes.`,
        'info',
        'P2P Federation'
      )
    }

    window.addEventListener('nexus_event_dream_cycle_complete', handleDreamCycle)
    window.addEventListener('nexus_event_federation_sync', handleFederationSync)
    return () => {
      window.removeEventListener(
        'nexus_event_dream_cycle_complete',
        handleDreamCycle
      )
      window.removeEventListener(
        'nexus_event_federation_sync',
        handleFederationSync
      )
    }
  }, [notify])
}
