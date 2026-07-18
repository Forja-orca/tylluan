import React, { useState } from 'react';
import { LaboratoryTab } from './LaboratoryTab';
import { VisionTab } from './VisionTab';
import PlanModePanel from './PlanModePanel';
import ProjectSkillsPanel from './ProjectSkillsPanel';
import BackgroundJobsPanel from './BackgroundJobsPanel';
import { Beaker, Camera, ShieldCheck, FileCode, Cpu } from 'lucide-react';

interface LabConsolidatedProps {
  bridge: any;
  notify: (msg: string, type?: 'info' | 'error') => void;
}

export function LabConsolidated(props: LabConsolidatedProps) {
  const [subTab, setSubTab] = useState('laboratory');

  return (
    <div className="space-y-6">
      {/* Sub Navigation */}
      <div className="flex border-b border-slate-800 pb-2 gap-2 flex-wrap">
        <button
          onClick={() => setSubTab('laboratory')}
          className={`flex items-center gap-2 px-4 py-2 text-xs font-mono font-bold uppercase rounded-lg border transition-all ${
            subTab === 'laboratory'
              ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-400'
              : 'bg-slate-900/40 border-slate-800/80 text-slate-400 hover:text-slate-200'
          }`}
        >
          <Beaker className="w-3.5 h-3.5" />
          Laboratory
        </button>
        <button
          onClick={() => setSubTab('vision')}
          className={`flex items-center gap-2 px-4 py-2 text-xs font-mono font-bold uppercase rounded-lg border transition-all ${
            subTab === 'vision'
              ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-400'
              : 'bg-slate-900/40 border-slate-800/80 text-slate-400 hover:text-slate-200'
          }`}
        >
          <Camera className="w-3.5 h-3.5" />
          Vision
        </button>
        <button
          onClick={() => setSubTab('plan')}
          className={`flex items-center gap-2 px-4 py-2 text-xs font-mono font-bold uppercase rounded-lg border transition-all ${
            subTab === 'plan'
              ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-400'
              : 'bg-slate-900/40 border-slate-800/80 text-slate-400 hover:text-slate-200'
          }`}
        >
          <ShieldCheck className="w-3.5 h-3.5" />
          Plan Mode
        </button>
        <button
          onClick={() => setSubTab('skills')}
          className={`flex items-center gap-2 px-4 py-2 text-xs font-mono font-bold uppercase rounded-lg border transition-all ${
            subTab === 'skills'
              ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-400'
              : 'bg-slate-900/40 border-slate-800/80 text-slate-400 hover:text-slate-200'
          }`}
        >
          <FileCode className="w-3.5 h-3.5" />
          Project Skills
        </button>
        <button
          onClick={() => setSubTab('jobs')}
          className={`flex items-center gap-2 px-4 py-2 text-xs font-mono font-bold uppercase rounded-lg border transition-all ${
            subTab === 'jobs'
              ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-400'
              : 'bg-slate-900/40 border-slate-800/80 text-slate-400 hover:text-slate-200'
          }`}
        >
          <Cpu className="w-3.5 h-3.5" />
          Background Jobs
        </button>
      </div>

      {/* Tab Panels */}
      <div>
        {subTab === 'laboratory' && (
          <LaboratoryTab
            bridge={props.bridge}
            notify={props.notify}
          />
        )}
        {subTab === 'vision' && (
          <VisionTab
            bridge={props.bridge}
            notify={props.notify}
          />
        )}
        {subTab === 'plan' && (
          <PlanModePanel
            bridge={props.bridge}
            notify={props.notify}
          />
        )}
        {subTab === 'skills' && (
          <ProjectSkillsPanel
            bridge={props.bridge}
            notify={props.notify}
          />
        )}
        {subTab === 'jobs' && (
          <BackgroundJobsPanel
            bridge={props.bridge}
            notify={props.notify}
          />
        )}
      </div>
    </div>
  );
}
export default LabConsolidated;
