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
    '2xl': 'p-3',
  };

  return (
    <div className={cn('flex items-center gap-3 select-none', className)}>
      {/* SOTA Morphing Animation Engine CSS */}
      <style>{`
        @keyframes owl-solid-phase {
          0%, 35% {
            opacity: 1;
            transform: scale(1) rotateY(0deg);
            filter: drop-shadow(0 0 10px rgba(0, 245, 212, 0.5));
          }
          40% {
            opacity: 0;
            transform: scale(0.3) rotateY(360deg);
            filter: drop-shadow(0 0 25px rgba(0, 245, 212, 0.9)) blur(2px);
          }
          75% {
            opacity: 0;
            transform: scale(0.3) rotateY(360deg);
          }
          82% {
            opacity: 1;
            transform: scale(1.08) rotateY(720deg);
            filter: drop-shadow(0 0 30px rgba(0, 245, 212, 1));
          }
          88%, 100% {
            opacity: 1;
            transform: scale(1) rotateY(720deg);
            filter: drop-shadow(0 0 10px rgba(0, 245, 212, 0.5));
          }
        }

        @keyframes nodes-constellation-phase {
          0%, 38% {
            opacity: 0;
            transform: scale(0.2) rotate(0deg);
          }
          42% {
            opacity: 1;
            transform: scale(1.1) rotate(90deg);
            filter: drop-shadow(0 0 20px rgba(0, 245, 212, 0.9));
          }
          45%, 72% {
            opacity: 1;
            transform: scale(1) rotate(180deg);
          }
          78% {
            opacity: 0;
            transform: scale(0.2) rotate(360deg);
            filter: drop-shadow(0 0 30px rgba(56, 189, 248, 1));
          }
          80%, 100% {
            opacity: 0;
            transform: scale(0.2) rotate(360deg);
          }
        }

        @keyframes laser-pulse-beam {
          0%, 100% {
            stroke-dashoffset: 0;
            opacity: 0.7;
          }
          50% {
            stroke-dashoffset: 20;
            opacity: 1;
          }
        }

        @keyframes hud-rotate-cw {
          from { transform: rotate(0deg); }
          to { transform: rotate(360deg); }
        }

        @keyframes hud-rotate-ccw {
          from { transform: rotate(0deg); }
          to { transform: rotate(-360deg); }
        }

        .animate-owl-solid-morph {
          animation: owl-solid-phase 9.5s cubic-bezier(0.4, 0, 0.2, 1) infinite;
          transform-style: preserve-3d;
        }

        .animate-owl-nodes-morph {
          animation: nodes-constellation-phase 9.5s cubic-bezier(0.4, 0, 0.2, 1) infinite;
          transform-style: preserve-3d;
        }

        .animate-hud-cw {
          animation: hud-rotate-cw 12s linear infinite;
        }

        .animate-hud-ccw {
          animation: hud-rotate-ccw 16s linear infinite;
        }

        .laser-edge {
          stroke-dasharray: 4, 4;
          animation: laser-pulse-beam 2.5s linear infinite;
        }
      `}</style>

      {/* Cybernetic Avatar HUD Container */}
      <div className={cn('relative flex items-center justify-center group perspective-1000', containerSizes[size])}>
        {/* Outer Rotating HUD Ring 1 */}
        {animated && (
          <div className="absolute -inset-1.5 rounded-2xl border border-dashed border-[#00F5D4]/40 animate-hud-cw pointer-events-none" />
        )}

        {/* Outer Counter-Rotating HUD Ring 2 */}
        {animated && (
          <div className="absolute -inset-3 rounded-full border border-dotted border-sky-400/30 animate-hud-ccw pointer-events-none" />
        )}

        {/* Inner Card Container */}
        <div
          className={cn(
            'relative w-full h-full rounded-2xl bg-[#0B0F17] border border-[#00F5D4]/50 overflow-hidden flex items-center justify-center shadow-[0_0_20px_rgba(0,245,212,0.3)] backdrop-blur-md',
            ringPadding[size]
          )}
        >
          {/* Phase 1: Solid Tylluan Owl Emblem (Morphing outward) */}
          <div className={cn('w-full h-full flex items-center justify-center absolute inset-0', animated && 'animate-owl-solid-morph')}>
            {imgLoaded ? (
              <img
                src="/tylluan-logo.jpg"
                alt="Tylluan Cybernetic Owl Logo"
                className="w-full h-full object-cover rounded-xl"
                onError={() => setImgLoaded(false)}
              />
            ) : (
              <svg viewBox="0 0 30 30" className="w-full h-full text-slate-100">
                <path
                  fill="#0B0F17"
                  stroke="#00F5D4"
                  strokeWidth="0.8"
                  d="M24.51,28.51H5.49c-2.21,0-4-1.79-4-4V5.49c0-2.21,1.79-4,4-4h19.03c2.21,0,4,1.79,4,4v19.03C28.51,26.72,26.72,28.51,24.51,28.51z"
                />
                <path fill="#00F5D4" d="M15.47,7.1l-1.3,1.85c-0.2,0.29-0.54,0.47-0.9,0.47h-7.1V7.09C6.16,7.1,15.47,7.1,15.47,7.1z" />
                <polygon fill="#38BDF8" points="24.3,7.1 13.14,22.91 5.7,22.91 16.86,7.1" />
                <path fill="#00F5D4" d="M14.53,22.91l1.31-1.86c0.2-0.29,0.54-0.47,0.9-0.47h7.09v2.33H14.53z" />
              </svg>
            )}
          </div>

          {/* Phase 2: Cybernetic Node Network Constellation (Morphing inward) */}
          <div className={cn('w-full h-full flex items-center justify-center absolute inset-0 p-1', animated ? 'animate-owl-nodes-morph' : 'opacity-0')}>
            <svg viewBox="0 0 100 100" className="w-full h-full overflow-visible">
              {/* Connecting Laser Edges */}
              <line x1="50" y1="15" x2="25" y2="35" stroke="#00F5D4" strokeWidth="1.5" className="laser-edge" />
              <line x1="50" y1="15" x2="75" y2="35" stroke="#00F5D4" strokeWidth="1.5" className="laser-edge" />
              <line x1="25" y1="35" x2="35" y2="55" stroke="#38BDF8" strokeWidth="1.5" className="laser-edge" />
              <line x1="75" y1="35" x2="65" y2="55" stroke="#38BDF8" strokeWidth="1.5" className="laser-edge" />
              <line x1="35" y1="55" x2="50" y2="85" stroke="#00F5D4" strokeWidth="1.5" className="laser-edge" />
              <line x1="65" y1="55" x2="50" y2="85" stroke="#00F5D4" strokeWidth="1.5" className="laser-edge" />

              {/* Owl Retinal Cybernetic Eyes */}
              <line x1="38" y1="40" x2="62" y2="40" stroke="#00F5D4" strokeWidth="1" strokeDasharray="2 2" />
              <line x1="50" y1="15" x2="50" y2="85" stroke="#38BDF8" strokeWidth="1" opacity="0.6" />

              {/* Node Vertices */}
              <circle cx="50" cy="15" r="4" fill="#00F5D4" className="animate-ping opacity-75" />
              <circle cx="50" cy="15" r="3" fill="#00F5D4" />

              <circle cx="25" cy="35" r="2.5" fill="#38BDF8" />
              <circle cx="75" cy="35" r="2.5" fill="#38BDF8" />

              {/* Glowing Eyes Nodes */}
              <circle cx="38" cy="40" r="4" fill="#00F5D4" className="shadow-[0_0_10px_#00F5D4]" />
              <circle cx="62" cy="40" r="4" fill="#00F5D4" className="shadow-[0_0_10px_#00F5D4]" />

              <circle cx="35" cy="55" r="2.5" fill="#38BDF8" />
              <circle cx="65" cy="55" r="2.5" fill="#38BDF8" />

              <circle cx="50" cy="85" r="3.5" fill="#00F5D4" />
            </svg>
          </div>

          {/* Vignette Overlay */}
          <div className="absolute inset-0 bg-gradient-to-b from-transparent via-transparent to-slate-950/30 pointer-events-none" />
        </div>
      </div>

      {/* Typography Label */}
      {showText && (
        <div className="font-mono leading-none">
          <div className="flex items-center gap-1.5 font-bold tracking-tight text-slate-100 text-sm">
            <span className="text-[#00F5D4] tracking-wider uppercase drop-shadow-[0_0_8px_rgba(0,245,212,0.4)]">
              TYLLUAN
            </span>
            <span className="text-[9px] px-1.5 py-0.5 rounded-md bg-[#00F5D4]/10 text-[#00F5D4] border border-[#00F5D4]/30 uppercase tracking-widest font-extrabold">
              o3
            </span>
          </div>
          <div className="text-[10px] text-slate-400 tracking-widest uppercase font-semibold mt-1 flex items-center gap-1">
            <span className="w-1.5 h-1.5 rounded-full bg-[#00F5D4] animate-ping" />
            <span>Sovereign Substrate</span>
          </div>
        </div>
      )}
    </div>
  );
}
