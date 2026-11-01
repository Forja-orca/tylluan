import React, { useState, useEffect } from 'react';
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

  useEffect(() => {
    const loadData = async () => {
      if (!bridge) return;
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
    <div className="space-y-3 font-sans">
      {/* Titleband and Property Sheet Tabs */}
      <div className="border border-[#ACA899] dark:border-border bg-[#ECE9D8] dark:bg-card p-2.5 rounded-sm">
        <div className="pb-2 border-b border-[#ACA899] dark:border-border">
          <h2 className="text-xs font-bold text-[#01285F] dark:text-foreground">
            Modelos y Aceleración de Hardware
          </h2>
          <p className="text-[11px] text-[#444444] dark:text-muted-foreground mt-0.5">
            Configuración del motor de inferencia local, asignación de roles a agentes, embeddings vectoriales y proveedores externos.
          </p>
        </div>

        {/* Property Sheet Tabs */}
        <div role="tablist" aria-label="Secciones de modelos" className="flex flex-wrap items-center gap-1 mt-2 pt-1 border-t border-[#ACA899] dark:border-border">
          {tabs.map((tab) => {
            const isActive = subTab === tab.id;
            return (
              <button
                key={tab.id}
                role="tab"
                aria-selected={isActive}
                type="button"
                onClick={() => setSubTab(tab.id)}
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
        <div className="fixed inset-0 flex items-center justify-center bg-black/60 z-50 p-4 font-sans">
          <div className="bg-[#ECE9D8] dark:bg-card border-2 border-[#ACA899] dark:border-border rounded-none p-4 max-w-md w-full shadow-none space-y-3">
            <div className="flex items-center gap-2 pb-2 border-b border-[#ACA899] dark:border-border">
              <AlertTriangle className="w-4 h-4 text-[#B8860B]" aria-hidden="true" />
              <h3 className="text-xs font-bold text-[#01285F] dark:text-foreground">Reinicio Requerido</h3>
            </div>
            
            <p className="text-[11px] text-[#444444] dark:text-muted-foreground leading-normal">
              La aceleración por hardware se ha configurado a <span className="font-mono text-[#9E4024] dark:text-primary font-bold">{selectedDevice}</span>. Para cargar los proveedores de ejecución y aplicar los cambios, el Kernel de Tylluan debe reiniciarse.
            </p>

            <fieldset className="bg-white dark:bg-muted/40 border border-[#ACA899] dark:border-border p-2.5 rounded-none space-y-1.5 text-[11px] text-[#444444] dark:text-muted-foreground">
              <legend className="text-[11px] font-bold text-[#01285F] dark:text-foreground px-1">Instrucciones de reinicio</legend>
              <p>1. Detenga el kernel en su terminal (<kbd className="bg-[#ECE9D8] px-1 py-0.5 border border-[#ACA899] text-[#000000] font-mono text-[10px]">Ctrl + C</kbd>).</p>
              <p>2. Vuelva a iniciarlo ejecutando:</p>
              <div className="bg-[#F5F4EA] dark:bg-card p-1.5 border border-[#ACA899] dark:border-border text-[#000000] dark:text-foreground font-mono text-[11px]">
                tylluan-cli start
              </div>
            </fieldset>

            <div className="flex justify-end gap-2 pt-1 border-t border-[#ACA899] dark:border-border">
              <button
                type="button"
                onClick={() => setShowRestartModal(false)}
                className="px-3 py-1 bg-white hover:bg-[#F5F4EA] text-[#000000] dark:text-foreground border border-[#ACA899] rounded-sm text-[11px] font-bold transition-colors cursor-pointer"
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
