import React, { useState, useEffect } from 'react';
import type { NexusBridge } from '../lib/nexus-bridge';
import { RefreshCw, AlertTriangle, Cpu, ShieldCheck, Globe, Database } from 'lucide-react';
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

  const tabs: { id: SubTab; label: string; icon: React.ElementType }[] = [
    { id: 'inference', label: 'Inferencia Local', icon: Cpu },
    { id: 'roles', label: 'Asignación de Roles', icon: ShieldCheck },
    { id: 'external', label: 'Proveedores Externos', icon: Globe },
    { id: 'embeddings', label: 'Embeddings y Disco', icon: Database },
  ];

  return (
    <div className="space-y-4">
      {/* Sub Navigation */}
      <div className="flex items-center gap-2 p-1 bg-muted/60 border border-border rounded w-max">
        {tabs.map((tab) => {
          const Icon = tab.icon;
          return (
            <button
              key={tab.id}
              type="button"
              onClick={() => setSubTab(tab.id)}
              className={cn(
                "flex items-center gap-2 px-4 py-2 rounded text-xs font-bold transition-colors cursor-pointer",
                subTab === tab.id
                  ? "bg-card text-foreground"
                  : "text-muted-foreground hover:text-foreground hover:bg-muted/40"
              )}
            >
              <Icon className="w-4 h-4 text-primary" />
              {tab.label}
            </button>
          );
        })}
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
        <div className="fixed inset-0 flex items-center justify-center bg-black/60 z-50 p-4">
          <div className="bg-card border border-border rounded p-6 max-w-md w-full shadow-none space-y-4">
            <div className="flex items-center gap-3 text-primary">
              <AlertTriangle className="w-6 h-6" />
              <h3 className="text-lg font-bold text-foreground font-mono">Reinicio Requerido</h3>
            </div>
            
            <p className="text-sm text-muted-foreground leading-relaxed">
              La aceleración por hardware se ha configurado a <span className="font-mono text-primary font-semibold">{selectedDevice}</span>. Para cargar los proveedores de ejecución y aplicar los cambios, el Kernel de Tylluan debe reiniciarse.
            </p>

            <div className="bg-background border border-border rounded p-3 space-y-2 text-xs text-muted-foreground font-mono">
              <p className="text-foreground font-semibold text-[11px] mb-1">Instrucciones de reinicio:</p>
              <p>1. Detén el kernel en tu terminal (<kbd className="bg-muted px-1.5 py-0.5 rounded text-foreground">Ctrl + C</kbd>).</p>
              <p>2. Vuelve a iniciarlo ejecutando:</p>
              <div className="bg-card p-2 rounded border border-border text-foreground">
                tylluan-cli start
              </div>
            </div>

            <div className="flex justify-end gap-2 pt-2">
              <button
                type="button"
                onClick={() => setShowRestartModal(false)}
                className="px-4 py-2 bg-primary text-primary-foreground rounded text-xs font-semibold hover:opacity-90 transition-opacity cursor-pointer"
              >
                Entendido
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
