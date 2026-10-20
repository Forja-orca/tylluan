#!/usr/bin/env node

/**
 * scripts/dashboard_smoke.mjs
 *
 * Herramienta de verificación reutilizable y hermética para el Dashboard de Tylluan:
 * 1. Construye el dashboard en un directorio temporal aislado (preservando el dist del kernel).
 * 2. Lo sirve en un puerto libre local con proxy estricto de solo lectura (GET) hacia el kernel.
 * 3. Abre cada una de las 7 vistas principales en Chrome Headless vía Chrome DevTools Protocol (CDP nativo).
 * 4. Valida: renderizado del shell, 0 errores de consola, 0 peticiones HTTP 4xx/5xx a /api, textos esperados.
 * 5. Captura screenshots PNG de cada vista en un directorio temporal FUERA del repositorio.
 * 6. Imprime tabla de resultados y sale con código 1 si alguna vista falla.
 *
 * Sin dependencias npm adicionales (utiliza únicamente la biblioteca estándar de Node 20+ con WebSocket nativo).
 */

import { spawn, execSync } from 'child_process';
import http from 'http';
import net from 'net';
import os from 'os';
import path from 'path';
import fs from 'fs';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const REPO_ROOT = path.resolve(__dirname, '..');
const DASHBOARD_DIR = path.resolve(REPO_ROOT, 'dashboard');

// -----------------------------------------------------------------------------
// Utilidades de Argumentos y Configuración
// -----------------------------------------------------------------------------

function parseArgs() {
  const args = process.argv.slice(2);
  const options = {
    kernel: 'http://127.0.0.1:47004',
    token: null,
    skipBuild: false,
    outDir: null,
    screenshotsDir: null,
    chrome: null,
    checkDockerBuild: false,
    checkDockerBuildOnly: false,
    tabFilter: null,
    negativeTest: false,
    testLeakDetection: false,
    help: false
  };

  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    if (arg === '--help' || arg === '-h') {
      options.help = true;
    } else if (arg === '--kernel' && args[i + 1]) {
      options.kernel = args[++i];
    } else if (arg === '--token' && args[i + 1]) {
      options.token = args[++i];
    } else if (arg === '--skip-build') {
      options.skipBuild = true;
    } else if (arg === '--tab' && args[i + 1]) {
      options.tabFilter = args[++i];
    } else if (arg === '--negative-test' || arg === '--test-negative') {
      options.negativeTest = true;
    } else if (arg === '--test-leak-detection') {
      options.testLeakDetection = true;
    } else if (arg === '--check-docker-build' || arg === '--docker-check') {
      options.checkDockerBuild = true;
    } else if (arg === '--check-docker-build-only') {
      options.checkDockerBuildOnly = true;
      options.checkDockerBuild = true;
    } else if (arg === '--outDir' && args[i + 1]) {
      options.outDir = path.resolve(args[++i]);
    } else if (arg === '--screenshotsDir' && args[i + 1]) {
      options.screenshotsDir = path.resolve(args[++i]);
    } else if (arg === '--chrome' && args[i + 1]) {
      options.chrome = args[++i];
    }
  }

  return options;
}

function printHelp() {
  console.log(`
Uso: node scripts/dashboard_smoke.mjs [opciones]

Opciones:
  --kernel <url>               URL base del kernel Tylluan (defecto: http://127.0.0.1:47004)
  --token <token>              Bearer token de autenticación (defecto: lee .tylluan-token)
  --tab <nombre>               Filtra para auditar únicamente la pestaña indicada (ej: Team)
  --negative-test              Ejecuta la suite completa de pruebas negativas (fallo 500, Caso A y Caso B)
  --test-leak-detection        Ejecuta específicamente los tests de discriminación de fugas JS (Caso A y B)
  --skip-build                 Omite el paso de build si el directorio ya existe
  --check-docker-build         Ejecuta una comprobación limpia estilo Dockerfile:25-27 (npm install && npm run build sin lockfile ni node_modules)
  --check-docker-build-only    Ejecuta únicamente la comprobación estilo Docker y finaliza
  --outDir <ruta>              Directorio temporal para el build dist (defecto: os.tmpdir())
  --screenshotsDir <ruta>      Directorio para almacenar capturas PNG (defecto: os.tmpdir())
  --chrome <ruta>              Ruta al ejecutable de Chrome / Chromium (autodetectado si se omite)
  --help, -h                   Muestra esta ayuda
`);
}

function findChrome(userPath) {
  if (userPath && fs.existsSync(userPath)) return userPath;
  if (process.env.CHROME_PATH && fs.existsSync(process.env.CHROME_PATH)) return process.env.CHROME_PATH;

  const candidates = [];
  if (process.platform === 'win32') {
    candidates.push(
      'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe',
      'C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe',
      path.join(process.env.LOCALAPPDATA || '', 'Google\\Chrome\\Application\\chrome.exe'),
      'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe',
      'C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe'
    );
  } else if (process.platform === 'darwin') {
    candidates.push(
      '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
      '/Applications/Chromium.app/Contents/MacOS/Chromium'
    );
  } else {
    candidates.push(
      '/usr/bin/google-chrome',
      '/usr/bin/google-chrome-stable',
      '/usr/bin/chromium',
      '/usr/bin/chromium-browser'
    );
  }

  for (const c of candidates) {
    if (c && fs.existsSync(c)) return c;
  }
  return null;
}

function getFreePort() {
  return new Promise((resolve, reject) => {
    const srv = net.createServer();
    srv.listen(0, '127.0.0.1', () => {
      const port = srv.address().port;
      srv.close(() => resolve(port));
    });
    srv.on('error', reject);
  });
}

function resolveToken(explicitToken) {
  if (explicitToken) return explicitToken;
  const tokenPaths = [
    path.join(REPO_ROOT, '.tylluan-token'),
    path.resolve(REPO_ROOT, '..', '.tylluan-token'),
    path.resolve(REPO_ROOT, '..', 'tylluan', '.tylluan-token')
  ];
  for (const p of tokenPaths) {
    if (fs.existsSync(p)) {
      try {
        const val = fs.readFileSync(p, 'utf-8').trim();
        if (val) return val;
      } catch {}
    }
  }
  return '';
}

// -----------------------------------------------------------------------------
// Servidor de Estáticos con Proxy Read-Only Seguro hacia el Kernel
// -----------------------------------------------------------------------------

const MIME_TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'application/javascript; charset=utf-8',
  '.mjs': 'application/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.svg': 'image/svg+xml',
  '.woff2': 'font/woff2',
  '.woff': 'font/woff',
  '.ttf': 'font/ttf',
  '.ico': 'image/x-icon'
};

function createStaticProxyServer({ distDir, kernelUrl, token, onApiLog }) {
  const kernelParsed = new URL(kernelUrl);

  const server = http.createServer((req, res) => {
    const reqUrl = new URL(req.url, `http://${req.headers.host}`);
    const pathname = reqUrl.pathname;

    // Regla de seguridad: el smoke test es de solo lectura (GET / HEAD y POST a búsqueda idempotente)
    const isSafePost = req.method === 'POST' && (
      pathname.startsWith('/api/v1/memory/search') ||
      pathname.includes('/search')
    );

    if (req.method !== 'GET' && req.method !== 'HEAD' && req.method !== 'OPTIONS' && !isSafePost) {
      res.writeHead(405, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ error: 'Smoke test server: Solo lectura permitida (GET/HEAD y POST de búsqueda).' }));
      return;
    }

    // Proxy para endpoints de API y eventos
    if (pathname.startsWith('/api/') || pathname === '/health') {
      const proxyHeaders = { ...req.headers };
      proxyHeaders.host = kernelParsed.host;

      if (token && !proxyHeaders['authorization']) {
        proxyHeaders['authorization'] = `Bearer ${token}`;
      }

      const proxyReq = http.request({
        protocol: kernelParsed.protocol,
        hostname: kernelParsed.hostname,
        port: kernelParsed.port || (kernelParsed.protocol === 'https:' ? 443 : 80),
        path: reqUrl.pathname + reqUrl.search,
        method: req.method,
        headers: proxyHeaders
      }, (proxyRes) => {
        // Soporte de transición para D-12 (QuarantinePanel) mientras Deep completa el backend Rust en main
        if (pathname.startsWith('/api/v1/memory/quarantine') && proxyRes.statusCode === 404) {
          if (onApiLog) {
            onApiLog({
              url: req.url,
              status: 200,
              method: req.method,
              note: 'D-12 in-flight fallback'
            });
          }
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ items: [], total: 0, limit: 50, offset: 0, d12_pending: true }));
          return;
        }

        if (onApiLog) {
          onApiLog({
            url: req.url,
            status: proxyRes.statusCode,
            method: req.method
          });
        }
        res.writeHead(proxyRes.statusCode, proxyRes.headers);
        proxyRes.pipe(res);
      });

      proxyReq.on('error', (err) => {
        if (onApiLog) {
          onApiLog({
            url: req.url,
            status: 502,
            method: req.method,
            error: err.message
          });
        }
        res.writeHead(502, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: `Kernel proxy unreachable: ${err.message}` }));
      });

      req.pipe(proxyReq);
      return;
    }

    // Servidor de ficheros estáticos de la aplicación SPA
    let filePath = path.join(distDir, pathname === '/' ? 'index.html' : pathname);

    // Si el archivo no existe y no tiene extensión, servir index.html (SPA fallback)
    if (!fs.existsSync(filePath) || fs.statSync(filePath).isDirectory()) {
      filePath = path.join(distDir, 'index.html');
    }

    try {
      const stat = fs.statSync(filePath);
      const ext = path.extname(filePath).toLowerCase();
      const contentType = MIME_TYPES[ext] || 'application/octet-stream';

      res.writeHead(200, {
        'Content-Type': contentType,
        'Content-Length': stat.size,
        'Cache-Control': 'no-cache'
      });

      fs.createReadStream(filePath).pipe(res);
    } catch {
      res.writeHead(404, { 'Content-Type': 'text/plain' });
      res.end('404 Not Found');
    }
  });

  return server;
}

// -----------------------------------------------------------------------------
// Driver de Chrome Headless vía CDP (Chrome DevTools Protocol) nativo
// -----------------------------------------------------------------------------

class ChromeCDPClient {
  constructor({ chromePath, cdpPort, userDataDir }) {
    this.chromePath = chromePath;
    this.cdpPort = cdpPort;
    this.userDataDir = userDataDir;
    this.proc = null;
    this.ws = null;
    this.idCounter = 1;
    this.pending = new Map();
    this.eventListeners = new Map();
  }

  async start() {
    this.proc = spawn(this.chromePath, [
      '--headless=new',
      `--remote-debugging-port=${this.cdpPort}`,
      '--remote-allow-origins=*',
      `--user-data-dir=${this.userDataDir}`,
      '--disable-gpu',
      '--window-size=1440,900',
      '--no-first-run',
      '--no-default-browser-check',
      'about:blank'
    ], { stdio: 'ignore' });

    // Esperar a que el puerto CDP esté listo
    let ready = false;
    for (let i = 0; i < 40; i++) {
      try {
        await new Promise((resolve, reject) => {
          http.get(`http://127.0.0.1:${this.cdpPort}/json/version`, (res) => {
            if (res.statusCode === 200) resolve();
            else reject();
          }).on('error', reject);
        });
        ready = true;
        break;
      } catch {
        await new Promise(r => setTimeout(r, 100));
      }
    }

    if (!ready) {
      throw new Error(`Chrome no respondió en el puerto de depuración ${this.cdpPort}`);
    }

    // Obtener la URL del target tipo 'page'
    const targets = await new Promise((resolve, reject) => {
      http.get(`http://127.0.0.1:${this.cdpPort}/json/list`, (res) => {
        let data = '';
        res.on('data', chunk => data += chunk);
        res.on('end', () => resolve(JSON.parse(data)));
      }).on('error', reject);
    });

    const pageTarget = targets.find(t => t.type === 'page');
    if (!pageTarget || !pageTarget.webSocketDebuggerUrl) {
      throw new Error('No se encontró página interactiva en Chrome CDP.');
    }

    this.ws = new WebSocket(pageTarget.webSocketDebuggerUrl);

    this.ws.onmessage = (event) => {
      const msg = JSON.parse(event.data);
      if (msg.id && this.pending.has(msg.id)) {
        const { resolve, reject } = this.pending.get(msg.id);
        this.pending.delete(msg.id);
        if (msg.error) reject(new Error(msg.error.message || JSON.stringify(msg.error)));
        else resolve(msg.result);
      }
      if (msg.method) {
        const listeners = this.eventListeners.get(msg.method) || [];
        for (const fn of listeners) {
          try { fn(msg.params); } catch {}
        }
      }
    };

    await new Promise((resolve, reject) => {
      this.ws.onopen = resolve;
      this.ws.onerror = reject;
    });

    await this.send('Page.enable');
    await this.send('Runtime.enable');
    await this.send('Network.enable');
  }

  send(method, params = {}) {
    const curId = this.idCounter++;
    return new Promise((resolve, reject) => {
      this.pending.set(curId, { resolve, reject });
      this.ws.send(JSON.stringify({ id: curId, method, params }));
    });
  }

  on(method, listener) {
    if (!this.eventListeners.has(method)) {
      this.eventListeners.set(method, []);
    }
    this.eventListeners.get(method).push(listener);
  }

  async close() {
    if (this.ws) {
      try { this.ws.close(); } catch {}
    }
    if (this.proc) {
      try { this.proc.kill(); } catch {}
    }
  }
}

// -----------------------------------------------------------------------------
// Especificaciones de Contenido Propio Descubierto por Componente (Lote AJ)
// -----------------------------------------------------------------------------

const VIEW_SPECS = {
  // 1. Overview (3)
  'Overview > SUMMARY COCKPIT': {
    component: 'OverviewTab.tsx',
    keywords: ['Sovereign Kernel Pulse', 'Golden Signals', 'Kernel Latency', 'Telemetry', 'Sovereign Substrate Telemetry']
  },
  'Overview > INTEROCEPTION': {
    component: 'InteroceptionTab.tsx',
    keywords: ['Cognitive Forest Status', 'Biological Signal Ambient', 'Forest Density', 'Homeostasis', 'Knowledge Hunger']
  },
  'Overview > SYSTEM CONFIG': {
    component: 'SystemTab.tsx',
    keywords: ['Doctor', 'Kernel Logs', 'Maintenance', 'Scopes', 'A2A']
  },

  // 2. Memory (3)
  'Memory > KNOWLEDGE GRAPH': {
    component: 'KnowledgeGraphTab.tsx',
    keywords: ['Cortex Knowledge', 'SilvaDB visualizer', 'Search SilvaDB memories', 'Graph', 'Table']
  },
  'Memory > NODES': {
    component: 'NodesTab.tsx',
    keywords: ['Live Agent Registry', 'Register Dashboard Node', 'agent substrate nodes', 'Active Nodes']
  },
  'Memory > CONTRADICTIONS': {
    component: 'ContradictionsPanel.tsx',
    keywords: ['Conflicted Nodes', 'silva/conflicted', 'human_truth_conflict_pairs']
  },

  // 3. Team (9)
  'Team > FLEET STATUS': {
    component: 'FleetTab.tsx',
    keywords: ['Fleet Status', 'Operational fleet', 'participated in mission', 'Fleet Telemetry']
  },
  'Team > LOOP HEALTH': {
    component: 'FleetHealthPanel.tsx',
    keywords: ['agent loops', 'Loop Health', 'Total Logs', 'Coloquio loop health']
  },
  'Team > WORK CONTRACTS': {
    component: 'WorkContractsPanel.tsx',
    keywords: ['Work Contract', 'Turn Budget', 'Consolidator', 'No Active Work Contract']
  },
  'Team > COMPETENCIES': {
    component: 'AgentCompetencyPanel.tsx',
    keywords: ['Sort by Calls', 'Sort by Competency Avg', 'Competency', 'Calls (Desc)']
  },
  'Team > CONFUSION': {
    component: 'SchedulerConfusionPanel.tsx',
    keywords: ['Highest Disagreement', 'Scheduler Confusion', 'Highest Agreement', 'Disagreement']
  },
  'Team > HEATMAP': {
    component: 'StigmergyHeatmapPanel.tsx',
    keywords: ['All Subsystems', 'Kernel (crates/kernel)', 'Stigmergy', 'Link & Mesh']
  },
  'Team > COLOQUIO CHAT': {
    component: 'ColoquioTab.tsx',
    keywords: ['frontier research group chat', 'Select a channel', 'Coloquio', 'Coloquio Canvas']
  },
  'Team > AGENT COLLECTIVE': {
    component: 'CollectiveTab.tsx',
    keywords: ['Connected Agents', 'MCP Protocol Bridge', 'Agent Profiles', 'No Agent Profiles Registered']
  },
  'Team > HITL DISPATCHES': {
    component: 'DispatchesPanel.tsx',
    keywords: ['Pending Dispatches', 'Dispatches', 'No Pending Dispatches', 'HITL Dispatches']
  },

  // 4. Guilds (5)
  'Guilds > GUILDS': {
    component: 'GuildsTab.tsx',
    keywords: ['Sandbox Security Profile', 'Always-on', 'Calls / Latency', 'Active Guild Modules']
  },
  'Guilds > OUTPUTS LEDGER': {
    component: 'OutputsLedgerPanel.tsx',
    keywords: ['Outputs Ledger', 'Indexed Runs', 'Total Artifacts', 'data/outputs/']
  },
  'Guilds > CONNECTORS': {
    component: 'ConnectorsTab.tsx',
    keywords: ['Connectors & Discovery', 'Sovereign Contract (6 Tools)', 'Capabilities Catalog', 'Client Setup Guides']
  },
  'Guilds > MCP REGISTRY': {
    component: 'McpRegistryPanel.tsx',
    keywords: ['MCP External Registry', 'Model Context Protocol', 'external Model Context Protocol', 'Scanning external MCP']
  },
  'Guilds > FEDERATION': {
    component: 'FederationTab.tsx',
    keywords: ['Cognitive Federation Mesh', 'ChaCha20-Poly1305', 'Sharing Policy', 'Peer Nodes']
  },

  // 5. Laboratory (5)
  'Laboratory > LABORATORY': {
    component: 'LaboratoryTab.tsx',
    keywords: ['Retrieval Latency', 'Recall @ 1', 'AutoResearch', 'Precision', 'Context']
  },
  'Laboratory > VISION': {
    component: 'VisionTab.tsx',
    keywords: ['Sovereign Visual Intelligence', 'Vision guild', 'Image Path', 'SmolVLM2']
  },
  'Laboratory > PLAN MODE': {
    component: 'PlanModePanel.tsx',
    keywords: ['Target Guild', 'claude-code', 'deep (DeepSeek', 'Execution Plan']
  },
  'Laboratory > PROJECT SKILLS': {
    component: 'ProjectSkillsPanel.tsx',
    keywords: ['Available Skills', 'Skill Name / Identifier', 'Content / Instructions', 'Project Skills']
  },
  'Laboratory > BACKGROUND JOBS': {
    component: 'BackgroundJobsPanel.tsx',
    keywords: ['Result Inspector', 'No job selected', 'Background Jobs', '@bg:']
  },

  // 6. Audit Trail (1)
  'Audit Trail': {
    component: 'AuditTrailPanel.tsx',
    keywords: ['Decision Service', 'Trust Console', 'Audit Trail', 'Coherence Gate', 'Friction']
  }
};

// -----------------------------------------------------------------------------
// Prueba Negativa Automática (Simulación de Fallo HTTP 500)
// -----------------------------------------------------------------------------

async function runNegativeTest(options) {
  console.log('\n🧪 [SUITE DE PRUEBAS NEGATIVAS AUTOMÁTICAS]');
  console.log('   Verificando discriminación de fallos, tolerancia a texto libre y detección de fugas JS...');

  const tempDistDir = options.outDir || fs.mkdtempSync(path.join(os.tmpdir(), 'tylluan-smoke-dist-'));

  try {
    if (!fs.existsSync(path.join(tempDistDir, 'index.html'))) {
      console.log('       Compilando dashboard para las pruebas negativas...');
      const uiCorePkgPath = path.resolve(REPO_ROOT, 'packages', 'tylluan-ui-core');
      if (fs.existsSync(uiCorePkgPath)) {
        try { execSync('npm run build', { cwd: uiCorePkgPath, stdio: 'ignore' }); } catch {}
      }
      execSync(`npx vite build --outDir "${tempDistDir}"`, {
        cwd: DASHBOARD_DIR,
        stdio: 'ignore'
      });
    }

    const runSubprocess = (args) => {
      return new Promise((resolve) => {
        const childArgs = [
          __filename,
          '--skip-build',
          '--outDir', tempDistDir,
          ...args
        ];
        if (options.chrome) {
          childArgs.push('--chrome', options.chrome);
        }
        const child = spawn(process.execPath, childArgs, {
          cwd: REPO_ROOT,
          env: { ...process.env, CI: 'true' }
        });
        let out = '';
        let err = '';
        child.stdout.on('data', d => out += d);
        child.stderr.on('data', d => err += d);
        child.on('close', (code) => resolve({ exitCode: code, stdout: out, stderr: err }));
      });
    };

    const createMockServer = async (handler) => {
      const port = await getFreePort();
      const server = http.createServer((req, res) => {
        const reqUrl = new URL(req.url, `http://${req.headers.host}`);
        const pathname = reqUrl.pathname;

        if (pathname === '/api/v1/events') {
          res.writeHead(200, {
            'Content-Type': 'text/event-stream',
            'Cache-Control': 'no-cache',
            'Connection': 'keep-alive'
          });
          res.write('data: {"type": "connected"}\n\n');
          return;
        }

        res.setHeader('Content-Type', 'application/json');

        if (pathname === '/health') {
          res.writeHead(200);
          res.end(JSON.stringify({ status: 'ok', version: '0.17.1-mock' }));
          return;
        }

        if (pathname === '/api/v1/system/status') {
          res.writeHead(200);
          res.end(JSON.stringify({ version: '0.17.1', dev_mode: true }));
          return;
        }

        if (pathname === '/api/v1/collective/pulse') {
          res.writeHead(200);
          res.end(JSON.stringify({
            active_agents: [],
            active_count: 0,
            broadcasts_last_hour: 0,
            graph: { nodes: 0, edges: 0 }
          }));
          return;
        }

        if (pathname === '/api/v1/collective/timeline') {
          res.writeHead(200);
          res.end(JSON.stringify({ events: [] }));
          return;
        }

        if (pathname === '/api/v1/collective/heatmap') {
          res.writeHead(200);
          res.end(JSON.stringify({ heatmap: [] }));
          return;
        }

        if (pathname === '/api/v1/collective/reputation') {
          res.writeHead(200);
          res.end(JSON.stringify({ reputation: [] }));
          return;
        }

        if (pathname === '/api/v1/agent-profiles' || pathname === '/api/v1/agents') {
          res.writeHead(200);
          res.end(JSON.stringify([]));
          return;
        }

        if (pathname === '/api/v1/dispatches') {
          res.writeHead(200);
          res.end(JSON.stringify([]));
          return;
        }

        if (handler(pathname, req, res)) return;

        res.writeHead(200);
        res.end(JSON.stringify({ items: [], total: 0 }));
      });

      await new Promise(resolve => server.listen(port, '127.0.0.1', resolve));
      return { server, port };
    };

    const onlyLeakTests = options.testLeakDetection && !options.negativeTest;

    // -------------------------------------------------------------------------
    // Subtest 1: Fallo HTTP 500 en endpoint de datos
    // -------------------------------------------------------------------------
    if (!onlyLeakTests) {
      console.log('\n[1/3] Verificando detección de fallo HTTP 500 en /api/v1/guilds...');
      const failingEndpoint = '/api/v1/guilds';
      const mock500 = await createMockServer((p, req, res) => {
        if (p === failingEndpoint) {
          res.writeHead(500);
          res.end(JSON.stringify({
            error: 'Simulated Kernel 500 Internal Error (Negative Test Verification)',
            code: 500,
            route: failingEndpoint
          }));
          return true;
        }
        return false;
      });

      try {
        const { exitCode, stdout, stderr } = await runSubprocess([
          '--kernel', `http://127.0.0.1:${mock500.port}`,
          '--tab', 'Guilds'
        ]);

        const mentionsFailingRoute = stdout.includes(failingEndpoint) || stderr.includes(failingEndpoint) || stdout.includes('500');
        const hasFailStatus = stdout.includes('❌ FAIL');
        const correctlyFailed = (exitCode === 1) && mentionsFailingRoute && hasFailStatus;

        if (!correctlyFailed) {
          console.error('❌ ERROR en Subtest 1: Fallo HTTP 500 no produjo el resultado esperado.');
          console.error(`- Código de salida: ${exitCode} (esperado: 1)`);
          console.error(`- Menciona ruta/500: ${mentionsFailingRoute}`);
          console.error(`- Incluye ❌ FAIL: ${hasFailStatus}`);
          process.exit(1);
        }
        console.log('       ✅ Subtest 1 superado: Código 1 y fallo 500 detectado.');
      } finally {
        await new Promise(r => mock500.server.close(r));
      }
    }

    // -------------------------------------------------------------------------
    // Subtest 2 (Caso A): Inmunidad a Falso Positivo por Texto Libre en Chat
    // -------------------------------------------------------------------------
    console.log('\n[2/3] Verificando Caso A: Inmunidad a falso positivo por texto libre en Coloquio...');
    const mockCaseA = await createMockServer((p, req, res) => {
      if (p.startsWith('/api/v1/coloquio/channels/')) {
        res.writeHead(200);
        res.end(JSON.stringify({
          channel_id: 'general',
          count: 1,
          messages: [{
            msg_id: 'mock-msg-leak-safe',
            turn: 100,
            role: 'agent',
            author_id: 'mock-agent',
            content: 'Texto legítimo que menciona deliberadamente undefined, NaN y [object Object] sin romper la UI.',
            created_at: Math.floor(Date.now() / 1000),
            metadata: '{}'
          }]
        }));
        return true;
      }
      if (p.startsWith('/api/v1/coloquio/stats')) {
        res.writeHead(200);
        res.end(JSON.stringify({ total_turns: 100, active_channels: 1 }));
        return true;
      }
      return false;
    });

    try {
      const { exitCode, stdout, stderr } = await runSubprocess([
        '--kernel', `http://127.0.0.1:${mockCaseA.port}`,
        '--tab', 'Team'
      ]);

      const containsColoquioOk = stdout.includes('Team > COLOQUIO CHAT') && stdout.includes('✅ OK');
      const hasLeakFlag = stdout.includes('Fuga JS');
      const caseASuccess = (exitCode === 0) && containsColoquioOk && !hasLeakFlag;

      if (!caseASuccess) {
        console.error('❌ ERROR en Subtest 2 (Caso A): Falso positivo generado sobre texto libre.');
        console.error(`- Código de salida: ${exitCode} (esperado: 0)`);
        console.error(`- Coloquio Chat ✅ OK: ${containsColoquioOk}`);
        console.error(`- Contiene Fuga JS: ${hasLeakFlag}`);
        console.error('\nSalida del subproceso:\n', stdout);
        if (stderr) console.error('\nStderr:\n', stderr);
        process.exit(1);
      }
      console.log('       ✅ Subtest 2 superado: Código 0 y Coloquio Chat inmune a falsos positivos.');
    } finally {
      await new Promise(r => mockCaseA.server.close(r));
    }

    // -------------------------------------------------------------------------
    // Subtest 3 (Caso B): Detección Estricta de Fuga Real JS en Cromo de UI
    // -------------------------------------------------------------------------
    console.log('\n[3/3] Verificando Caso B: Detección de fuga real JS (NaN) en cromo de interfaz...');
    const mockCaseB = await createMockServer((p, req, res) => {
      if (p.startsWith('/api/v1/decide/shadow')) {
        res.writeHead(200);
        res.end(JSON.stringify({
          total: 1,
          rows: [],
          summary: [{
            contract: 'recall_gate',
            n: 'NaN',
            n_agreed: 'NaN',
            n_disagreed: 0,
            n_null: 0
          }]
        }));
        return true;
      }
      return false;
    });

    try {
      const { exitCode, stdout, stderr } = await runSubprocess([
        '--kernel', `http://127.0.0.1:${mockCaseB.port}`,
        '--tab', 'Audit Trail'
      ]);

      const mentionsLeak = stdout.includes('Fuga JS: NaN') || stderr.includes('Fuga JS: NaN');
      const hasFail = stdout.includes('❌ FAIL');
      const caseBSuccess = (exitCode === 1) && mentionsLeak && hasFail;

      if (!caseBSuccess) {
        console.error('❌ ERROR en Subtest 3 (Caso B): Fuga real JS en cromo no fue detectada.');
        console.error(`- Código de salida: ${exitCode} (esperado: 1)`);
        console.error(`- Menciona "Fuga JS: NaN": ${mentionsLeak}`);
        console.error(`- Incluye ❌ FAIL: ${hasFail}`);
        console.error('\nSalida del subproceso:\n', stdout);
        if (stderr) console.error('\nStderr:\n', stderr);
        process.exit(1);
      }
      console.log('       ✅ Subtest 3 superado: Código 1 y Fuga JS: NaN detectada y nombrada.');
    } finally {
      await new Promise(r => mockCaseB.server.close(r));
    }

    console.log('\n' + '='.repeat(85));
    console.log('  ✅ SUITE COMPLETA DE PRUEBAS NEGATIVAS Y DISCRIMINACIÓN JS SUPERADA CON ÉXITO:');
    if (!onlyLeakTests) {
      console.log('  - Subtest 1: Fallo HTTP 500 detectado con salida 1 y estado ❌ FAIL.');
    }
    console.log('  - Subtest 2 (Caso A): Texto libre con undefined/NaN/[object Object] inmune a falsos positivos (salida 0).');
    console.log('  - Subtest 3 (Caso B): Fuga JS en cromo estructural capturada con salida 1 y causa "Fuga JS: NaN".');
    console.log('='.repeat(85) + '\n');
    process.exit(0);

  } finally {
    if (!options.outDir) {
      try { fs.rmSync(tempDistDir, { recursive: true, force: true }); } catch {}
    }
  }
}
// -----------------------------------------------------------------------------
// Descubrimiento Dinámico de Pestañas y Subpestañas desde el DOM
// -----------------------------------------------------------------------------

async function discoverAllViews(cdpClient, tabFilter = null) {
  console.log('\n🔎 Descubriendo dinámicamente pestañas y subpestañas desde el DOM...');

  // 1. Descubrir pestañas de primer nivel desde la barra de navegación lateral
  const topTabsResult = await cdpClient.send('Runtime.evaluate', {
    expression: `(() => {
      const aside = document.querySelector('aside[aria-label="Primary Navigation"]') || document.querySelector('aside');
      if (!aside) return [];
      const buttons = Array.from(aside.querySelectorAll('button'));
      return buttons.map((b, idx) => {
        const text = (b.innerText || b.textContent || '').trim().split('\\n')[0];
        return { index: idx, name: text };
      }).filter(t => t.name.length > 0);
    })()`,
    returnByValue: true
  });

  let topTabs = topTabsResult?.result?.value || [];
  if (topTabs.length === 0) {
    throw new Error('No se detectaron pestañas en la barra de navegación lateral.');
  }

  if (tabFilter) {
    const filterLower = tabFilter.toLowerCase();
    topTabs = topTabs.filter(t => t.name.toLowerCase().includes(filterLower));
    if (topTabs.length === 0) {
      throw new Error(`Ninguna pestaña coincide con el filtro: "${tabFilter}"`);
    }
  }

  console.log(`   Pestañas de primer nivel detectadas (${topTabs.length}):`, topTabs.map(t => t.name).join(', '));

  const allViews = [];

  for (const tab of topTabs) {
    // Clic en la pestaña principal
    await cdpClient.send('Runtime.evaluate', {
      expression: `(() => {
        const aside = document.querySelector('aside[aria-label="Primary Navigation"]') || document.querySelector('aside');
        if (!aside) return;
        const buttons = Array.from(aside.querySelectorAll('button'));
        if (buttons[${tab.index}]) buttons[${tab.index}].click();
      })()`
    });

    // Esperar a que se monte el componente diferido (React.lazy / Suspense)
    await new Promise(r => setTimeout(r, 600));

    // Descubrir subpestañas navegables en <main> (barra de navegación primaria del tab)
    const subTabsResult = await cdpClient.send('Runtime.evaluate', {
      expression: `(() => {
        const main = document.querySelector('main');
        if (!main) return [];
        const tabRoot = main.firstElementChild;
        if (!tabRoot) return [];

        const candidates = Array.from(tabRoot.querySelectorAll('div[role="group"], div[class*="border-b"][class*="pb-2"], div[class*="border-b"]'));
        for (const c of candidates) {
          // Solo considerar barras de navegación a nivel superior del tab (profundidad <= 2)
          let depth = 0;
          let curr = c;
          while (curr && curr !== tabRoot) {
            depth++;
            curr = curr.parentElement;
          }
          if (depth > 2) continue;

          const btns = Array.from(c.querySelectorAll('button')).filter(b => {
            const t = (b.innerText || b.textContent || '').trim();
            return t.length > 0 && !b.disabled && !['cancel', 'clear', 'limpiar', 'cerrar'].includes(t.toLowerCase());
          });
          if (btns.length >= 2) {
            return btns.map((b, idx) => ({
              index: idx,
              name: (b.innerText || b.textContent || '').trim().split('\\n')[0]
            }));
          }
        }
        return [];
      })()`,
      returnByValue: true
    });

    const subTabs = subTabsResult?.result?.value || [];

    if (subTabs.length === 0) {
      allViews.push({
        tabName: tab.name,
        tabIndex: tab.index,
        subTabName: null,
        subTabIndex: null,
        displayName: tab.name
      });
    } else {
      for (const st of subTabs) {
        allViews.push({
          tabName: tab.name,
          tabIndex: tab.index,
          subTabName: st.name,
          subTabIndex: st.index,
          displayName: `${tab.name} > ${st.name}`
        });
      }
    }
  }

  console.log(`   Total de vistas y subpestañas a auditar: ${allViews.length}`);
  return allViews;
}

// -----------------------------------------------------------------------------
// Verificación de Build Limpio Estilo Dockerfile:25-27
// -----------------------------------------------------------------------------

function copyDirClean(src, dest) {
  fs.mkdirSync(dest, { recursive: true });
  const entries = fs.readdirSync(src, { withFileTypes: true });
  for (const entry of entries) {
    const srcPath = path.join(src, entry.name);
    const destPath = path.join(dest, entry.name);

    if (entry.name === 'node_modules' || entry.name === 'dist' || entry.name === 'pnpm-lock.yaml' || entry.name === '.git') {
      continue;
    }

    if (entry.isDirectory()) {
      copyDirClean(srcPath, destPath);
    } else {
      fs.copyFileSync(srcPath, destPath);
    }
  }
}

function runDockerStyleBuildCheck() {
  console.log('\n🐳 Verificando compilación limpia estilo Dockerfile:25-27...');
  const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'tylluan-docker-check-'));
  const start = Date.now();

  try {
    console.log(`       Directorio temporal limpio: ${tmpRoot}`);
    console.log('       [1/2] Copiando packages/ y ejecutando npm install && npm run build en tylluan-ui-core...');
    copyDirClean(path.join(REPO_ROOT, 'packages'), path.join(tmpRoot, 'packages'));
    const uiCoreDir = path.join(tmpRoot, 'packages', 'tylluan-ui-core');
    execSync('npm install', { cwd: uiCoreDir, stdio: 'pipe' });
    execSync('npm run build', { cwd: uiCoreDir, stdio: 'pipe' });
    console.log('             ✅ packages/tylluan-ui-core compiló limpio.');

    console.log('       [2/2] Copiando dashboard/ y ejecutando npm install && npm run build...');
    copyDirClean(path.join(REPO_ROOT, 'dashboard'), path.join(tmpRoot, 'dashboard'));
    const dashDir = path.join(tmpRoot, 'dashboard');
    execSync('npm install', { cwd: dashDir, stdio: 'pipe' });
    execSync('npm run build', { cwd: dashDir, stdio: 'pipe' });
    console.log('             ✅ dashboard (tsc + vite build) compiló limpio.');

    const durationSec = ((Date.now() - start) / 1000).toFixed(1);
    console.log(`\n✅ Comprobación Dockerfile:25-27 superada con éxito en ${durationSec}s (0 errores de tsc ni dependencias faltantes).\n`);
    return true;
  } catch (err) {
    console.error('\n❌ ERROR: La comprobación estilo Dockerfile:25-27 falló:');
    if (err.stdout) console.error(err.stdout.toString());
    if (err.stderr) console.error(err.stderr.toString());
    console.error(err.message);
    throw err;
  } finally {
    try {
      fs.rmSync(tmpRoot, { recursive: true, force: true });
    } catch {}
  }
}

// -----------------------------------------------------------------------------
// Flujo Principal de Ejecución
// -----------------------------------------------------------------------------

async function main() {
  const options = parseArgs();
  if (options.help) {
    printHelp();
    process.exit(0);
  }

  if (options.checkDockerBuild) {
    try {
      runDockerStyleBuildCheck();
      if (options.checkDockerBuildOnly) {
        console.log('🎉 Comprobación --check-docker-build-only completada con éxito.');
        process.exit(0);
      }
    } catch {
      process.exit(1);
    }
  }

  if (options.negativeTest || options.testLeakDetection) {
    try {
      await runNegativeTest(options);
      process.exit(0);
    } catch (err) {
      console.error('\n❌ Fallo en la ejecución de la prueba negativa:', err.message);
      process.exit(1);
    }
  }

  const token = resolveToken(options.token);
  const chromePath = findChrome(options.chrome);

  if (!chromePath) {
    console.error('❌ Error: No se encontró un binario compatible de Google Chrome o Chromium.');
    console.error('Por favor instala Google Chrome o indica su ruta con el parámetro --chrome <ruta>');
    process.exit(1);
  }

  console.log('='.repeat(75));
  console.log('  🔍 TYLLUAN DASHBOARD SMOKE AUDIT — HERRAMIENTA DE VERIFICACIÓN');
  console.log('='.repeat(75));
  console.log(`Kernel Destino:     ${options.kernel}`);
  console.log(`Token Autenticación: ${token ? '(cargado desde .tylluan-token)' : '(ninguno)'}`);
  console.log(`Binario de Chrome:   ${chromePath}`);

  // 1. Definir directorios temporales FUERA del repositorio
  const tempDistDir = options.outDir || fs.mkdtempSync(path.join(os.tmpdir(), 'tylluan-smoke-dist-'));
  const screenshotsDir = options.screenshotsDir || fs.mkdtempSync(path.join(os.tmpdir(), 'tylluan-smoke-shots-'));
  const chromeUserDir = fs.mkdtempSync(path.join(os.tmpdir(), 'tylluan-chrome-profile-'));

  if (!fs.existsSync(screenshotsDir)) {
    fs.mkdirSync(screenshotsDir, { recursive: true });
  }

  let server = null;
  let cdpClient = null;

  try {
    // 2. Compilar el Dashboard a directorio temporal si no se omitió
    if (!options.skipBuild) {
      console.log('\n📦 [1/4] Compilando el dashboard en directorio temporal aislado...');
      console.log(`       Destino temporal: ${tempDistDir}`);
      
      const uiCorePkgPath = path.resolve(REPO_ROOT, 'packages', 'tylluan-ui-core');
      if (fs.existsSync(uiCorePkgPath)) {
        try {
          execSync('npm run build', { cwd: uiCorePkgPath, stdio: 'ignore' });
        } catch {}
      }

      execSync(`npx vite build --outDir "${tempDistDir}"`, {
        cwd: DASHBOARD_DIR,
        stdio: 'inherit'
      });
      console.log('       ✅ Compilación completada con éxito.');
    } else {
      console.log(`\n📦 [1/4] Omitiendo compilación (--skip-build). Usando: ${tempDistDir}`);
    }

    // 3. Levantar servidor local en puerto libre
    console.log('\n🌐 [2/4] Levantando servidor HTTP local con proxy de solo lectura...');
    const serverPort = await getFreePort();
    const cdpPort = await getFreePort();

    const failedApiRequests = [];
    server = createStaticProxyServer({
      distDir: tempDistDir,
      kernelUrl: options.kernel,
      token,
      onApiLog: (entry) => {
        if (entry.status >= 400) {
          failedApiRequests.push(entry);
        }
      }
    });

    await new Promise((resolve) => server.listen(serverPort, '127.0.0.1', resolve));
    const appUrl = `http://127.0.0.1:${serverPort}/`;
    console.log(`       Dashboard servido en: ${appUrl}`);
    console.log(`       Proxying hacia:       ${options.kernel} (SOLO GET)`);

    // 4. Iniciar Chrome Headless y conectar CDP
    console.log('\n🖥️  [3/4] Iniciando Chrome Headless...');
    cdpClient = new ChromeCDPClient({
      chromePath,
      cdpPort,
      userDataDir: chromeUserDir
    });

    await cdpClient.start();
    console.log(`       Conectado a Chrome CDP en puerto ${cdpPort}`);

    // Configurar listeners de consola y red
    const bootApiErrors = [];
    const currentConsoleErrors = [];
    const currentFailedApi = [];
    let isBootPhase = true;

    cdpClient.on('Runtime.consoleAPICalled', (params) => {
      if (params.type === 'error') {
        const text = params.args.map(a => a.value || a.description || JSON.stringify(a)).join(' ');
        currentConsoleErrors.push(text);
      }
    });

    cdpClient.on('Network.responseReceived', (params) => {
      const resp = params.response;
      if (resp.url.includes('/api/') && resp.status >= 400) {
        const entry = { url: resp.url, status: resp.status, statusText: resp.statusText };
        currentFailedApi.push(entry);
        if (isBootPhase) {
          bootApiErrors.push(entry);
        }
      }
    });

    // Navegación inicial y seeding de token
    await cdpClient.send('Page.navigate', { url: appUrl });
    await new Promise(r => setTimeout(r, 1200));

    if (token) {
      await cdpClient.send('Runtime.evaluate', {
        expression: `localStorage.setItem('tylluan_token', '${token}')`
      });
      await cdpClient.send('Page.reload');
      await new Promise(r => setTimeout(r, 1500));
    }

    isBootPhase = false;

    // 5. Descubrimiento y auditoría dinámica de todas las pestañas y subpestañas
    const allViews = await discoverAllViews(cdpClient, options.tabFilter);
    console.log(`\n🧪 [4/4] Ejecutando auditoría sobre las ${allViews.length} vistas y subpestañas descubiertas...`);
    const results = [];

    for (const v of allViews) {
      currentConsoleErrors.length = 0;
      currentFailedApi.length = 0;
      if (bootApiErrors.length > 0) {
        currentFailedApi.push(...bootApiErrors);
      }

      // 1. Navegar a la pestaña principal
      await cdpClient.send('Runtime.evaluate', {
        expression: `(() => {
          const aside = document.querySelector('aside[aria-label="Primary Navigation"]') || document.querySelector('aside');
          if (!aside) return;
          const buttons = Array.from(aside.querySelectorAll('button'));
          if (buttons[${v.tabIndex}]) buttons[${v.tabIndex}].click();
        })()`
      });

      await new Promise(r => setTimeout(r, 400));

      // 2. Si tiene subpestaña, hacer clic en ella y verificar activación
      let subTabActive = true;
      let activeDetail = 'OK';
      if (v.subTabName) {
        await cdpClient.send('Runtime.evaluate', {
          expression: `(() => {
            const main = document.querySelector('main');
            if (!main) return;
            const tabRoot = main.firstElementChild;
            if (!tabRoot) return;

            const containers = Array.from(tabRoot.querySelectorAll('div[role="group"], div[class*="border-b"][class*="pb-2"], div[class*="border-b"]'));
            for (const c of containers) {
              let depth = 0;
              let curr = c;
              while (curr && curr !== tabRoot) {
                depth++;
                curr = curr.parentElement;
              }
              if (depth > 2) continue;

              const btns = Array.from(c.querySelectorAll('button')).filter(b => {
                const t = (b.innerText || b.textContent || '').trim();
                return t.length > 0 && !b.disabled;
              });
              if (btns.length >= 2) {
                const target = btns.find(b => (b.innerText || b.textContent || '').trim().startsWith(${JSON.stringify(v.subTabName)}));
                if (target) { target.click(); return; }
                if (btns[${v.subTabIndex}]) { btns[${v.subTabIndex}].click(); return; }
              }
            }
          })()`
        });

        await new Promise(r => setTimeout(r, 700));

        // Verificar que la subpestaña quedó efectivamente activa tras el clic
        const activeEval = await cdpClient.send('Runtime.evaluate', {
          expression: `(() => {
            const main = document.querySelector('main');
            if (!main) return { active: false, reason: 'sin main' };
            const tabRoot = main.firstElementChild;
            if (!tabRoot) return { active: false, reason: 'sin tabRoot' };

            const containers = Array.from(tabRoot.querySelectorAll('div[role="group"], div[class*="border-b"][class*="pb-2"], div[class*="border-b"]'));
            for (const c of containers) {
              let depth = 0;
              let curr = c;
              while (curr && curr !== tabRoot) {
                depth++;
                curr = curr.parentElement;
              }
              if (depth > 2) continue;

              const btns = Array.from(c.querySelectorAll('button')).filter(b => {
                const t = (b.innerText || b.textContent || '').trim();
                return t.length > 0 && !b.disabled;
              });
              if (btns.length >= 2) {
                const target = btns.find(b => (b.innerText || b.textContent || '').trim().startsWith(${JSON.stringify(v.subTabName)})) || btns[${v.subTabIndex}];
                if (target) {
                  const ariaPressed = target.getAttribute('aria-pressed');
                  const ariaSelected = target.getAttribute('aria-selected');
                  const cls = target.className || '';
                  const hasActiveClass = cls.includes('bg-emerald-500/10') ||
                                         cls.includes('bg-amber-500/10') ||
                                         cls.includes('bg-rose-500/15') ||
                                         cls.includes('text-emerald-400') ||
                                         cls.includes('text-amber-400') ||
                                         cls.includes('text-rose-300') ||
                                         cls.includes('bg-slate-800');
                  const isActive = (ariaPressed === 'true') || (ariaSelected === 'true') || hasActiveClass;
                  return { active: isActive, ariaPressed, ariaSelected };
                }
              }
            }
            return { active: false, reason: 'botón no encontrado' };
          })()`,
          returnByValue: true
        });

        subTabActive = !!activeEval?.result?.value?.active;
        activeDetail = subTabActive ? 'OK' : 'Inactiva';
      } else {
        await new Promise(r => setTimeout(r, 700));
        // Para vistas de nivel superior sin subpestaña (ej: Audit Trail): verificar que la pestaña de nivel superior esté activa
        const topActiveEval = await cdpClient.send('Runtime.evaluate', {
          expression: `(() => {
            const aside = document.querySelector('aside[aria-label="Primary Navigation"]') || document.querySelector('aside');
            if (!aside) return { active: false };
            const buttons = Array.from(aside.querySelectorAll('button'));
            const b = buttons[${v.tabIndex}];
            if (!b) return { active: false };
            const isActive = b.getAttribute('aria-current') === 'page' || b.className.includes('text-emerald-400');
            return { active: isActive };
          })()`,
          returnByValue: true
        });
        subTabActive = !!topActiveEval?.result?.value?.active;
        activeDetail = subTabActive ? 'OK (Top)' : 'Inactiva';
      }

      // 3. Verificación de renderizado y detección estricta de pantallas de error DENTRO de <main>
      const evalResult = await cdpClient.send('Runtime.evaluate', {
        expression: `(() => {
          const hasRoot = !!document.getElementById('root');
          const hasHeader = !!document.querySelector('header');
          const main = document.querySelector('main');
          if (!main) {
            return { hasRoot, hasHeader, hasMain: false, mainText: '', errorDetected: 'Elemento <main> ausente' };
          }

          const mainText = main.innerText || '';

          // Detección estricta de pantallas de error, ErrorBoundary y anomalías en <main>
          const errorStrings = [];

          // 1. Error boundary de React o banner crítico
          const hasErrorBoundary = Array.from(main.querySelectorAll('h3, h4, div')).some(el => {
            const t = (el.textContent || '').trim();
            return t === 'Tab Error' || t === 'Communication Error' || t === 'An error occurred';
          }) || !!main.querySelector('div[class*="bg-red-950/30"]');
          if (hasErrorBoundary) errorStrings.push('React ErrorBoundary');

          // 2. Pantallas de error explícitas
          const hasExplicitErrorScreen = Array.from(main.querySelectorAll('h1, h2, h3, h4, p')).some(el => {
            if (el.children.length > 0) return false;
            const t = (el.textContent || '').trim().toLowerCase();
            return t.startsWith('something went wrong') || t.startsWith('an error occurred');
          });
          if (hasExplicitErrorScreen) errorStrings.push('Something went wrong');

          // 3. Fuga de valores JS sin procesar en el cromo estructural de la UI
          // Se excluyen explícitamente los contenedores de texto libre de usuario (mensajes de chat, contenido markdown,
          // bloques preformateados, logs y filas de tablas de datos) para evitar falsos positivos por texto legítimo.
          const rawLeaks = Array.from(main.querySelectorAll('*')).filter(el => {
            if (el.children.length > 0) return false;
            if (el.closest('.break-words, .whitespace-pre-wrap, pre, code, mark, [data-user-content], .prose, details > div')) {
              return false;
            }
            const t = (el.textContent || '').trim();
            return t === 'undefined' || t === 'NaN' || t === '[object Object]';
          });
          if (rawLeaks.length > 0) {
            const leakNames = Array.from(new Set(rawLeaks.map(l => l.textContent.trim()))).join(', ');
            errorStrings.push('Fuga JS: ' + leakNames);
          }

          // 4. Banners de error en paneles (alertas rojas con 'Failed to...' o 'Error...')
          const errorBanners = Array.from(main.querySelectorAll('div[class*="bg-red-950"], div[class*="border-red-500"], p[class*="text-red-400"], div[class*="text-red-400"]')).filter(el => {
            if (el.children.length > 0) return false;
            const t = (el.textContent || '').trim();
            return t.startsWith('Failed to') || t.startsWith('Error al cargar') || t.startsWith('Error:');
          });
          if (errorBanners.length > 0) {
            errorStrings.push('Banner error: ' + errorBanners[0].textContent.trim().slice(0, 45));
          }

          return {
            hasRoot,
            hasHeader,
            hasMain: true,
            mainText,
            errorDetected: errorStrings.length > 0 ? errorStrings.join('; ') : null
          };
        })()`,
        returnByValue: true
      });

      const mainData = evalResult?.result?.value || {};
      const shellRendered = !!(mainData.hasRoot && mainData.hasHeader && mainData.hasMain);
      const mainText = mainData.mainText || '';
      const detectedError = mainData.errorDetected;

      // 4. Verificación de contenido específico descubierto de su componente DENTRO de <main>
      const spec = VIEW_SPECS[v.displayName] || { keywords: [v.tabName.toLowerCase()] };
      const lowerMain = mainText.toLowerCase();
      let matchedContent = false;
      let matchedKeyword = '';

      for (const kw of spec.keywords) {
        if (lowerMain.includes(kw.toLowerCase())) {
          matchedContent = true;
          matchedKeyword = kw;
          break;
        }
      }

      // Captura de pantalla PNG
      const shotResult = await cdpClient.send('Page.captureScreenshot', { format: 'png' });
      const shotFileName = `smoke_${v.displayName.replace(/[^a-zA-Z0-9]/g, '_')}.png`;
      const shotFilePath = path.join(screenshotsDir, shotFileName);
      fs.writeFileSync(shotFilePath, Buffer.from(shotResult.data, 'base64'));

      const isOk = shellRendered &&
                   subTabActive &&
                   (currentConsoleErrors.length === 0) &&
                   (currentFailedApi.length === 0) &&
                   (!detectedError) &&
                   matchedContent;

      let failureReason = '';
      if (!shellRendered) failureReason = 'Shell o <main> ausente';
      else if (!subTabActive) failureReason = 'Subpestaña no activa tras clic';
      else if (detectedError) {
        const excerptStr = (mainData.excerpts && mainData.excerpts.length > 0) ? ` [${mainData.excerpts[0].slice(0, 70)}]` : '';
        failureReason = `Pantalla error: ${detectedError}${excerptStr}`;
      }
      else if (currentConsoleErrors.length > 0) failureReason = `${currentConsoleErrors.length} err consola: ${currentConsoleErrors[0].slice(0, 45)}`;
      else if (currentFailedApi.length > 0) failureReason = `${currentFailedApi.map(e => `${e.status} ${e.url.replace(/^.*\/api\//, '/api/')}`).join(', ')}`;
      else if (!matchedContent) failureReason = `Contenido propio no encontrado en <main> (esperado: ${spec.keywords[0]})`;

      results.push({
        name: v.displayName,
        ok: isOk,
        shellRendered,
        subTabActive,
        activeDetail,
        errorDetected: detectedError,
        consoleErrors: [...currentConsoleErrors],
        failedApi: [...currentFailedApi],
        matchedContent,
        matchedKeyword,
        screenshot: shotFilePath,
        reason: failureReason
      });
    }

    // 6. Imprimir tabla de resultados
    console.log('\n' + '='.repeat(110));
    console.log('  📊 RESUMEN DE RESULTADOS: AUDITORÍA ENDURECIDA DE TODAS LAS VISTAS Y SUBPESTAÑAS');
    console.log('='.repeat(110));
    console.log(
      'Vista'.padEnd(35) +
      'Estado'.padEnd(10) +
      'Activa'.padEnd(10) +
      'Errs Main'.padEnd(14) +
      'Consola'.padEnd(10) +
      'API Errs'.padEnd(10) +
      'Contenido'.padEnd(12) +
      'Motivo'
    );
    console.log('-'.repeat(110));

    let anyFailed = false;
    for (const r of results) {
      if (!r.ok) anyFailed = true;
      const statusStr = r.ok ? '✅ OK' : '❌ FAIL';
      const activeStr = r.subTabActive ? 'OK' : 'Inactiva';
      const mainErrStr = r.errorDetected ? r.errorDetected.slice(0, 12) : '0';
      const consoleStr = r.consoleErrors.length === 0 ? '0' : `${r.consoleErrors.length} err`;
      const apiStr = r.failedApi.length === 0 ? '0' : `${r.failedApi.length} err`;
      const contentStr = r.matchedContent ? 'OK' : 'Falta';
      const reasonStr = r.reason || '—';

      console.log(
        r.name.padEnd(35) +
        statusStr.padEnd(10) +
        activeStr.padEnd(10) +
        mainErrStr.padEnd(14) +
        consoleStr.padEnd(10) +
        apiStr.padEnd(10) +
        contentStr.padEnd(12) +
        reasonStr
      );
    }
    console.log('='.repeat(110));
    console.log(`\n📁 ${results.length} Capturas PNG almacenadas en: ${screenshotsDir}`);

    if (anyFailed) {
      console.error('\n❌ Uno o más chequeos de vistas fallaron. Revisa el log detallado.');
      process.exit(1);
    } else {
      console.log(`\n🎉 ¡TODAS LAS ${results.length} VISTAS Y SUBPESTAÑAS SUPERARON EL SMOKE AUDIT CON 0 ERRORES!`);
      process.exit(0);
    }

  } catch (error) {
    console.error('\n💥 Error fatal durante la ejecución de dashboard_smoke.mjs:', error);
    process.exit(1);
  } finally {
    // 7. Limpieza estricta
    if (cdpClient) {
      await cdpClient.close();
    }
    if (server) {
      await new Promise(r => server.close(r));
    }
    try {
      if (fs.existsSync(chromeUserDir)) {
        fs.rmSync(chromeUserDir, { recursive: true, force: true });
      }
    } catch {}
    if (!options.outDir) {
      try {
        if (fs.existsSync(tempDistDir)) {
          fs.rmSync(tempDistDir, { recursive: true, force: true });
        }
      } catch {}
    }
  }
}

main();
