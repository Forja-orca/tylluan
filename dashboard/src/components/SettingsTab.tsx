import React, { useState } from 'react';
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
                role="tab"
                aria-selected={isActive}
                type="button"
                onClick={() => setSection(tab.id)}
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
