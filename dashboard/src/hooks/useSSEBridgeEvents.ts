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
        `Ciclo de consolidación cognitiva (NREM) finalizado. Duplicados fusionados: ${data?.duplicates_merged ?? 0}, clústeres consolidados: ${data?.clusters_consolidated ?? 0}, nodos decaídos: ${data?.nodes_decayed ?? 0}.`,
        'info',
        'Consolidación NREM'
      )
    }

    const handleFederationSync = (e: Event) => {
      const data = (e as CustomEvent<FederationSyncDetail>).detail
      const peer = data?.peer || 'un par'
      const count = data?.count || 0
      notify(
        `Sincronización de federación completada con ${peer}. Sincronizados ${count} nodos de conocimiento.`,
        'info',
        'Federación P2P'
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
