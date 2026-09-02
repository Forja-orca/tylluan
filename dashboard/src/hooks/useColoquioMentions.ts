import { useEffect } from 'react'
import { useAppStore } from '../stores/useAppStore'

interface MentionDetail {
  agent_id?: string
  channel?: string
  message?: string
  sender?: string
}

interface UseColoquioMentionsOptions {
  notify: (msg: string, type?: 'info' | 'error', guild?: string) => void
}

export function useColoquioMentions({ notify }: UseColoquioMentionsOptions) {
  const { addMention, setColoquioUnread } = useAppStore()

  useEffect(() => {
    const handleMention = (e: Event) => {
      const rawDetail = (e as CustomEvent<unknown>).detail
      if (!rawDetail || typeof rawDetail !== 'object') return
      const { agent_id, channel = 'coloquio', message = '', sender = 'someone' } = rawDetail as MentionDetail

      if (
        agent_id === 'jose' ||
        agent_id === 'antigravity' ||
        agent_id === 'all'
      ) {
        const fullMsg = `${sender} in #${channel}: "${message}"`
        notify(fullMsg, 'info', 'Mention Received')

        addMention({ sender, channel, message })
        setColoquioUnread((prev) => prev + 1)
      }
    }

    window.addEventListener('nexus_mention', handleMention)
    return () => {
      window.removeEventListener('nexus_mention', handleMention)
    }
  }, [addMention, notify, setColoquioUnread])
}
