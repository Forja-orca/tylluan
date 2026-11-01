import React, { useState, useEffect, useRef } from 'react';
import type { NexusBridge } from '../lib/nexus-bridge';
import { RefreshCw, AlertTriangle } from 'lucide-react';
import { cn } from '../lib/utils';
import { ModelsLocalInference } from './ModelsLocalInference';
import { ModelsRoleAssignment } from './ModelsRoleAssignment';
import { ModelsExternalProviders } from './ModelsExternalProviders';
import { ModelsEmbeddingsDisk, type ModelsConfigData, type ModelsResponseData } from './ModelsEmbeddingsDisk';

interface Props {
  bridge: NexusBridge | null;
}

type SubTab = 'inference' | 'roles' | 'external' | 'embeddings';

export function ModelsTab({ bridge }: Props) {
  const [subTab, setSubTab] = useState<SubTab>('inference');
  const [config, setConfig] = useState<ModelsConfigData | null>(null);
  const [models, setModels] = useState<ModelsResponseData | null>(null);
  const [loading, setLoading] = useState(true);
  const [selectedDevice, setSelectedDevice] = useState('cpu');
  const [initialDevice, setInitialDevice] = useState('cpu');
  const [showRestartModal, setShowRestartModal] = useState(false);
  const restartModalBtnRef = useRef<HTMLButtonElement>(null);
  const tabRefs = useRef<Record<SubTab, HTMLButtonElement | null>>({
    inference: null,
    roles: null,
    external: null,
    embeddings: null,
  });

  const tabList: SubTab[] = ['inference', 'roles', 'external', 'embeddings'];

  const handleTabKeyDown = (e: React.KeyboardEvent<HTMLButtonElement>, current: SubTab) => {
    const idx = tabList.indexOf(current);
    let nextIdx = -1;
    if (e.key === 'ArrowRight') nextIdx = (idx + 1) % tabList.length;
    else if (e.key === 'ArrowLeft') nextIdx = (idx - 1 + tabList.length) % tabList.length;
    else if (e.key === 'Home') nextIdx = 0;
    else if (e.key === 'End') nextIdx = tabList.length - 1;
    if (nextIdx !== -1) {
      e.preventDefault();
      const nextTab = tabList[nextIdx];
      setSubTab(nextTab);
      tabRefs.current[nextTab]?.focus();
    }
  };

  useEffect(() => {
    if (showRestartModal) {
      const timer = setTimeout(() => {
        restartModalBtnRef.current?.focus();
      }, 50);
      return () => clearTimeout(timer);
    }
  }, [showRestartModal]);

  useEffect(() => {
    const loadData = async () => {
      if (!bridge) {
        setLoading(false);
        return;
      }
      setLoading(true);
      try {
        const cfg = await bridge.getConfig();
        setConfig(cfg);

        const dev = ((cfg?.inference?.device) || 'cpu') as string;
        setSelectedDevice(dev);
        setInitialDevice(dev);

        try {
          const m = await bridge.fetchRaw('/api/v1/models');
          setModels(m);
        } catch (err) {
          console.warn('Failed fetching models:', err);
        }
      } catch (e) {
        console.error('Failed to load config/models', e);
      }
      setLoading(false);
    };
    loadData();
  }, [bridge]);

  const handleDeviceChange = (device: string) => {
    setSelectedDevice(device);
  };

  const handleRestartRequired = () => {
    setInitialDevice(selectedDevice);
    setShowRestartModal(true);
  };

  if (loading) {
    return (
      <div className="p-8 flex items-center justify-center">
        <RefreshCw className="w-6 h-6 animate-spin text-slate-500" />
      </div>
    );
  }

  const tabs: { id: SubTab; label: string }[] = [
    { id: 'inference', label: 'Inferencia Local' },
    { id: 'roles', label: 'Asignación de Roles' },
    { id: 'external', label: 'Proveedores Externos' },
    { id: 'embeddings', label: 'Embeddings y Disco' },
  ];

  return (
    <div className="space-y-4 font-sans">
      {/* Titleband and Sovereign Tabs */}
      <div className="border border-border bg-card p-4 rounded-lg shadow-sm">
        <div className="pb-3 border-b border-border">
          <h2 className="text-base font-bold tracking-tight text-foreground">
            Modelos y Aceleración de Hardware
          </h2>
          <p className="text-xs text-muted-foreground mt-0.5">
            Configuración del motor de inferencia local, asignación de roles a agentes, embeddings vectoriales y proveedores externos.
          </p>
        </div>

        {/* Section Tabs */}
        <div role="tablist" aria-label="Secciones de modelos" className="flex flex-wrap items-center gap-1.5 mt-3">
          {tabs.map((tab) => {
            const isActive = subTab === tab.id;
            return (
              <button
                key={tab.id}
                ref={(el) => { tabRefs.current[tab.id] = el; }}
                role="tab"
                aria-selected={isActive}
                tabIndex={isActive ? 0 : -1}
                type="button"
                onClick={() => setSubTab(tab.id)}
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

      {/* Tab Panels */}
      <div className="flex-1 min-h-0">
        {subTab === 'inference' && (
          <ModelsLocalInference
            bridge={bridge}
            config={config}
            models={models}
            selectedDevice={selectedDevice}
            initialDevice={initialDevice}
            onDeviceChange={handleDeviceChange}
            onRestartRequired={handleRestartRequired}
          />
        )}
        {subTab === 'roles' && (
          <ModelsRoleAssignment
            bridge={bridge}
            models={models}
          />
        )}
        {subTab === 'external' && (
          <ModelsExternalProviders
            bridge={bridge}
          />
        )}
        {subTab === 'embeddings' && (
          <ModelsEmbeddingsDisk
            bridge={bridge}
            config={config}
            models={models}
          />
        )}
      </div>

      {/* Restart Kernel Modal (shared across all sub-tabs) */}
      {showRestartModal && (
        <div
          role="alertdialog"
          aria-modal="true"
          aria-labelledby="restart-modal-title"
          className="fixed inset-0 flex items-center justify-center bg-black/60 z-50 p-4 font-sans"
          onKeyDown={(e) => {
            if (e.key === 'Escape') {
              e.stopPropagation();
              setShowRestartModal(false);
            }
          }}
        >
          <div className="bg-card border border-border rounded-lg p-5 max-w-md w-full shadow-lg space-y-3">
            <div className="flex items-center gap-2 pb-2.5 border-b border-border">
              <AlertTriangle className="w-4 h-4 text-amber-600 dark:text-amber-400" aria-hidden="true" />
              <h3 id="restart-modal-title" className="text-sm font-bold text-foreground">Reinicio Requerido</h3>
            </div>
            
            <p className="text-xs text-muted-foreground leading-normal">
              La aceleración por hardware se ha configurado a <span className="font-mono text-primary font-bold">{selectedDevice}</span>. Para cargar los proveedores de ejecución y aplicar los cambios, el Kernel de Tylluan debe reiniciarse.
            </p>

            <fieldset className="bg-muted/40 border border-border p-3 rounded-md space-y-1.5 text-xs text-muted-foreground">
              <legend className="text-xs font-bold text-foreground px-1">Instrucciones de reinicio</legend>
              <p>1. Detenga el kernel en su terminal (<kbd className="bg-muted px-1.5 py-0.5 rounded border border-border text-foreground font-mono text-[10px]">Ctrl + C</kbd>).</p>
              <p>2. Vuelva a iniciarlo ejecutando:</p>
              <div className="bg-card p-2 rounded-md border border-border text-foreground font-mono text-xs">
                tylluan-cli start
              </div>
            </fieldset>

            <div className="flex justify-end gap-2 pt-2 border-t border-border">
              <button
                ref={restartModalBtnRef}
                type="button"
                onClick={() => setShowRestartModal(false)}
                className="px-3.5 py-1.5 bg-muted/60 hover:bg-muted text-foreground border border-border rounded-md text-xs font-semibold shadow-sm transition-colors cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
              >
                Aceptar
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
