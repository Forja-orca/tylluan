import { Search, ChevronRight, ChevronDown, Plus, Hash, Trash2, AlertTriangle } from 'lucide-react';
import { useMemo, useState, useEffect, useRef } from 'react';
import { cn } from '../lib/utils';
import { ColoquioChannel } from './coloquio-types';

interface ColoquioChannelsPanelProps {
  channels: ColoquioChannel[];
  selectedId: string | null;
  searchQuery: string;
  setSearchQuery: (q: string) => void;
  collapsedGroups: Record<string, boolean>;
  setCollapsedGroups: React.Dispatch<React.SetStateAction<Record<string, boolean>>>;
  unreadMap: Map<string, number>;
  selectChannel: (id: string) => void;
  showNewChannel: boolean;
  setShowNewChannel: (b: boolean) => void;
  newChannelId: string;
  setNewChannelId: (q: string) => void;
  newChannelName: string;
  setNewChannelName: (q: string) => void;
  createChannel: () => void;
  creating: boolean;
  onDeleteChannel: (channelId: string, archive: boolean) => Promise<void>;
}

export function ColoquioChannelsPanel({
  channels,
  selectedId,
  searchQuery,
  setSearchQuery,
  collapsedGroups,
  setCollapsedGroups,
  unreadMap,
  selectChannel,
  showNewChannel,
  setShowNewChannel,
  newChannelId,
  setNewChannelId,
  newChannelName,
  setNewChannelName,
  createChannel,
  creating,
  onDeleteChannel
}: ColoquioChannelsPanelProps) {
  const [deleteTarget, setDeleteTarget] = useState<ColoquioChannel | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        const active = document.activeElement;
        const target = e.target as Node;
        const isInside = containerRef.current && (
          containerRef.current.contains(active) || containerRef.current.contains(target)
        );
        if (!isInside) return;

        if (deleteTarget) {
          setDeleteTarget(null);
        } else if (showNewChannel) {
          setShowNewChannel(false);
        }
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [deleteTarget, showNewChannel, setShowNewChannel]);
  const [archiveOnDelete, setArchiveOnDelete] = useState(true);
  const [deleting, setDeleting] = useState(false);

  const filteredChannels = useMemo(() =>
    channels.filter(ch => ch.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
      ch.channel_id.toLowerCase().includes(searchQuery.toLowerCase())), [channels, searchQuery]);

  const groupedChannels = useMemo(() => {
    const g: Record<string, ColoquioChannel[]> = { Diarios: [], Misiones: [], 'Debates privados': [], Canales: [] };
    filteredChannels.forEach(ch => {
      if (ch.channel_id.startsWith('daily-')) g['Diarios'].push(ch);
      else if (ch.channel_id.startsWith('mision-')) g['Misiones'].push(ch);
      else if (ch.channel_id.startsWith('private-') || ch.channel_id.startsWith('agent-')) g['Debates privados'].push(ch);
      else g['Canales'].push(ch);
    });
    return g;
  }, [filteredChannels]);

  const handleDeleteConfirm = async () => {
    if (!deleteTarget || deleting) return;
    setDeleting(true);
    try {
      await onDeleteChannel(deleteTarget.channel_id, archiveOnDelete);
      setDeleteTarget(null);
    } catch (err) {
      console.error('Error deleting channel:', err);
    } finally {
      setDeleting(false);
    }
  };

  const renderChannelItem = (ch: ColoquioChannel) => {
    const u = unreadMap.get(ch.channel_id) ?? 0;
    const active = selectedId === ch.channel_id;
    return (
      <div key={ch.channel_id} onClick={() => selectChannel(ch.channel_id)}
        className={cn('group/ch relative flex items-center gap-2 px-2.5 py-1 cursor-pointer rounded-none mx-0.5 transition-colors',
          active ? 'bg-white dark:bg-muted/80 text-[#0A246A] dark:text-primary border-l-2 border-[#0A246A] font-bold' : 'text-[#444444] dark:text-muted-foreground hover:text-foreground hover:bg-white/60 dark:hover:bg-muted/40 font-medium')}>
        <Hash className="w-3.5 h-3.5 shrink-0 opacity-60" />
        <span className="flex-1 text-[11px] truncate">{ch.name}</span>
        {u > 0 && <span className="px-1.5 py-0.2 rounded-none border border-amber-500/40 bg-amber-500/10 text-amber-700 dark:text-amber-400 text-[10px] font-bold leading-none">{u}</span>}
        <button
          type="button"
          onClick={e => { e.stopPropagation(); setDeleteTarget(ch); }}
          aria-label={`Eliminar canal ${ch.name}`}
          title="Eliminar canal"
          className="opacity-0 group-hover/ch:opacity-100 p-0.5 text-muted-foreground hover:text-rose-500 transition-all rounded-none cursor-pointer focus-visible:outline-1 focus-visible:outline-[#0A246A] focus:opacity-100"
        >
          <Trash2 className="w-3 h-3" />
        </button>
      </div>
    );
  };

  return (
    <div ref={containerRef} className="w-full md:w-52 shrink-0 border-r border-[#ACA899] dark:border-border flex flex-col bg-[#ECE9D8] dark:bg-card overflow-hidden h-full font-sans">
      <div className="flex items-center gap-2 px-2.5 py-1.5 border-b border-[#ACA899] dark:border-border bg-white dark:bg-muted">
        <Search className="w-3.5 h-3.5 text-[#555555] dark:text-muted-foreground shrink-0" />
        <input
          className="flex-1 bg-transparent text-[11px] text-[#000000] dark:text-foreground placeholder:text-[#888888] focus:outline-none"
          placeholder="Buscar canales..."
          aria-label="Buscar canales"
          value={searchQuery}
          onChange={e => setSearchQuery(e.target.value)}
        />
      </div>
      <div className="flex-1 overflow-y-auto py-1">
        {Object.entries(groupedChannels).map(([group, chs]) => (
          <div key={group}>
            <button
              type="button"
              onClick={() => setCollapsedGroups(p => ({ ...p, [group]: !p[group] }))}
              aria-label={`Alternar grupo ${group}`}
              className="w-full flex items-center gap-1.5 px-2 py-1 text-[11px] font-bold text-[#01285F] dark:text-muted-foreground hover:text-foreground transition-colors cursor-pointer focus-visible:outline-1 focus-visible:outline-[#0A246A] rounded-none"
            >
              {collapsedGroups[group] ? <ChevronRight className="w-2.5 h-2.5" /> : <ChevronDown className="w-2.5 h-2.5" />}
              {group}<span className="ml-auto text-[#666666] font-mono text-[10px]">{chs.length}</span>
            </button>
            {!collapsedGroups[group] && chs.map(renderChannelItem)}
          </div>
        ))}
        {filteredChannels.length === 0 && <div className="px-3 py-4 text-center text-[11px] text-[#666666] italic">Sin canales</div>}
      </div>
      <div className="border-t border-[#ACA899] dark:border-border p-1.5 bg-[#ECE9D8] dark:bg-card">
        {showNewChannel ? (
          <div className="flex flex-col gap-1 p-1 bg-white dark:bg-card border border-[#ACA899] rounded-sm">
            <input className="bg-white dark:bg-card border border-[#7F9DB9] rounded-none px-2 py-0.5 text-[11px] text-foreground placeholder:text-muted-foreground focus:outline-none w-full"
              placeholder="Identificador (ej. canal-01)" value={newChannelId} autoFocus
              onChange={e => setNewChannelId(e.target.value)} onKeyDown={e => {
                if (e.key === 'Enter') createChannel();
                if (e.key === 'Escape') setShowNewChannel(false);
              }} />
            <input className="bg-white dark:bg-card border border-[#7F9DB9] rounded-none px-2 py-0.5 text-[11px] text-foreground placeholder:text-muted-foreground focus:outline-none w-full"
              placeholder="Nombre visible (opcional)" value={newChannelName}
              onChange={e => setNewChannelName(e.target.value)} onKeyDown={e => {
                if (e.key === 'Enter') createChannel();
                if (e.key === 'Escape') setShowNewChannel(false);
              }} />
            <div className="flex gap-1 mt-0.5">
              <button
                type="button"
                onClick={() => setShowNewChannel(false)}
                className="flex-1 py-0.5 text-[11px] text-foreground bg-[#ECE9D8] hover:bg-[#F5F4EA] border border-[#ACA899] rounded-sm transition-colors cursor-pointer"
              >
                Cancelar
              </button>
              <button
                type="button"
                onClick={createChannel}
                disabled={creating || !newChannelId.trim()}
                className="flex-1 py-0.5 text-[11px] bg-[#0A246A] hover:bg-[#081C52] disabled:opacity-40 text-white font-bold rounded-sm transition-colors cursor-pointer"
              >
                {creating ? '...' : 'Crear'}
              </button>
            </div>
          </div>
        ) : (
          <button
            type="button"
            onClick={() => setShowNewChannel(true)}
            className="w-full flex items-center justify-center gap-1.5 px-2 py-1 text-[11px] font-bold text-[#000000] dark:text-foreground bg-white dark:bg-muted border border-[#ACA899] dark:border-border hover:bg-[#F5F4EA] rounded-sm transition-colors cursor-pointer"
          >
            <Plus className="w-3.5 h-3.5" /> Nuevo canal
          </button>
        )}
      </div>

      {/* Delete modal overlay */}
      {deleteTarget && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <div className="bg-[#ECE9D8] dark:bg-card border border-[#ACA899] dark:border-border rounded-sm p-4 w-96 flex flex-col gap-3 font-sans shadow-none">
            <div className="flex items-center gap-2 text-rose-700">
              <AlertTriangle className="w-4 h-4" />
              <span className="font-bold text-xs">Confirmar eliminación de canal</span>
            </div>
            <p className="text-[11px] text-foreground">
              ¿Desea eliminar el canal <span className="font-mono text-[#0A246A] font-bold">#{deleteTarget.name}</span> ({deleteTarget.message_count} mensajes)? Esta acción no se puede deshacer.
            </p>
            <label className="flex items-center gap-2 cursor-pointer text-[11px] text-foreground">
              <input type="checkbox" checked={archiveOnDelete} onChange={e => setArchiveOnDelete(e.target.checked)} className="rounded-none accent-[#0A246A]" />
              Archivar en memoria SilvaDB antes de borrar
            </label>
            <div className="flex gap-2 justify-end mt-1">
              <button
                type="button"
                onClick={() => setDeleteTarget(null)}
                className="px-3 py-1 text-[11px] font-bold text-foreground bg-white border border-[#ACA899] rounded-sm transition-colors cursor-pointer"
              >
                Cancelar
              </button>
              <button
                type="button"
                onClick={handleDeleteConfirm}
                disabled={deleting}
                className="px-3 py-1 text-[11px] font-bold text-white bg-rose-700 hover:bg-rose-800 disabled:opacity-50 border border-rose-900 rounded-sm transition-colors cursor-pointer"
              >
                {deleting ? 'Eliminando...' : 'Eliminar canal'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
