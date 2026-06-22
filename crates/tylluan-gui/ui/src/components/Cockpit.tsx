import React, { useState } from 'react';
import ReactFlow, { 
  Background, 
  Controls, 
  useNodesState, 
  useEdgesState,
} from 'reactflow';
import 'reactflow/dist/style.css';
import { 
  Shield, 
  Activity, 
  Settings, 
  Database, 
  Image as ImageIcon, 
  Terminal,
  ChevronRight,
  Upload,
  FileText,
  CheckCircle,
  XCircle
} from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';

interface IngestResult {
  filename: string;
  status: 'success' | 'error';
  message?: string;
}

export const Cockpit: React.FC = () => {
  const [nodes, , onNodesChange] = useNodesState([]);
  const [edges, , onEdgesChange] = useEdgesState([]);
  const [activePanel, setActivePanel] = useState<'graph' | 'doctor' | 'memory' | 'superpowers' | 'ingest'>('graph');
  const [ingestResults, setIngestResults] = useState<IngestResult[]>([]);

  return (
    <div className="flex h-screen w-full overflow-hidden bg-background text-foreground">
      {/* Side Navigation (Glass HUD) */}
      <nav className="w-16 md:w-20 glass-panel !rounded-none border-y-0 border-l-0 flex flex-col items-center py-8 gap-8 z-50">
        <div className="w-10 h-10 bg-primary/20 rounded-lg flex items-center justify-center border border-primary/50 text-glow">
          <Shield className="text-primary" size={24} />
        </div>
        
        <div className="flex-1 flex flex-col gap-4">
          <NavIcon active={activePanel === 'graph'} onClick={() => setActivePanel('graph')} icon={<Activity size={22} />} label="Flow" />
          <NavIcon active={activePanel === 'doctor'} onClick={() => setActivePanel('doctor')} icon={<Terminal size={22} />} label="Doctor" />
          <NavIcon active={activePanel === 'memory'} onClick={() => setActivePanel('memory')} icon={<Database size={22} />} label="Memory" />
          <NavIcon active={activePanel === 'superpowers'} onClick={() => setActivePanel('superpowers')} icon={<ImageIcon size={22} />} label="Powers" />
          <NavIcon active={activePanel === 'ingest'} onClick={() => setActivePanel('ingest')} icon={<Upload size={22} />} label="Ingest" />
        </div>

        <NavIcon active={false} onClick={() => {}} icon={<Settings size={22} />} label="Config" />
      </nav>

      {/* Main Viewport */}
      <main className="flex-1 relative flex flex-col h-full">
        {/* Top Header */}
        <header className="h-16 px-8 flex items-center justify-between glass-panel !rounded-none border-x-0 border-t-0 bg-black/20 z-40">
          <div className="flex items-center gap-3">
            <span className="text-primary/50 font-mono text-sm">OS // TYLLUANNEXUS_O3</span>
            <ChevronRight size={14} className="text-white/20" />
            <span className="font-bold tracking-widest text-lg uppercase">{activePanel}</span>
          </div>
          
          <div className="flex items-center gap-6">
            <Metric label="RAM" value="1.2GB" color="text-primary" />
            <Metric label="LATENCY" value="12ms" color="text-green-400" />
            <div className="flex items-center gap-2">
              <div className="w-2 h-2 rounded-full bg-green-500 animate-pulse shadow-[0_0_8px_rgba(34,197,94,0.8)]" />
              <span className="text-xs font-mono text-green-500/80 uppercase">Kernel Online</span>
            </div>
          </div>
        </header>

        {/* Dynamic Canvas / Panels */}
        <div className="flex-1 relative">
          <AnimatePresence mode="wait">
            {activePanel === 'graph' && (
              <motion.div 
                key="graph"
                initial={{ opacity: 0, scale: 0.98 }}
                animate={{ opacity: 1, scale: 1 }}
                exit={{ opacity: 0, scale: 1.02 }}
                className="w-full h-full"
              >
                <ReactFlow
                  nodes={nodes}
                  edges={edges}
                  onNodesChange={onNodesChange}
                  onEdgesChange={onEdgesChange}
                  fitView
                >
                  <Background color="#ffffff05" gap={24} />
                  <Controls className="bg-glass border-glass-border !shadow-none" />
                </ReactFlow>
              </motion.div>
            )}
            {activePanel === 'ingest' && (
              <motion.div
                key="ingest"
                initial={{ opacity: 0, scale: 0.98 }}
                animate={{ opacity: 1, scale: 1 }}
                exit={{ opacity: 0, scale: 1.02 }}
                className="w-full h-full p-8 overflow-auto"
              >
                <IngestPanel
                  results={ingestResults}
                  onClear={() => setIngestResults([])}
                  onIngest={(batch) => setIngestResults(prev => [...prev, ...batch])}
                />
              </motion.div>
            )}
          </AnimatePresence>
        </div>
      </main>
    </div>
  );
};

const IngestPanel: React.FC<{
  results: IngestResult[];
  onClear: () => void;
  onIngest: (results: IngestResult[]) => void;
}> = ({ results, onClear, onIngest }) => {
  const [isDragging, setIsDragging] = React.useState(false);
  const [uploading, setUploading] = React.useState(false);

  const handleDragOver = (e: React.DragEvent) => { e.preventDefault(); setIsDragging(true); };
  const handleDragLeave = () => setIsDragging(false);

  const handleDrop = async (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(false);
    const files = Array.from(e.dataTransfer.files);
    if (files.length === 0) return;
    setUploading(true);
    const batch: IngestResult[] = [];
    for (const file of files) {
      const form = new FormData();
      form.append('file', file);
      try {
        const res = await fetch('http://localhost:3030/api/v1/ingest/upload', { method: 'POST', body: form });
        const data = await res.json() as { status?: string; result?: { content?: { text?: string }[] }[]; error?: string };
        batch.push({
          filename: file.name,
          status: data.status === 'ingested' ? 'success' : 'error',
          message: data.result?.[0]?.content?.[0]?.text ?? data.error,
        });
      } catch (err) {
        batch.push({ filename: file.name, status: 'error', message: String(err) });
      }
    }
    onIngest(batch);
    setUploading(false);
  };

  return (
    <div className="max-w-2xl mx-auto space-y-6">
      <div className="text-center mb-8">
        <h2 className="text-2xl font-bold text-primary mb-2">Ingest Files</h2>
        <p className="text-white/50">Drop files here to add them to SilvaDB</p>
      </div>
      <div
        onDragOver={handleDragOver}
        onDragLeave={handleDragLeave}
        onDrop={handleDrop}
        className={`border-2 border-dashed rounded-xl p-12 text-center transition-all ${
          isDragging ? 'border-primary bg-primary/10 scale-105' : 'border-white/20 hover:border-primary/50'
        }`}
      >
        {uploading ? (
          <div className="flex flex-col items-center gap-4">
            <div className="w-12 h-12 border-4 border-primary border-t-transparent rounded-full animate-spin" />
            <p className="text-primary">Ingesting files...</p>
          </div>
        ) : (
          <>
            <Upload className="w-16 h-16 mx-auto mb-4 text-white/30" />
            <p className="text-lg text-white/70 mb-2">{isDragging ? 'Drop files here' : 'Drag & drop files here'}</p>
            <p className="text-sm text-white/40">.md .txt .py .js .json .yaml .pdf...</p>
          </>
        )}
      </div>
      {results.length > 0 && (
        <div className="space-y-3">
          <div className="flex justify-between items-center">
            <h3 className="text-lg font-semibold text-white/80">Recent Uploads</h3>
            <button type="button" onClick={onClear} className="text-sm text-white/40 hover:text-white/70">Clear</button>
          </div>
          {results.map((r, i) => (
            <div key={i} className="flex items-center gap-3 p-3 bg-white/5 rounded-lg">
              {r.status === 'success'
                ? <CheckCircle className="w-5 h-5 text-green-400 flex-shrink-0" />
                : <XCircle className="w-5 h-5 text-red-400 flex-shrink-0" />}
              <FileText className="w-4 h-4 text-white/30 flex-shrink-0" />
              <span className="flex-1 text-white/80 truncate">{r.filename}</span>
              {r.message && <span className="text-xs text-white/40 max-w-xs truncate">{r.message}</span>}
            </div>
          ))}
        </div>
      )}
    </div>
  );
};

const NavIcon: React.FC<{ active: boolean, onClick: () => void, icon: React.ReactNode, label: string }> = ({ active, onClick, icon, label }) => (
  <button
    type="button"
    onClick={onClick}
    className={`group relative p-3 rounded-xl transition-all duration-300 ${active ? 'bg-primary/20 text-primary border border-primary/30' : 'text-white/40 hover:bg-white/5 hover:text-white/80'}`}
  >
    {icon}
    <span className="absolute left-full ml-4 px-2 py-1 bg-black text-white text-[10px] rounded opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none uppercase tracking-tighter whitespace-nowrap z-50">
      {label}
    </span>
    {active && <div className="absolute -left-1 top-1/2 -translate-y-1/2 w-1 h-4 bg-primary rounded-full shadow-[0_0_8px_#00f2fe]" />}
  </button>
);

const Metric: React.FC<{ label: string, value: string, color: string }> = ({ label, value, color }) => (
  <div className="flex flex-col items-end">
    <span className="text-[9px] font-mono text-white/30 uppercase tracking-widest">{label}</span>
    <span className={`text-sm font-bold font-mono ${color}`}>{value}</span>
  </div>
);
