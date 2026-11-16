import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import React from 'react';
import { ColoquioChannelsPanel } from '../ColoquioChannelsPanel';

describe('ColoquioChannelsPanel — Navegación por teclado', () => {
  it('permite enfocar y seleccionar canales con botón nativo accesible por Tab', () => {
    const selectChannel = vi.fn();
    render(
      <ColoquioChannelsPanel
        channels={[
          { channel_id: 'general', name: 'general', created_at: 0, message_count: 1, last_turn: 1 },
          { channel_id: 'equipo', name: 'equipo', created_at: 0, message_count: 1, last_turn: 1 },
        ]}
        selectedId="general"
        searchQuery=""
        setSearchQuery={vi.fn()}
        collapsedGroups={{}}
        setCollapsedGroups={vi.fn()}
        unreadMap={new Map()}
        selectChannel={selectChannel}
        showNewChannel={false}
        setShowNewChannel={vi.fn()}
        newChannelId=""
        setNewChannelId={vi.fn()}
        newChannelName=""
        setNewChannelName={vi.fn()}
        createChannel={vi.fn()}
        creating={false}
        onDeleteChannel={vi.fn()}
      />
    );

    const equipoBtn = screen.getByRole('button', { name: /^equipo$/i });
    expect(equipoBtn).toBeInTheDocument();
    expect(equipoBtn.className).toContain('focus-visible:ring-2');
    fireEvent.click(equipoBtn);
    expect(selectChannel).toHaveBeenCalledWith('equipo');
  });
});
