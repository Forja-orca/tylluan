import React, { useState } from 'react';
import type { NexusBridge } from '../lib/nexus-bridge';
import { Settings, Key } from 'lucide-react';
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
    <div className="space-y-6">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 pb-3 border-b border-slate-800">
        <div>
          <h2 className="text-lg font-bold text-slate-100 flex items-center gap-2">
            <Settings className="w-5 h-5 text-emerald-400" aria-hidden="true" />
            Ajustes y Conexión
          </h2>
          <p className="text-xs text-slate-400 mt-0.5">
            Configuración del kernel Tylluan, snippets para clientes MCP (Claude Desktop, Cursor, Code) y credenciales.
          </p>
        </div>

        <div className="flex items-center gap-2">
          <div role="group" aria-label="Secciones de ajustes" className="flex items-center gap-1 bg-slate-900 border border-slate-800 rounded-lg p-1">
            <button
              type="button"
              onClick={() => setSection('connectors')}
              className={cn(
                "px-2.5 py-1 rounded text-xs font-medium transition-colors cursor-pointer",
                section === 'connectors' ? "bg-slate-800 text-slate-100 shadow-sm" : "text-slate-400 hover:text-slate-200"
              )}
            >
              Conexión MCP
            </button>
            <button
              type="button"
              onClick={() => setSection('maintenance')}
              className={cn(
                "px-2.5 py-1 rounded text-xs font-medium transition-colors cursor-pointer",
                section === 'maintenance' ? "bg-slate-800 text-slate-100 shadow-sm" : "text-slate-400 hover:text-slate-200"
              )}
            >
              Mantenimiento
            </button>
          </div>

          <button
            type="button"
            onClick={onOpenAuthModal}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg border bg-slate-900 border-slate-800 text-slate-300 hover:text-amber-400 hover:border-amber-500/30 text-xs font-medium transition-colors cursor-pointer"
            title="Configurar Bearer Token"
          >
            <Key className="w-3.5 h-3.5" />
            <span>Token</span>
          </button>
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
