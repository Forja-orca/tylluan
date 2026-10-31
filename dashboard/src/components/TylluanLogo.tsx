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
  const AMBER_GOLD = 'hsl(var(--primary))';

  return (
    <div className={cn('flex items-center gap-3 select-none', className)}>
      {/* High-End Motion Styling: Clean Ambient Breathing Aura */}
      <style>{`
        @keyframes sovereign-aura-breathe {
          0%, 100% {
            box-shadow: 0 0 14px hsl(var(--primary) / 0.18), inset 0 0 8px hsl(var(--primary) / 0.1);
            border-color: hsl(var(--primary) / 0.3);
          }
          50% {
            box-shadow: 0 0 26px hsl(var(--primary) / 0.4), inset 0 0 14px hsl(var(--primary) / 0.22);
            border-color: hsl(var(--primary) / 0.55);
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
            'relative w-full h-full rounded bg-slate-950 border border-primary/30 overflow-hidden flex items-center justify-center transition-colors group-hover:border-primary/60',
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
                className="w-full h-full object-cover rounded shadow-none"
                onError={() => setImgLoaded(false)}
              />
            ) : (
              <svg viewBox="0 0 30 30" className="w-full h-full text-slate-100">
                <path
                  fill="hsl(var(--slate-950))"
                  stroke={AMBER_GOLD}
                  strokeWidth="0.8"
                  d="M24.51,28.51H5.49c-2.21,0-4-1.79-4-4V5.49c0-2.21,1.79-4,4-4h19.03c2.21,0,4,1.79,4,4v19.03C28.51,26.72,26.72,28.51,24.51,28.51z"
                />
                <path fill={AMBER_GOLD} d="M15.47,7.1l-1.3,1.85c-0.2,0.29-0.54,0.47-0.9,0.47h-7.1V7.09C6.16,7.1,15.47,7.1,15.47,7.1z" />
                <polygon fill={AMBER_GOLD} points="24.3,7.1 13.14,22.91 5.7,22.91 16.86,7.1" />
                <path fill={AMBER_GOLD} d="M14.53,22.91l1.31-1.86c0.2-0.29,0.54-0.47,0.9-0.47h7.09v2.33H14.53z" />
              </svg>
            )}
          </div>
        </div>
      </div>

      {/* Typography Label */}
      {showText && (
        <div className="font-mono leading-none">
          <div className="flex items-center gap-1.5 font-bold tracking-tight text-slate-100 text-sm">
            <span className="text-primary font-bold">
              TYLLUAN
            </span>
            <span className="text-[11px] px-1.5 py-0.5 rounded bg-primary/10 text-primary border border-primary/30 font-bold">
              o3
            </span>
          </div>
          <div className="text-[11px] text-slate-400 font-semibold mt-1 flex items-center gap-1.5">
            <span className="w-1.5 h-1.5 rounded-full bg-primary animate-beacon" />
            <span>Sovereign Substrate</span>
          </div>
        </div>
      )}
    </div>
  );
}
