import React, { useState, useRef } from 'react';
import type { NexusBridge } from '../lib/nexus-bridge';
import { Key } from 'lucide-react';
import { cn } from '../lib/utils';
import { MaintenanceTab } from './MaintenanceTab';
import { ConnectorsTab } from './ConnectorsTab';

interface SettingsTabProps {
  bridge: NexusBridge | null;
  notify: (msg: string, type?: 'info' | 'error') => void;
  onOpenAuthModal: () => void;
}

type SubSection = 'connectors' | 'maintenance';

export function SettingsTab({ bridge, notify, onOpenAuthModal }: SettingsTabProps) {
  const [section, setSection] = useState<SubSection>('connectors');
  const sectionList: SubSection[] = ['connectors', 'maintenance'];
  const tabRefs = useRef<Record<SubSection, HTMLButtonElement | null>>({
    connectors: null,
    maintenance: null,
  });

  const handleTabKeyDown = (e: React.KeyboardEvent<HTMLButtonElement>, current: SubSection) => {
    const idx = sectionList.indexOf(current);
    let nextIdx = -1;
    if (e.key === 'ArrowRight') nextIdx = (idx + 1) % sectionList.length;
    else if (e.key === 'ArrowLeft') nextIdx = (idx - 1 + sectionList.length) % sectionList.length;
    else if (e.key === 'Home') nextIdx = 0;
    else if (e.key === 'End') nextIdx = sectionList.length - 1;
    if (nextIdx !== -1) {
      e.preventDefault();
      const nextSec = sectionList[nextIdx];
      setSection(nextSec);
      tabRefs.current[nextSec]?.focus();
    }
  };

  return (
    <div className="space-y-4 font-sans">
      {/* Titleband and Sovereign Tabs */}
      <div className="border border-border bg-card p-4 rounded-lg shadow-sm">
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 pb-3 border-b border-border">
          <div>
            <h2 className="text-base font-bold tracking-tight text-foreground">
              Ajustes y Conexión
            </h2>
            <p className="text-xs text-muted-foreground mt-0.5">
              Configuración del kernel Tylluan, integración para clientes MCP (Claude Desktop, Cursor, Code) y credenciales.
            </p>
          </div>

          <button
            type="button"
            onClick={onOpenAuthModal}
            className="flex items-center gap-1.5 px-3 py-1.5 bg-muted/60 hover:bg-muted border border-border text-foreground rounded-md text-xs font-semibold shadow-sm transition-colors cursor-pointer self-start sm:self-auto focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
            title="Configurar Bearer Token"
          >
            <Key className="w-3.5 h-3.5 text-primary" aria-hidden="true" />
            <span>Configurar Token</span>
          </button>
        </div>

        {/* Section Tabs */}
        <div role="tablist" aria-label="Secciones de ajustes" className="flex flex-wrap items-center gap-1.5 mt-3">
          {[
            { id: 'connectors' as SubSection, label: 'Conexión MCP' },
            { id: 'maintenance' as SubSection, label: 'Mantenimiento del Sistema' },
          ].map((tab) => {
            const isActive = section === tab.id;
            return (
              <button
                key={tab.id}
                ref={(el) => { tabRefs.current[tab.id] = el; }}
                role="tab"
                aria-selected={isActive}
                tabIndex={isActive ? 0 : -1}
                type="button"
                onClick={() => setSection(tab.id)}
                onKeyDown={(e) => handleTabKeyDown(e, tab.id)}
                className={cn(
                  "px-3 py-1.5 text-xs font-semibold border transition-all cursor-pointer rounded-md focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none",
                  isActive
                    ? "bg-primary/10 border-primary/30 text-primary shadow-sm"
                    : "bg-muted/50 border-border text-muted-foreground hover:bg-muted hover:text-foreground"
                )}
              >
                {tab.label}
              </button>
            );
          })}
        </div>
      </div>

      {/* Content */}
      <div className="flex-1 min-h-0">
        {section === 'connectors' ? (
          <ConnectorsTab notify={notify} />
        ) : (
          <MaintenanceTab bridge={bridge} notify={notify} />
        )}
      </div>
    </div>
  );
}
