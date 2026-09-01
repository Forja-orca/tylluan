import React, { useState } from 'react';
import { cn } from '../lib/utils';

interface TylluanLogoProps {
  size?: 'sm' | 'md' | 'lg' | 'xl' | '2xl';
  animated?: boolean;
  className?: string;
  showText?: boolean;
}

export function TylluanLogo({
  size = 'md',
  animated = true,
  className,
  showText = true,
}: TylluanLogoProps) {
  const [imgLoaded, setImgLoaded] = useState(true);

  const containerSizes = {
    sm: 'w-8 h-8',
    md: 'w-10 h-10',
    lg: 'w-14 h-14',
    xl: 'w-20 h-20',
    '2xl': 'w-28 h-28',
  };

  const ringPadding = {
    sm: 'p-0.5',
    md: 'p-1',
    lg: 'p-1.5',
    xl: 'p-2',
    '2xl': 'p-2.5',
  };

  // Sovereign Color Tokens (Nocturnal Observatory Amber/Gold)
  const AMBER_GOLD = '#F59E0B';

  return (
    <div className={cn('flex items-center gap-3 select-none', className)}>
      {/* High-End Motion Styling: Clean Ambient Breathing Aura */}
      <style>{`
        @keyframes sovereign-aura-breathe {
          0%, 100% {
            box-shadow: 0 0 14px rgba(245, 158, 11, 0.18), inset 0 0 8px rgba(245, 158, 11, 0.1);
            border-color: rgba(245, 158, 11, 0.3);
          }
          50% {
            box-shadow: 0 0 26px rgba(245, 158, 11, 0.4), inset 0 0 14px rgba(245, 158, 11, 0.22);
            border-color: rgba(245, 158, 11, 0.55);
          }
        }

        .animate-sovereign-aura {
          animation: sovereign-aura-breathe 4.5s ease-in-out infinite;
        }
      `}</style>

      {/* Cybernetic Owl Avatar Container */}
      <div className={cn('relative flex items-center justify-center group', containerSizes[size])}>
        {/* Sovereign Card Base */}
        <div
          className={cn(
            'relative w-full h-full rounded-2xl bg-[#0B0F17] border border-amber-500/30 overflow-hidden flex items-center justify-center backdrop-blur-md transition-all duration-300 group-hover:border-amber-500/60',
            animated && 'animate-sovereign-aura',
            ringPadding[size]
          )}
        >
          {/* Base Layer: Official Owl Image ALWAYS 100% Crisp, Visible & Unobstructed */}
          <div className="w-full h-full flex items-center justify-center relative z-10">
            {imgLoaded ? (
              <img
                src="/tylluan-logo.jpg"
                alt="Tylluan Owl Official Logo"
                className="w-full h-full object-cover rounded-xl shadow-inner transition-transform duration-500 group-hover:scale-[1.02]"
                onError={() => setImgLoaded(false)}
              />
            ) : (
              <svg viewBox="0 0 30 30" className="w-full h-full text-slate-100">
                <path
                  fill="#0B0F17"
                  stroke={AMBER_GOLD}
                  strokeWidth="0.8"
                  d="M24.51,28.51H5.49c-2.21,0-4-1.79-4-4V5.49c0-2.21,1.79-4,4-4h19.03c2.21,0,4,1.79,4,4v19.03C28.51,26.72,26.72,28.51,24.51,28.51z"
                />
                <path fill={AMBER_GOLD} d="M15.47,7.1l-1.3,1.85c-0.2,0.29-0.54,0.47-0.9,0.47h-7.1V7.09C6.16,7.1,15.47,7.1,15.47,7.1z" />
                <polygon fill="#F59E0B" points="24.3,7.1 13.14,22.91 5.7,22.91 16.86,7.1" />
                <path fill={AMBER_GOLD} d="M14.53,22.91l1.31-1.86c0.2-0.29,0.54-0.47,0.9-0.47h7.09v2.33H14.53z" />
              </svg>
            )}
          </div>

          {/* Clean Subtle Vignette Overlay for Depth */}
          <div className="absolute inset-0 bg-gradient-to-b from-transparent via-transparent to-slate-950/20 pointer-events-none z-20" />
        </div>
      </div>

      {/* Typography Label */}
      {showText && (
        <div className="font-mono leading-none">
          <div className="flex items-center gap-1.5 font-bold tracking-tight text-slate-100 text-sm">
            <span className="text-amber-400 tracking-wider uppercase drop-shadow-[0_0_8px_rgba(245,158,11,0.35)]">
              TYLLUAN
            </span>
            <span className="text-[9px] px-1.5 py-0.5 rounded-md bg-amber-500/10 text-amber-400 border border-amber-500/30 uppercase tracking-widest font-extrabold">
              o3
            </span>
          </div>
          <div className="text-[10px] text-slate-400 tracking-widest uppercase font-semibold mt-1 flex items-center gap-1.5">
            <span className="w-1.5 h-1.5 rounded-full bg-amber-400 animate-beacon" />
            <span>Sovereign Substrate</span>
          </div>
        </div>
      )}
    </div>
  );
}
