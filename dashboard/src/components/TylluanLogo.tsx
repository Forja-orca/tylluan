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
  const MAGENTA_FLASH = '#FF2E93';

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
      {/* Synchronized 60FPS organic 4-phase keyframe engine */}
      <style>{`
        /* Phase 1: Base State Ambient Aura Breathe */
        @keyframes sovereign-aura-breathe {
          0%, 100% {
            box-shadow: 0 0 10px rgba(0, 245, 212, 0.35), inset 0 0 6px rgba(0, 245, 212, 0.2);
          }
          50% {
            box-shadow: 0 0 22px rgba(0, 245, 212, 0.7), inset 0 0 14px rgba(0, 245, 212, 0.4);
          }
        }

        /* Owl Body Progressive Opacity: Phase 1 flat at 1.0, Phase 2 decays, Phase 3 low, Phase 4 returns */
        @keyframes owl-body-progressive {
          0%, 15.8% {
            /* Phase 1 (1.5s): Completely static owl base */
            opacity: 1;
            transform: scale(1);
            filter: drop-shadow(0 0 8px rgba(0, 245, 212, 0.5));
          }
          47.4%, 68.4% {
            /* Phase 2 & 3: Decayed body opacity */
            opacity: 0.15;
            transform: scale(1);
            filter: drop-shadow(0 0 2px rgba(0, 245, 212, 0.2));
          }
          94.7% {
            /* Phase 4: Full opacity recovered */
            opacity: 1;
            transform: scale(1);
            filter: drop-shadow(0 0 12px rgba(0, 245, 212, 0.8));
          }
          97.8% {
            /* Micro-Flash Seal at end of cycle (200ms) */
            opacity: 1;
            transform: scale(1.05);
            filter: drop-shadow(0 0 25px rgba(255, 46, 147, 0.95));
          }
          100% {
            opacity: 1;
            transform: scale(1);
            filter: drop-shadow(0 0 8px rgba(0, 245, 212, 0.5));
          }
        }

        /* Outer HUD Rings: Synchronized strictly with the 4 phases (Static in Phase 1, Spins in Phase 2 & 3, Decelerates in Phase 4) */
        @keyframes hud-ring-sync-cw {
          0%, 15.8% {
            transform: rotate(0deg);
          }
          47.4% {
            transform: rotate(180deg);
          }
          68.4% {
            transform: rotate(360deg);
          }
          94.7%, 100% {
            transform: rotate(540deg);
          }
        }

        @keyframes hud-ring-sync-ccw {
          0%, 15.8% {
            transform: rotate(0deg);
          }
          47.4% {
            transform: rotate(-180deg);
          }
          68.4% {
            transform: rotate(-360deg);
          }
          94.7%, 100% {
            transform: rotate(-540deg);
          }
        }

        /* Constellation Lines Opacity */
        @keyframes constellation-lines-fade {
          0%, 15.8% {
            opacity: 0;
          }
          47.4%, 68.4% {
            opacity: 0.45;
          }
          94.7%, 100% {
            opacity: 0;
          }
        }

        .animate-sovereign-aura {
          animation: sovereign-aura-breathe 3.5s ease-in-out infinite;
        }

        .animate-owl-body-loop {
          animation: owl-body-progressive 9.5s linear infinite;
        }

        .animate-hud-ring-cw {
          animation: hud-ring-sync-cw 9.5s cubic-bezier(0.4, 0, 0.2, 1) infinite;
        }

        .animate-hud-ring-ccw {
          animation: hud-ring-sync-ccw 9.5s cubic-bezier(0.4, 0, 0.2, 1) infinite;
        }

        .animate-constellation-lines {
          animation: constellation-lines-fade 9.5s cubic-bezier(0.4, 0, 0.2, 1) infinite;
        }
      `}</style>

      {/* Cybernetic Owl Avatar Container */}
      <div className={cn('relative flex items-center justify-center group', containerSizes[size])}>
        {/* Outer Synchronized HUD Ring 1 */}
        {animated && (
          <div className="absolute -inset-1.5 rounded-2xl border border-dashed border-[#00F5D4]/40 animate-hud-ring-cw pointer-events-none" />
        )}

        {/* Outer Synchronized HUD Ring 2 */}
        {animated && (
          <div className="absolute -inset-3 rounded-full border border-dotted border-[#FF2E93]/30 animate-hud-ring-ccw pointer-events-none" />
        )}

        {/* Sovereign Card Base */}
        <div
          className={cn(
            'relative w-full h-full rounded-2xl bg-[#0B0F17] border border-[#00F5D4]/40 overflow-hidden flex items-center justify-center backdrop-blur-md transition-all duration-300',
            animated && 'animate-sovereign-aura',
            ringPadding[size]
          )}
        >
          {/* Base Layer: Official Owl Image */}
          <div className={cn('w-full h-full flex items-center justify-center relative z-10', animated && 'animate-owl-body-loop')}>
            {imgLoaded ? (
              <img
                src="/tylluan-logo.jpg"
                alt="Tylluan Owl Official Logo"
                className="w-full h-full object-cover rounded-xl"
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

          {/* Layer 1.5: Professional Vector Feather Lines Overlay (Cyan Neon Geometric Wings & Contour) */}
          <svg viewBox="0 0 100 100" className={cn("absolute inset-0 w-full h-full z-15 pointer-events-none transition-opacity duration-500", animated && 'animate-owl-body-loop')}>
            {/* Wing Feather Structural Facets */}
            <path d="M18,30 Q28,20 40,32 Q32,48 18,30 Z" fill="none" stroke={CYAN_NEON} strokeWidth="0.75" opacity="0.6" strokeDasharray="4 2" />
            <path d="M82,30 Q72,20 60,32 Q68,48 82,30 Z" fill="none" stroke={CYAN_NEON} strokeWidth="0.75" opacity="0.6" strokeDasharray="4 2" />
            <path d="M28,45 L32,65 L50,55 L40,32 Z" fill="none" stroke={CYAN_NEON} strokeWidth="0.6" opacity="0.4" />
            <path d="M72,45 L68,65 L50,55 L60,32 Z" fill="none" stroke={CYAN_NEON} strokeWidth="0.6" opacity="0.4" />

            {/* Chest Memory Lattice */}
            <polygon points="42,42 58,42 50,55" fill="none" stroke={MAGENTA_FLASH} strokeWidth="0.75" opacity="0.5" />
            <polygon points="50,55 32,65 50,74 68,65" fill="none" stroke={CYAN_NEON} strokeWidth="0.6" opacity="0.4" />
            
            {/* Precision Eye Retina Rings (Superimposed Exactly over Logo Eyes at 42,42 & 58,42) */}
            <circle cx="42" cy="42" r="4.5" fill="none" stroke={MAGENTA_FLASH} strokeWidth="1.2" opacity="0.95" />
            <circle cx="42" cy="42" r="1.5" fill={MAGENTA_FLASH} className="animate-ping" style={{ animationDuration: '2.5s' }} />
            
            <circle cx="58" cy="42" r="4.5" fill="none" stroke={MAGENTA_FLASH} strokeWidth="1.2" opacity="0.95" />
            <circle cx="58" cy="42" r="1.5" fill={MAGENTA_FLASH} className="animate-ping" style={{ animationDuration: '2.5s' }} />
          </svg>

          {/* SVG Overlay Layer: 12 Granular Nodes + Staggered Trajectories + Constellation Lines */}
          <svg viewBox="0 0 100 100" className="absolute inset-0 w-full h-full z-20 pointer-events-none overflow-visible">
            {/* Constellation Connecting Lines (Soft White rgba(255,255,255,0.3)) */}
            <g className={cn(animated && 'animate-constellation-lines')}>
              {constellationEdges.map(([fromId, toId], idx) => {
                const nFrom = nodesData[fromId];
                const nTo = nodesData[toId];
                return (
                  <line
                    key={idx}
                    x1={nFrom.cx}
                    y1={nFrom.cy}
                    x2={nTo.cx}
                    y2={nTo.cy}
                    stroke="rgba(255,255,255,0.3)"
                    strokeWidth="1.2"
                    strokeDasharray="3 2"
                  />
                );
              })}
            </g>

            {/* 12 Individual Nodes with Staggered Flight Trajectories */}
            {nodesData.map((node) => {
              const nodeAnimName = `node-granular-flight-${node.id}`;
              const totalLoopSecs = 9.5;

              // Timing Percentages for 9.5s Total Loop
              const flyOutStartMs = 1500 + node.delay; // Phase 2 start (staggered)
              const flyOutEndMs = 4500;                // Phase 2 end

              const reverseDelay = 1320 - node.delay;  // Phase 4 reverse order start
              const flyInStartMs = 6500 + reverseDelay;
              const flyInEndMs = 9000;                 // Phase 4 end

              const pFlyOutStart = ((flyOutStartMs / (totalLoopSecs * 1000)) * 100).toFixed(1);
              const pFlyOutEnd = ((flyOutEndMs / (totalLoopSecs * 1000)) * 100).toFixed(1);
              const pFlyInStart = ((flyInStartMs / (totalLoopSecs * 1000)) * 100).toFixed(1);
              const pFlyInEnd = ((flyInEndMs / (totalLoopSecs * 1000)) * 100).toFixed(1);

              const dx = node.cx - node.hx;
              const dy = node.cy - node.hy;

              return (
                <g key={node.id}>
                  {/* Per-node keyframes */}
                  <style>{`
                    @keyframes ${nodeAnimName} {
                      0%, ${pFlyOutStart}% {
                        /* Phase 1: Flat at home inside owl base */
                        transform: translate(0px, 0px);
                        opacity: 0.1;
                      }
                      ${pFlyOutEnd}% {
                        /* Phase 2 end: Reaches constellation position */
                        transform: translate(${dx}px, ${dy}px);
                        opacity: 1;
                      }
                      ${pFlyInStart}% {
                        /* Phase 3: Soft spring micro-oscillation in constellation position */
                        transform: translate(${dx}px, ${dy + (node.id % 2 === 0 ? 1.5 : -1.5)}px);
                        opacity: 1;
                      }
                      ${pFlyInEnd}%, 100% {
                        /* Phase 4: Elastic overshoot return to home position */
                        transform: translate(0px, 0px);
                        opacity: 0.1;
                      }
                    }

                    .node-granular-${node.id} {
                      animation: ${nodeAnimName} ${totalLoopSecs}s cubic-bezier(0.34, 1.56, 0.64, 1) infinite;
                      transform-origin: ${node.hx}px ${node.hy}px;
                    }
                  `}</style>

                  {/* SVG Node Vertex */}
                  <g className={cn(animated && `node-granular-${node.id}`)}>
                    <circle
                      cx={node.hx}
                      cy={node.hy}
                      r={node.isEye ? 3.5 : 2.5}
                      fill={node.isEye ? MAGENTA_FLASH : CYAN_NEON}
                    />

                    <circle
                      cx={node.hx}
                      cy={node.hy}
                      r={node.isEye ? 6 : 4.5}
                      fill="none"
                      stroke={node.isEye ? MAGENTA_FLASH : CYAN_NEON}
                      strokeWidth="0.8"
                      opacity={node.isEye ? '0.85' : '0.5'}
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
