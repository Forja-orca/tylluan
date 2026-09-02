import React, { useState, useEffect } from 'react';
import type { NexusBridge } from '../lib/nexus-bridge';
import { RefreshCw, AlertTriangle, Cpu, ShieldCheck, Globe, Database } from 'lucide-react';
import { cn } from '../lib/utils';
import { ModelsLocalInference } from './ModelsLocalInference';
import { ModelsRoleAssignment } from './ModelsRoleAssignment';
import { ModelsExternalProviders } from './ModelsExternalProviders';
import { ModelsEmbeddingsDisk } from './ModelsEmbeddingsDisk';

interface Props {
  bridge: NexusBridge | null;
}

type SubTab = 'inference' | 'roles' | 'external' | 'embeddings';

export function ModelsTab({ bridge }: Props) {
  const [subTab, setSubTab] = useState<SubTab>('inference');
  const [config, setConfig] = useState<any>(null);
  const [models, setModels] = useState<any>(null);
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
    { id: 'inference', label: 'Local Inference', icon: Cpu },
    { id: 'roles', label: 'Role Assignment', icon: ShieldCheck },
    { id: 'external', label: 'External Providers', icon: Globe },
    { id: 'embeddings', label: 'Embeddings & Disk', icon: Database },
  ];

  return (
    <div className="space-y-4">
      {/* Sub Navigation */}
      <div className="flex items-center gap-2 p-1 bg-slate-900 border border-slate-800 rounded-xl w-max">
        {tabs.map((tab) => {
          const Icon = tab.icon;
          return (
            <button
              key={tab.id}
              type="button"
              onClick={() => setSubTab(tab.id)}
              className={cn(
                "flex items-center gap-2 px-4 py-2 rounded-lg text-xs font-bold transition-colors",
                subTab === tab.id
                  ? "bg-slate-800 text-slate-200 shadow-sm"
                  : "text-slate-500 hover:text-slate-300 hover:bg-slate-800/50"
              )}
            >
              <Icon className="w-4 h-4" />
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
        <div className="fixed inset-0 flex items-center justify-center bg-black/70 backdrop-blur-sm z-50 p-4">
          <div className="bg-slate-900 border border-slate-800 rounded-xl p-6 max-w-md w-full shadow-2xl space-y-4">
            <div className="flex items-center gap-3 text-amber-400">
              <AlertTriangle className="w-6 h-6" />
              <h3 className="text-lg font-bold text-slate-200 font-mono">Restart Required</h3>
            </div>
            
            <p className="text-sm text-slate-300 leading-relaxed">
              Hardware acceleration has been configured to <span className="font-mono text-amber-400 font-semibold">{selectedDevice}</span>. To load the appropriate Execution Providers and apply the changes, the Tylluan Nexus Kernel must be restarted.
            </p>

            <div className="bg-slate-950 border border-slate-800 rounded-lg p-3 space-y-2 text-xs text-slate-400 font-mono">
              <p className="text-slate-300 font-semibold text-[10px] mb-1">Restart instructions:</p>
              <p>1. Stop the current kernel in your terminal (press <kbd className="bg-slate-800 px-1.5 py-0.5 rounded text-slate-300">Ctrl + C</kbd>).</p>
              <p>2. Start it again by running:</p>
              <div className="bg-slate-900 p-2 rounded border border-slate-800 text-slate-300">
                .\tylluan-mcp.bat
              </div>
            </div>

            <div className="flex justify-end gap-2 pt-2">
              <button
                type="button"
                onClick={() => setShowRestartModal(false)}
                className="px-4 py-2 bg-slate-800 hover:bg-slate-700 text-slate-300 rounded-lg text-xs font-semibold transition-colors"
              >
                Got it
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
