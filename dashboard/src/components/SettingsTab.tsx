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
    <div className="space-y-3 font-sans">
      {/* Titleband and Property Sheet Tabs */}
      <div className="border border-[#ACA899] dark:border-border bg-[#ECE9D8] dark:bg-card p-2.5 rounded-sm">
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 pb-2 border-b border-[#ACA899] dark:border-border">
          <div>
            <h2 className="text-xs font-bold text-[#01285F] dark:text-foreground">
              Ajustes y Conexión
            </h2>
            <p className="text-[11px] text-[#444444] dark:text-muted-foreground mt-0.5">
              Configuración del kernel Tylluan, integración para clientes MCP (Claude Desktop, Cursor, Code) y credenciales.
            </p>
          </div>

          <button
            type="button"
            onClick={onOpenAuthModal}
            className="flex items-center gap-1.5 px-3 py-1 bg-white hover:bg-[#F5F4EA] border border-[#ACA899] text-[#000000] dark:text-foreground rounded-sm text-[11px] font-bold transition-colors cursor-pointer self-start sm:self-auto"
            title="Configurar Bearer Token"
          >
            <Key className="w-3.5 h-3.5 text-[#0A246A]" aria-hidden="true" />
            <span>Configurar Token</span>
          </button>
        </div>

        {/* Property Sheet Tabs */}
        <div role="tablist" aria-label="Secciones de ajustes" className="flex flex-wrap items-center gap-1 mt-2 pt-1 border-t border-[#ACA899] dark:border-border">
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
                  "px-3 py-1 text-[11px] font-bold border transition-all cursor-pointer rounded-t-sm -mb-px",
                  isActive
                    ? "bg-white dark:bg-card border-[#ACA899] dark:border-border border-b-transparent text-[#01285F] dark:text-primary shadow-none"
                    : "bg-[#D8D4C4] dark:bg-muted/40 border-transparent text-[#555555] dark:text-muted-foreground hover:bg-[#EAE7DC] hover:text-[#000000]"
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
