'use client';

import { useState } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { ArchitectureMap } from '@/components/architecture/architecture-map';
import { FsrsModel } from '@/components/architecture/fsrs-model';
import { RetrievalPipeline } from '@/components/architecture/retrieval-pipeline';
import { FederationMesh } from '@/components/architecture/federation-mesh';
import { SleepCycle } from '@/components/architecture/sleep-cycle';
import { DispatchFlow } from '@/components/architecture/dispatch-flow';
import { Roadmap } from '@/components/architecture/roadmap';
import {
  Network,
  Brain,
  Search,
  Radio,
  Moon,
  Route,
  Map,
  ChevronRight,
  Activity,
  Cpu,
  Shield,
  GitBranch,
} from 'lucide-react';

const TABS = [
  { id: 'architecture', label: 'Arquitectura', icon: Network, description: 'Mapa general del sistema' },
  { id: 'fsrs', label: 'FSRS', icon: Brain, description: 'Modelo de memoria espaciada' },
  { id: 'retrieval', label: 'Retrieval', icon: Search, description: 'Pipeline de recuperación' },
  { id: 'federation', label: 'Federación', icon: Radio, description: 'Mesh P2P y sincronización' },
  { id: 'sleep', label: 'SleepCycle', icon: Moon, description: 'Consolidación de memoria' },
  { id: 'dispatch', label: 'Dispatch', icon: Route, description: 'Flujo de guilds' },
  { id: 'roadmap', label: 'Roadmap', icon: Map, description: 'Plan de implementación' },
] as const;

const KEY_STATS = [
  { label: 'Subsistemas', value: '7', detail: 'LinearRAG · Coloquio · Guilds · SilvaDB · Federation · DreamCycle · Dashboard' },
  { label: 'Retrieval Paths', value: '4+1', detail: 'BM25 · BGE-M3 · PageRank · DreamCycle · (future) HippoRAG-PPR' },
  { label: 'Deployment Profiles', value: '3', detail: 'portable (Pi 4) · clinic (local server) · server (full mesh)' },
  { label: 'Benchmark', value: 'R@5 82%', detail: 'LongMemEval — validated' },
  { label: 'Tests', value: '383', detail: '310 kernel + 61 tylluan-link + 12 FSRS — all passing' },
  { label: 'Cloud Dependency', value: 'Zero', detail: 'All models local · ONNX runtime · Pi 4 compatible' },
];

export default function Page() {
  const [activeTab, setActiveTab] = useState('architecture');

  return (
    <div className="min-h-screen flex flex-col bg-[#0A0F1A] text-slate-200">
      {/* ═══════ HEADER ═══════ */}
      <header className="sticky top-0 z-50 border-b border-white/[0.06] bg-[#0A0F1A]/95 backdrop-blur-md">
        <div className="max-w-[1600px] mx-auto px-4 sm:px-6">
          <div className="flex items-center justify-between h-14">
            {/* Logo + Title */}
            <div className="flex items-center gap-3">
              <div className="flex items-center gap-2">
                <div className="w-7 h-7 rounded-md bg-teal-500/20 border border-teal-500/40 flex items-center justify-center">
                  <Activity className="w-4 h-4 text-teal-400" />
                </div>
                <div>
                  <h1 className="text-sm font-semibold tracking-tight text-slate-100">
                    Tylluan
                  </h1>
                  <p className="text-[10px] text-slate-500 font-mono -mt-0.5 leading-none">
                    cognitive substrate
                  </p>
                </div>
              </div>
              <ChevronRight className="w-3.5 h-3.5 text-slate-600" />
              <span className="text-xs text-slate-400 font-mono">
                Architecture Maps
              </span>
            </div>

            {/* Right side info */}
            <div className="hidden sm:flex items-center gap-4 text-[10px] font-mono text-slate-500">
              <span className="flex items-center gap-1">
                <Cpu className="w-3 h-3" />
                Rust + React
              </span>
              <span className="flex items-center gap-1">
                <Shield className="w-3 h-3" />
                Sovereign
              </span>
              <span className="flex items-center gap-1">
                <GitBranch className="w-3 h-3" />
                Papers 2026
              </span>
            </div>
          </div>
        </div>
      </header>

      {/* ═══════ MAIN CONTENT ═══════ */}
      <main className="flex-1 max-w-[1600px] mx-auto w-full px-4 sm:px-6 py-6">
        {/* Key Stats Bar */}
        <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-2 mb-6">
          {KEY_STATS.map((stat, i) => (
            <motion.div
              key={stat.label}
              initial={{ opacity: 0, y: 8 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.3, delay: i * 0.04 }}
              className="rounded-lg border border-white/[0.06] bg-white/[0.02] px-3 py-2.5 group hover:border-teal-500/20 hover:bg-teal-500/[0.03] transition-colors"
            >
              <div className="text-[10px] text-slate-500 font-mono uppercase tracking-wider mb-0.5">
                {stat.label}
              </div>
              <div className="text-lg font-mono font-bold text-teal-400 tabular-nums leading-tight">
                {stat.value}
              </div>
              <div className="text-[9px] text-slate-600 font-mono mt-1 leading-tight line-clamp-2 group-hover:text-slate-500 transition-colors">
                {stat.detail}
              </div>
            </motion.div>
          ))}
        </div>

        {/* Navigation Tabs */}
        <Tabs value={activeTab} onValueChange={setActiveTab} className="w-full">
          <div className="sticky top-14 z-40 bg-[#0A0F1A]/95 backdrop-blur-md pb-3 -mx-4 px-4 sm:-mx-6 sm:px-6">
            <TabsList className="bg-white/[0.04] border border-white/[0.06] h-10 p-1 gap-0.5 overflow-x-auto w-full justify-start">
              {TABS.map((tab) => {
                const Icon = tab.icon;
                return (
                  <TabsTrigger
                    key={tab.id}
                    value={tab.id}
                    className="flex items-center gap-2 px-3 py-1.5 text-xs font-mono data-[state=active]:bg-teal-500/15 data-[state=active]:text-teal-400 data-[state=active]:border-teal-500/30 data-[state=active]:shadow-none border border-transparent rounded-md transition-all hover:bg-white/[0.04] whitespace-nowrap"
                  >
                    <Icon className="w-3.5 h-3.5" />
                    <span className="hidden sm:inline">{tab.label}</span>
                    <span className="sm:hidden text-[10px]">{tab.label.slice(0, 6)}</span>
                  </TabsTrigger>
                );
              })}
            </TabsList>
          </div>

          {/* Tab content with animation */}
          <AnimatePresence mode="wait">
            <motion.div
              key={activeTab}
              initial={{ opacity: 0, y: 8 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -8 }}
              transition={{ duration: 0.2 }}
              className="mt-4"
            >
              <TabsContent value="architecture" className="mt-0">
                <ArchitectureMap />
              </TabsContent>

              <TabsContent value="fsrs" className="mt-0">
                <FsrsModel />
              </TabsContent>

              <TabsContent value="retrieval" className="mt-0">
                <RetrievalPipeline />
              </TabsContent>

              <TabsContent value="federation" className="mt-0">
                <FederationMesh />
              </TabsContent>

              <TabsContent value="sleep" className="mt-0">
                <SleepCycle />
              </TabsContent>

              <TabsContent value="dispatch" className="mt-0">
                <DispatchFlow />
              </TabsContent>

              <TabsContent value="roadmap" className="mt-0">
                <Roadmap />
              </TabsContent>
            </motion.div>
          </AnimatePresence>
        </Tabs>
      </main>

      {/* ═══════ FOOTER ═══════ */}
      <footer className="border-t border-white/[0.06] bg-[#0A0F1A] mt-auto">
        <div className="max-w-[1600px] mx-auto px-4 sm:px-6 py-4">
          <div className="flex flex-col sm:flex-row items-start sm:items-center justify-between gap-2 text-[10px] font-mono text-slate-600">
            <div className="flex items-center gap-4">
              <span>Tylluan Architecture Visualizer</span>
              <span className="text-slate-700">·</span>
              <span>Based on 8 papers (2025–2026) + project analysis</span>
            </div>
            <div className="flex items-center gap-4">
              <span className="text-slate-700">|</span>
              <span>Papers: FSRS · SCM · HippoRAG 2 · KNEXA-FL · Reddy2026 · Mem0 · Survey · Tool-calling · Deterministic Freshness</span>
            </div>
          </div>
        </div>
      </footer>
    </div>
  );
}