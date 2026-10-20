import { render } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import React from 'react';
import axe from 'axe-core';
import type { NexusBridge } from '../../lib/nexus-bridge';

// Mock canvas getContext to avoid JSDOM stderr noise
(HTMLCanvasElement.prototype as unknown as Record<string, unknown>).getContext = vi.fn().mockReturnValue({
  measureText: () => ({ width: 10 }),
  fillRect: () => {},
  clearRect: () => {},
  getImageData: () => ({ data: new Array(4) }),
  putImageData: () => {},
  createImageData: () => [],
  setTransform: () => {},
  drawImage: () => {},
  save: () => {},
  fillText: () => {},
  restore: () => {},
  beginPath: () => {},
  moveTo: () => {},
  lineTo: () => {},
  closePath: () => {},
  stroke: () => {},
  translate: () => {},
  scale: () => {},
  rotate: () => {},
  arc: () => {},
  fill: () => {},
});
vi.mock('../HomeostasisWidget', () => ({
  HomeostasisWidget: () => <div data-testid="mock-homeostasis-widget">Homeostasis</div>,
}));
vi.mock('../DreamStatusWidget', () => ({
  DreamStatusWidget: () => <div data-testid="mock-dream-widget">Dream</div>,
}));
vi.mock('../CanaryStatusWidget', () => ({
  CanaryStatusWidget: () => <div data-testid="mock-canary-widget">Canary</div>,
}));
vi.mock('../ui/MetricPrimitives', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../ui/MetricPrimitives')>();
  return {
    ...actual,
    MiniSparkline: () => <div data-testid="mock-mini-sparkline" />,
  };
});
vi.mock('../../hooks/usePolling', () => ({
  usePolling: () => undefined,
}));
vi.mock('../../lib/timeout', () => ({
  withTimeout: (promise: Promise<unknown>) => promise,
}));

const fetchRaw = vi.fn().mockImplementation((url: string) => {
  if (url === '/api/v1/setup-hint') {
    return Promise.resolve({
      mcp_clients: {
        claude_desktop: { config: { mcpServers: { tylluan: { url: 'http://127.0.0.1:47004/sse' } } } },
      },
    });
  }
  if (url.includes('/api/v1/coloquio/channels/general')) {
    return Promise.resolve({
      channel_id: 'general',
      messages: [
        {
          msg_id: 'm1',
          turn: 100,
          author_id: 'deep',
          content: 'Hello team',
          created_at: 1790000000,
        },
      ],
    });
  }
  if (url === '/api/v1/coloquio/channels') {
    return Promise.resolve({
      channels: [{ channel_id: 'general', name: 'General', archived: false }],
    });
  }
  if (url === '/health?verbose=true') {
    return Promise.resolve({
      status: 'healthy',
      version: '0.17.1',
      commit: '7f356a7',
      components: {
        sovereign_tools: { ok: true, count: 6 },
        embeddings: { ok: true },
        reranker: { ok: true },
        guilds: { active: 14, total: 50 },
        silva: { nodes: 10518, edges: 19329 },
      },
    });
  }
  if (url.includes('/api/v1/decide/shadow')) {
    return Promise.resolve({
      rows: [
        {
          id: 1,
          ts: '2026-10-20T00:00:00Z',
          contract: 'recall_gate',
          request_hash: 'hash-1',
          rules_value: 'allow',
          model_value: 'allow',
          agreed: 1,
          mode: 'shadow',
          model_identity: 'laya',
        },
      ],
      summary: [{ contract: 'recall_gate', n: 1, n_agreed: 1, n_disagreed: 0, n_null: 0 }],
      total: 1,
    });
  }
  if (url.includes('/api/v1/audit/trail')) {
    return Promise.resolve({
      entries: [
        {
          id: 'audit-1',
          action: 'dispatch',
          agent_id: 'deep',
          timestamp: '2026-10-20T00:00:00Z',
          hmac_valid: true,
        },
      ],
      total: 1,
    });
  }
  if (url.includes('/api/v1/silva/conflicted')) {
    return Promise.resolve({
      nodes: [
        {
          id: 'node-conf-1',
          node_type: 'Semantic',
          content_excerpt: 'Sample conflicted node excerpt',
          conflicted_since: '2026-10-20T00:00:00Z',
        },
      ],
      total_conflicted: 1,
      limit: 50,
    });
  }
  if (url.includes('/api/v1/mesh/peers') || url.includes('/api/v1/federation/mesh/peers')) {
    return Promise.resolve({
      peers: [
        {
          peer_id: 'peer-alpha',
          address: '192.168.1.50:47004',
          connected: true,
          last_seen: '2026-10-20T00:00:00Z',
        },
      ],
    });
  }
  if (url.includes('/api/v1/federation/nodes')) {
    return Promise.resolve({
      nodes: [
        {
          id: 'node-1',
          title: 'Remote Node 1',
          origin_peer: 'peer-alpha',
          synced_at: '2026-10-20T00:00:00Z',
        },
      ],
    });
  }
  if (url.includes('/api/v1/audit/verify')) {
    return Promise.resolve({
      ok: true,
      status: 'clean',
      valid_count: 10,
      tampered_count: 0,
      message: 'Audit trail intact',
    });
  }
  if (url.includes('/api/v1/audit/latency')) {
    return Promise.resolve({
      available: true,
      latency_ms: { p50: 10, p95: 25, p99: 50, max: 80, sampled: 100, zero_latency_excluded: 0 },
    });
  }
  if (url.includes('/api/v1/security/events')) {
    return Promise.resolve({
      status: 'healthy',
      timestamp: '2026-10-20T00:00:00Z',
      config_valid: true,
      guilds: [],
    });
  }
  if (url.includes('/api/v1/audit/logs')) {
    return Promise.resolve([]);
  }
  return Promise.resolve({ nodes: [], rows: [], summary: [], entries: [], peers: [], total: 0 });
});

const mockBridge = {
  fetchRaw,
  getBaseUrl: () => 'http://127.0.0.1:47004',
  fetchSessionDigests: vi.fn().mockResolvedValue([]),
  getAuditTrail: vi.fn().mockResolvedValue({
    entries: [
      {
        agent_id: 'deep',
        guild: 'system',
        intent_preview: 'Verify security state',
        allowed: true,
        timestamp: '2026-10-20T00:00:00Z',
      },
    ],
    total: 1,
  }),
  listFederationPeers: vi.fn().mockResolvedValue([
    { name: 'peer-alpha', url: 'http://192.168.1.50:47004', allowed: true }
  ]),
  getFederationNodes: vi.fn().mockResolvedValue([
    { id: 'node-1', node_type: 'memory', origin_peer: 'peer-alpha', title: 'Remote Note' }
  ]),
} as unknown as NexusBridge;

vi.mock('../../hooks/useNexus', () => ({
  useNexus: () => ({
    agentProfiles: [{ agent_id: 'deep', first_seen: '2026-08-01', total_calls: 10, competencies: {} }],
    bridge: mockBridge,
    events: [],
    online: true,
    guilds: [],
    stats: {},
    memoryStats: { document_count: 10518, disk_usage_bytes: 1024, node_count: 10518, edge_count: 19329 },
  }),
}));

import { OverviewTab } from '../OverviewTab';
import { ColoquioTab } from '../ColoquioTab';
import { DecisionServicePanel } from '../DecisionServicePanel';
import AuditTrailPanel from '../AuditTrailPanel';
import { ContradictionsPanel } from '../ContradictionsPanel';
import { FederationTab } from '../FederationTab';
import { TrustConsoleTab } from '../TrustConsoleTab';

// En JSDOM axe-core NO puede evaluar color-contrast ni renderizado layout real responsive a 390px
// (vitest corre sin Tailwind CSS cargado ni motor de layout de navegador).
// Esta suite valida la estructura a11y en jsdom (nombres accesibles, roles ARIA, labels, semántica).
// El contraste visual y responsive completo se verifican en navegador real con arnés headless.
describe('Axe-Core Accessibility Audit — 7 Views (estructura a11y en jsdom; contraste y responsive se miden en navegador real con arnés headless)', () => {
  beforeEach(() => {
    fetchRaw.mockClear();
  });

  const views: Array<{ name: string; render: () => React.ReactElement }> = [
    {
      name: 'Overview',
      render: () => (
        <OverviewTab
          bridge={mockBridge}
          goldenSignals={null}
          guildsUtilization={null}
          memoryRetention={null}
          sloSummary={null}
          guilds={[]}
          approvals={[]}
          memoryStats={{ document_count: 10518, disk_usage_bytes: 1024, node_count: 10518, edge_count: 19329 }}
          healthDetailed={null}
          sysStatus={{ embeddings_loaded: true, silva_healthy: true }}
          events={[]}
          notify={vi.fn()}
          refreshData={vi.fn().mockResolvedValue(undefined)}
          online={true}
        />
      ),
    },
    {
      name: 'Coloquio',
      render: () => <ColoquioTab bridge={mockBridge} />,
    },
    {
      name: 'Decision',
      render: () => <DecisionServicePanel bridge={mockBridge} />,
    },
    {
      name: 'Audit',
      render: () => <AuditTrailPanel bridge={mockBridge} />,
    },
    {
      name: 'Contradictions',
      render: () => <ContradictionsPanel bridge={mockBridge} notify={vi.fn()} />,
    },
    {
      name: 'Federation',
      render: () => <FederationTab bridge={mockBridge} notify={vi.fn()} />,
    },
    {
      name: 'TrustConsole (System)',
      render: () => <TrustConsoleTab />,
    },
  ];

  for (const view of views) {
    for (const theme of ['light', 'dark'] as const) {
      for (const width of [1440, 390] as const) {
        it(`audits ${view.name} [${theme} mode, ${width}px]`, async () => {
          window.innerWidth = width;
          const { container, unmount } = render(
            <div className={theme === 'dark' ? 'dark bg-slate-950 text-slate-100' : 'light bg-white text-slate-900'} style={{ width: `${width}px` }}>
              {view.render()}
            </div>
          );

          // Run axe-core with WCAG 2.1 AA rules
          const results = await axe.run(container, {
            runOnly: {
              type: 'tag',
              values: ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'best-practice'],
            },
          });

          // Unmount immediately after audit to abort active controllers and clear timers
          unmount();

          // Log violations for report generation
          if (results.violations.length > 0) {
            console.log(`\n=== AXE VIOLATIONS COUNT: ${results.violations.length} in ${view.name} (${theme}, ${width}px) ===`);
            for (const v of results.violations) {
              console.log(`[VIOLATION:${v.impact?.toUpperCase()}] id=${v.id} help=${v.help}`);
              for (const node of v.nodes) {
                console.log(`  Target: ${node.target.join(' ')}`);
                console.log(`  HTML: ${node.html.slice(0, 100)}...`);
                console.log(`  Failure: ${node.failureSummary}`);
              }
            }
          } else {
            console.log(`\n=== ZERO AXE VIOLATIONS: ${view.name} (${theme}, ${width}px) ===`);
          }

          // Filter for critical and serious violations
          const criticalOrSerious = results.violations.filter(
            v => v.impact === 'critical' || v.impact === 'serious'
          );

          // Expect zero critical or serious violations
          expect(criticalOrSerious, `Critical/Serious accessibility violations found in ${view.name} (${theme}, ${width}px): ${criticalOrSerious.map(v => `${v.id} (${v.impact}): ${v.help}`).join('; ')}`).toEqual([]);
        });
      }
    }
  }
});
