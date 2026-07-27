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

  // Sovereign Color Tokens (Immutable)
  const CYAN_NEON = '#00F5D4';

  // 12 Nodes Coordinates (Home vs Constellation Offsets)
  const nodesData = [
    // Left Wing (Nodes 0-3)
    { id: 0, type: 'wing_left', hx: 18, hy: 30, cx: 4, cy: 15, delay: 0 },
    { id: 1, type: 'wing_left', hx: 28, hy: 45, cx: 12, cy: 48, delay: 120 },
    { id: 2, type: 'wing_left', hx: 32, hy: 65, cx: 16, cy: 78, delay: 240 },
    { id: 3, type: 'wing_left', hx: 40, hy: 32, cx: 28, cy: 14, delay: 360 },

    // Right Wing (Nodes 4-7)
    { id: 4, type: 'wing_right', hx: 82, hy: 30, cx: 96, cy: 15, delay: 480 },
    { id: 5, type: 'wing_right', hx: 72, hy: 45, cx: 88, cy: 48, delay: 600 },
    { id: 6, type: 'wing_right', hx: 68, hy: 65, cx: 84, cy: 78, delay: 720 },
    { id: 7, type: 'wing_right', hx: 60, hy: 32, cx: 72, cy: 14, delay: 840 },

    // Eyes (Nodes 8-9, Magenta Memory Index)
    { id: 8, type: 'eye', hx: 42, hy: 42, cx: 38, cy: 36, delay: 960, isEye: true },
    { id: 9, type: 'eye', hx: 58, hy: 42, cx: 62, cy: 36, delay: 1080, isEye: true },

    // Chest (Nodes 10-11)
    { id: 10, type: 'chest', hx: 50, hy: 55, cx: 50, cy: 68, delay: 1200 },
    { id: 11, type: 'chest', hx: 50, hy: 74, cx: 50, cy: 88, delay: 1320 },
  ];

  // Edges connecting constellation nodes
  const constellationEdges = [
    [0, 3], [3, 8], [8, 10], [10, 9], [9, 7], [7, 4],
    [0, 1], [1, 2], [2, 11], [11, 6], [6, 5], [5, 4],
    [8, 9], [3, 7], [10, 11]
  ];

  return (
    <div className={cn('flex items-center gap-3 select-none', className)}>
      {/* Elegant Professional Motion System (Klarna / Modern Brand Identity 2026) */}
      <style>{`
        /* Gesto 1: Ambient Cyan Aura Breathe (Soft organic pulse) */
        @keyframes sovereign-aura-breathe {
          0%, 100% {
            box-shadow: 0 0 12px rgba(0, 245, 212, 0.25), inset 0 0 8px rgba(0, 245, 212, 0.15);
            border-color: rgba(0, 245, 212, 0.3);
          }
          50% {
            box-shadow: 0 0 24px rgba(0, 245, 212, 0.6), inset 0 0 14px rgba(0, 245, 212, 0.35);
            border-color: rgba(0, 245, 212, 0.7);
          }
        }

        /* Gesto 2: Sequential Node Glow Pulse (Simple, Purposeful, Playful) */
        @keyframes node-pulse-glow {
          0%, 100% {
            transform: scale(1);
            opacity: 0.4;
            filter: drop-shadow(0 0 2px rgba(0, 245, 212, 0.3));
          }
          50% {
            transform: scale(1.3);
            opacity: 1;
            filter: drop-shadow(0 0 8px rgba(0, 245, 212, 0.9));
          }
        }

        /* Gesto 3: Eye Retina Intelligent Pulse */
        @keyframes eye-retina-pulse {
          0%, 100% {
            transform: scale(1);
            opacity: 0.7;
          }
          50% {
            transform: scale(1.25);
            opacity: 1;
            filter: drop-shadow(0 0 10px rgba(255, 46, 147, 0.9));
          }
        }

        .animate-sovereign-aura {
          animation: sovereign-aura-breathe 4s ease-in-out infinite;
        }

        .animate-retina-pulse {
          animation: eye-retina-pulse 3s ease-in-out infinite;
          transform-origin: center;
        }
      `}</style>

      {/* Cybernetic Owl Avatar Container */}
      <div className={cn('relative flex items-center justify-center group', containerSizes[size])}>
        {/* Subtle Outer Cyan HUD Ring */}
        {animated && (
          <div className="absolute -inset-1 rounded-2xl border border-dashed border-[#00F5D4]/30 pointer-events-none transition-all duration-500 group-hover:border-[#00F5D4]/60" />
        )}

        {/* Sovereign Card Base */}
        <div
          className={cn(
            'relative w-full h-full rounded-2xl bg-[#0B0F17] border border-[#00F5D4]/40 overflow-hidden flex items-center justify-center backdrop-blur-md transition-all duration-300',
            animated && 'animate-sovereign-aura',
            ringPadding[size]
          )}
        >
          {/* Base Layer: Official Owl Image ALWAYS 100% Visible & Legible */}
          <div className="w-full h-full flex items-center justify-center relative z-10">
            {imgLoaded ? (
              <img
                src="/tylluan-logo.jpg"
                alt="Tylluan Owl Official Logo"
                className="w-full h-full object-cover rounded-xl shadow-inner"
                onError={() => setImgLoaded(false)}
              />
            ) : (
              <svg viewBox="0 0 30 30" className="w-full h-full text-slate-100">
                <path
                  fill="#0B0F17"
                  stroke={CYAN_NEON}
                  strokeWidth="0.8"
                  d="M24.51,28.51H5.49c-2.21,0-4-1.79-4-4V5.49c0-2.21,1.79-4,4-4h19.03c2.21,0,4,1.79,4,4v19.03C28.51,26.72,26.72,28.51,24.51,28.51z"
                />
                <path fill={CYAN_NEON} d="M15.47,7.1l-1.3,1.85c-0.2,0.29-0.54,0.47-0.9,0.47h-7.1V7.09C6.16,7.1,15.47,7.1,15.47,7.1z" />
                <polygon fill="#38BDF8" points="24.3,7.1 13.14,22.91 5.7,22.91 16.86,7.1" />
                <path fill={CYAN_NEON} d="M14.53,22.91l1.31-1.86c0.2-0.29,0.54-0.47,0.9-0.47h7.09v2.33H14.53z" />
              </svg>
            )}
          </div>

          {/* Layer 2: Subtle Vector Geometry Facets Overlay */}
          <svg viewBox="0 0 100 100" className="absolute inset-0 w-full h-full z-15 pointer-events-none">
            {/* Wing Feather Geometry */}
            <path d="M18,30 Q28,20 40,32 Q32,48 18,30 Z" fill="none" stroke={CYAN_NEON} strokeWidth="0.6" opacity="0.4" strokeDasharray="3 2" />
            <path d="M82,30 Q72,20 60,32 Q68,48 82,30 Z" fill="none" stroke={CYAN_NEON} strokeWidth="0.6" opacity="0.4" strokeDasharray="3 2" />

            {/* Precision Eye Retina Rings (Unified Cyan Neon) */}
            <g className={cn(animated && 'animate-retina-pulse')}>
              <circle cx="42" cy="42" r="4.5" fill="none" stroke={CYAN_NEON} strokeWidth="1.2" opacity="0.9" />
              <circle cx="42" cy="42" r="1.5" fill={CYAN_NEON} />

              <circle cx="58" cy="42" r="4.5" fill="none" stroke={CYAN_NEON} strokeWidth="1.2" opacity="0.9" />
              <circle cx="58" cy="42" r="1.5" fill={CYAN_NEON} />
            </g>
          </svg>

          {/* Layer 3: 12 Sequential Pulse Nodes (Unified Cyan Neon) */}
          <svg viewBox="0 0 100 100" className="absolute inset-0 w-full h-full z-20 pointer-events-none">
            {nodesData.map((node) => {
              const pulseDelaySecs = (node.id * 0.25).toFixed(2);

              return (
                <g key={node.id}>
                  <style>{`
                    .node-pulse-${node.id} {
                      animation: node-pulse-glow 3s ease-in-out infinite;
                      animation-delay: ${pulseDelaySecs}s;
                      transform-origin: ${node.hx}px ${node.hy}px;
                    }
                  `}</style>

                  <g className={cn(animated && `node-pulse-${node.id}`)}>
                    <circle
                      cx={node.hx}
                      cy={node.hy}
                      r={node.isEye ? 3 : 2}
                      fill={CYAN_NEON}
                    />
                  </g>
                </g>
              );
            })}
          </svg>

          {/* Vignette Overlay */}
          <div className="absolute inset-0 bg-gradient-to-b from-transparent via-transparent to-slate-950/20 pointer-events-none z-30" />
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
