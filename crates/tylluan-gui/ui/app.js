/**
 * TylluanNexus | Sovereign Control Center JS
 */

const API_BASE = window.location.origin;
let authToken = sessionStorage.getItem('tylluan_token') || localStorage.getItem('tylluan_token') || '';

const views = {
    dashboard: document.getElementById('dashboard'),
    'guilds-manager': document.getElementById('guilds-manager'),
    config: document.getElementById('config')
};

const app = {
    charts: { cpu: null, ram: null },

    async init() {
        this.initCharts();
        this.bindEvents();
        this.startPolling();
        this.loadConfig();
        this.loadWslConfig();
        this.loadInferenceProviders();
        this.setupNavigation();
        this.startLogStream(); // Sovereign Log Stream
        this.registerPWA();
        this.loadSilvaGraph();
        this.startAuditPolling();
        this.updateMaintenanceStatus();
    },

    registerPWA() {
        if ('serviceWorker' in navigator) {
            navigator.serviceWorker.register('/service-worker.js')
                .then(() => console.log('🛡️ TylluanNexus PWA: Ready.'))
                .catch(err => console.warn('PWA Registration failed:', err));
        }
    },

    setupNavigation() {
        document.querySelectorAll('.nav-item').forEach(item => {
            item.addEventListener('click', (e) => {
                e.preventDefault();
                const target = item.getAttribute('href').substring(1);
                
                // Toggle views
                Object.keys(views).forEach(v => {
                    views[v].classList.add('hidden');
                });
                if (views[target]) views[target].classList.remove('hidden');

                // Update nav state
                document.querySelectorAll('.nav-item').forEach(i => i.classList.remove('active'));
                item.classList.add('active');
            });
        });
    },

    initCharts() {
        const cpuCtx = document.getElementById('cpuChart').getContext('2d');
        const ramCtx = document.getElementById('ramChart').getContext('2d');

        const chartConfig = (color) => ({
            type: 'doughnut',
            data: {
                datasets: [{
                    data: [0, 100],
                    backgroundColor: [color, 'rgba(255,255,255,0.05)'],
                    borderWidth: 0
                }]
            },
            options: {
                cutout: '80%',
                responsive: true,
                plugins: { legend: { display: false }, tooltip: { enabled: false } }
            }
        });

        this.charts.cpu = new Chart(cpuCtx, chartConfig('#00f2fe'));
        this.charts.ram = new Chart(ramCtx, chartConfig('#6a11cb'));
    },

    async fetchAPI(endpoint, options = {}) {
        const headers = {
            'Content-Type': 'application/json',
            'Authorization': `Bearer ${authToken}`,
            ...options.headers
        };

        try {
            const resp = await fetch(`${API_BASE}${endpoint}`, { ...options, headers });
            
            if (resp.status === 401) {
                this.showLoginModal();
                throw new Error('Auth Required');
            }
            
            if (!resp.ok) throw new Error(await resp.text());
            
            // If we successfully fetched something, hide login if it was visible
            this.hideLoginModal();
            this.setKernelStatus('online');
            
            const contentType = resp.headers.get('content-type');
            if (contentType && contentType.includes('application/json')) {
                return await resp.json();
            }
            return await resp.text();
        } catch (e) {
            console.warn(`API Error [${endpoint}]:`, e.message);
            if (e.message !== 'Auth Required') {
                this.setKernelStatus('offline');
            }
            return null;
        }
    },

    showLoginModal() {
        document.getElementById('login-modal').classList.remove('hidden');
        document.getElementById('login-error').classList.add('hidden');
        this.setKernelStatus('auth');
    },

    hideLoginModal() {
        document.getElementById('login-modal').classList.add('hidden');
    },

    setKernelStatus(type) {
        const badge = document.getElementById('kernel-status');
        const dot = badge.querySelector('.pulse');
        const text = badge.querySelector('span:last-child');

        switch(type) {
            case 'online':
                dot.style.background = '#00ff88';
                text.innerText = 'KERNEL ONLINE';
                break;
            case 'offline':
                dot.style.background = '#ff4b2b';
                text.innerText = 'KERNEL OFFLINE';
                break;
            case 'auth':
                dot.style.background = '#ffcc00';
                text.innerText = 'AUTH REQUIRED';
                break;
        }
    },

    async startPolling() {
        const update = async () => {
            const data = await this.fetchAPI('/api/v1/system/diagnose');
            if (data && data.system) {
                this.lastData = data;
                this.renderStats(data);
            }
        };
        update();
        setInterval(update, 2000);
    },

    renderStats(data) {
        try {
            // Update Gauges
            const cpu = data.system.cpu_usage_percent || 0;
            this.charts.cpu.data.datasets[0].data = [cpu, 100 - cpu];
            this.charts.cpu.update();
            document.getElementById('cpu-val').innerText = Math.round(cpu);

            const used = data.system.used_memory_mb || 0;
            const total = data.system.total_memory_mb || 1;
            this.charts.ram.data.datasets[0].data = [used, total - used];
            this.charts.ram.update();
            document.getElementById('ram-val').innerText = used;

            // Update Info Cards
            document.getElementById('stat-guilds').innerText = (data.guilds || []).length;
            document.getElementById('stat-tools').innerText = (data.guilds || []).reduce((acc, g) => acc + g.tools_count, 0);
            
            // Render Sovereign Intelligence (Fase 3)
            if (data.storage && data.storage.recent_nodes) {
                const intelligenceEl = document.getElementById('intelligence-insights');
                if (data.storage.recent_nodes.length > 0) {
                    intelligenceEl.innerHTML = data.storage.recent_nodes.map(node => `
                        <div class="card insight-card">
                            <h4>${node.node_type}</h4>
                            <div class="insight-content">${this.truncate(node.content, 120)}</div>
                            <div class="insight-meta">ID: ${node.id} | Peso: ${node.weight.toFixed(2)}</div>
                        </div>
                    `).join('');
                } else {
                    intelligenceEl.innerHTML = '<div class="insight-placeholder">Memory empty. Start interacting to generate knowledge.</div>';
                }
            }

            // Render & Filter Guilds
            if (data.guilds) {
                const list = document.getElementById('guild-list-container');
                const filtered = data.guilds.filter(g => 
                    g.name.toLowerCase().includes(this.searchQuery) ||
                    (g.issues || []).some(iss => iss.toLowerCase().includes(this.searchQuery))
                );

                list.innerHTML = filtered.map(g => `
                    <div class="guild-item">
                        <div class="guild-info">
                            <h4>${g.name}</h4>
                            <span>${g.tools_count} tools | ${g.issues.length} alerts</span>
                        </div>
                        <div class="guild-actions">
                            <span class="badge ${g.running ? 'online' : 'offline'}">${g.running ? 'RUNNING' : 'STOPPED'}</span>
                        </div>
                    </div>
                `).join('');

                if (filtered.length === 0 && this.searchQuery) {
                    list.innerHTML = `<div class="log-line system" style="padding: 2rem; text-align: center;">No tools match "${this.searchQuery}"</div>`;
                }
            }
        } catch (e) {
            console.error("Render error:", e);
        }
    },

    async loadConfig() {
        const content = await this.fetchAPI('/api/v1/config');
        if (content && typeof content === 'string') {
            document.getElementById('config-editor').value = content;
        }
    },

    async saveConfig() {
        const content = document.getElementById('config-editor').value;
        const res = await this.fetchAPI('/api/v1/config', {
            method: 'POST',
            body: JSON.stringify({ content })
        });
        
        if (res) {
            this.showToast('✅ Configuración guardada correctamente.');
        } else {
            this.showToast('❌ Error al guardar. Verifica el formato TOML.', true);
        }
    },

    showToast(msg, isError = false) {
        const t = document.getElementById('toast');
        t.innerText = msg;
        t.className = `toast ${isError ? 'error' : 'success'}`;
        setTimeout(() => t.className = 'toast hidden', 4000);
    },

    bindEvents() {
        document.getElementById('btn-load-config').onclick = () => this.loadConfig();
        document.getElementById('btn-save-config').onclick = () => this.saveConfig();
        
        // WSL Proxy buttons
        const btnSaveWsl = document.getElementById('btn-save-wsl');
        if (btnSaveWsl) {
            btnSaveWsl.onclick = () => this.saveWslConfig();
        }

        // Inference Providers buttons
        const btnAddProvider = document.getElementById('btn-add-provider');
        if (btnAddProvider) {
            btnAddProvider.onclick = () => this.addProvider();
        }
        
        const btnExport = document.getElementById('btn-export-brain');
        if (btnExport) {
            btnExport.onclick = () => this.exportBrain();
        }

        // Cycle 6: Terminal Input handling
        const bashInput = document.getElementById('bash-input');
        if (bashInput) {
            bashInput.onkeypress = async (e) => {
                if (e.key === 'Enter') {
                    const cmd = bashInput.value.trim();
                    if (!cmd) return;
                    this.executeBash(cmd);
                    bashInput.value = '';
                }
            };
        }

        // Tab switching for Intelligence Console
        document.querySelectorAll('.tab-btn').forEach(btn => {
            btn.onclick = () => {
                const targetTab = btn.getAttribute('data-tab');
                document.querySelectorAll('.tab-btn').forEach(b => b.classList.remove('active'));
                document.querySelectorAll('.log-content, .thought-content').forEach(c => c.classList.remove('active'));
                
                btn.classList.add('active');
                document.getElementById(targetTab).classList.add('active');
            };
        });

        // Doctor Actions
        const btnDiagnose = document.getElementById('btn-diagnose');
        if (btnDiagnose) {
            btnDiagnose.onclick = async () => {
                this.showToast('🩺 Iniciando escaneo profundo del Kernel...');
                await this.fetchAPI('/api/v1/system/diagnose');
            };
        }

        const btnOptimize = document.getElementById('btn-optimize');
        if (btnOptimize) {
            btnOptimize.onclick = async () => {
                this.showToast('🩹 Iniciando reparación y optimización...');
                console.log("🩹 Command: Doctor, optimize storage.");
            };
        }

        // Tool Search
        const searchInput = document.getElementById('tool-search');
        if (searchInput) {
            searchInput.oninput = (e) => {
                this.searchQuery = e.target.value.toLowerCase();
                if (this.lastData) this.renderStats(this.lastData);
            };
        }

        // Modal Login
        const loginBtn = document.getElementById('btn-login');
        const loginInput = document.getElementById('login-token');
        
        const doLogin = () => {
            const token = loginInput.value.trim();
            if (token) {
                authToken = token;
                sessionStorage.setItem('tylluan_token', token);
                this.hideLoginModal();
                this.loadConfig();
            } else {
                document.getElementById('login-error').classList.remove('hidden');
            }
        };

        loginBtn.onclick = doLogin;
        loginInput.onkeypress = (e) => { if (e.key === 'Enter') doLogin(); };

        document.getElementById('auth-check').onclick = () => this.showLoginModal();

        // Vision Hub Initialization (Cycle 2)
        this.setupVisionHub();
    },

    setupVisionHub() {
        const dropzone = document.getElementById('vision-dropzone');
        const input = document.getElementById('vision-input');
        const preview = document.getElementById('vision-preview');
        const img = document.getElementById('img-preview');
        const btnAnalyze = document.getElementById('btn-analyze-vision');
        let currentStagingPath = null;

        if (!dropzone) return;

        dropzone.onclick = () => input.click();

        dropzone.ondragover = (e) => {
            e.preventDefault();
            dropzone.classList.add('dragover');
        };

        dropzone.ondragleave = () => dropzone.classList.remove('dragover');

        dropzone.ondrop = (e) => {
            e.preventDefault();
            dropzone.classList.remove('dragover');
            const file = e.dataTransfer.files[0];
            if (file) handleUpload(file);
        };

        input.onchange = (e) => {
            const file = e.target.files[0];
            if (file) handleUpload(file);
        };

        const handleUpload = async (file) => {
            this.showToast('📤 Subiendo imagen al núcleo...');
            const formData = new FormData();
            formData.append('file', file);

            try {
                const res = await fetch(`${API_BASE}/api/v1/vision/upload`, {
                    method: 'POST',
                    headers: { 'Authorization': `Bearer ${localStorage.getItem('tylluan_token')}` },
                    body: formData
                });
                const data = await res.json();
                
                if (data.path) {
                    currentStagingPath = data.path;
                    img.src = URL.createObjectURL(file);
                    dropzone.classList.add('hidden');
                    preview.classList.remove('hidden');
                    this.showToast('✅ Imagen en área de preparación.');
                    
                    // Add reasoning thought
                    this.addThoughtCard({
                        thought: `Imagen detectada y cargada en ${data.path}. Preparado para análisis multimodal con Moondream v2.5.`,
                        confidence: 1.0
                    });
                }
            } catch (err) {
                this.showToast('❌ Error en la carga de imagen', true);
            }
        };

        btnAnalyze.onclick = async () => {
            const prompt = document.getElementById('vision-prompt').value;
            const resultBox = document.getElementById('vision-result');
            
            this.showToast('👁️ Analizando con Moondream...');
            btnAnalyze.innerText = '⌛ Analizando...';
            btnAnalyze.disabled = true;

            const res = await this.fetchAPI('/messages', {
                method: 'POST',
                body: JSON.stringify({
                    jsonrpc: "2.0",
                    method: "tools/call",
                    params: {
                        name: "vision_analyze",
                        arguments: { imagePath: currentStagingPath, prompt }
                    },
                    id: Date.now()
                })
            });

            btnAnalyze.innerText = '🚀 Analizar';
            btnAnalyze.disabled = false;

            if (res && res.result) {
                resultBox.innerText = res.result.content[0].text;
                resultBox.classList.remove('hidden');
                this.showToast('✅ Análisis completado.');
            }
        };
    },

    searchQuery: '',
    lastData: null,

    startLogStream() {
        if (this.logStream) this.logStream.close();
        
        const url = `${API_BASE}/sse?sessionId=ui-${Math.random().toString(36).substr(2, 9)}`;
        this.logStream = new EventSource(url);
        
        const logContent = document.getElementById('log-content');
        const thoughtContent = document.getElementById('thought-content');
        
        this.logStream.onmessage = (event) => {
            try {
                // Try to parse as JSON for structured notifications (like "thought")
                if (event.data.startsWith('{')) {
                    const msg = JSON.parse(event.data);
                    if (msg.method === 'thought') {
                        this.addThoughtCard(msg.params);
                        return;
                    }
                }

                // Default log rendering
                const line = document.createElement('div');
                line.className = 'log-line';
                
                const text = event.data;
                if (text.includes('ERROR') || text.includes('❌')) line.classList.add('error');
                else if (text.includes('WARN') || text.includes('⚠️')) line.classList.add('warn');
                else if (text.includes('INFO') || text.includes('🌐')) line.classList.add('system');
                
                const time = new Date().toLocaleTimeString();
                line.innerText = `[${time}] ${text}`;
                logContent.appendChild(line);
                logContent.scrollTop = logContent.scrollHeight;
                
                if (logContent.children.length > 100) {
                    logContent.removeChild(logContent.firstChild);
                }
            } catch (e) {
                console.error("SSE Process error:", e);
            }
        };

        this.logStream.onerror = () => {
            document.getElementById('stream-status').className = 'badge offline';
            document.getElementById('stream-status').innerText = 'SSE DISCONNECTED';
            this.logStream.close();
            setTimeout(() => this.startLogStream(), 5000);
        };
    },

    addThoughtCard(data) {
        const thoughtContent = document.getElementById('thought-content');
        
        // Remove "Waiting" placeholder if it's the first real thought
        if (thoughtContent.innerText.includes('Esperando')) {
            thoughtContent.innerHTML = '';
        }

        const card = document.createElement('div');
        card.className = 'thought-card';
        
        const time = new Date().toLocaleTimeString();
        const confPct = Math.round(data.confidence * 100);
        
        card.innerHTML = `
            <div class="thought-meta">
                <span>🧠 PENSAMIENTO SOBERANO</span>
                <span>${time} | Confianza: ${confPct}%</span>
            </div>
            <div class="thought-text">${data.thought}</div>
            <div class="confidence-bar">
                <div class="confidence-fill" style="width: ${confPct}%"></div>
            </div>
        `;
        
        thoughtContent.prepend(card);
        
        // Keep only top 20 thoughts
        if (thoughtContent.children.length > 20) {
            thoughtContent.removeChild(thoughtContent.lastChild);
        }
    },

    // Cycle 4: Silva Graph Logic (D3.js Implementation)
    async loadSilvaGraph() {
        const svg = d3.select("#silva-svg");
        const container = document.getElementById('silva-graph-container');
        if (!container || !svg.node()) return;

        const width = container.clientWidth;
        const height = container.clientHeight || 500;
        svg.attr("viewBox", [0, 0, width, height]);

        const data = await this.fetchAPI('/api/v1/silva/graph?limit=100');
        if (!data || !data.nodes) return;

        // Limpiar previo
        svg.selectAll("*").remove();

        // Convertir Silva nodes a D3 format
        const nodes = data.nodes.map(d => ({ ...d }));
        const links = data.links || []; // Assume backend might eventually provide links, or we infer them

        const simulation = d3.forceSimulation(nodes)
            .force("link", d3.forceLink(links).id(d => d.id).distance(100))
            .force("charge", d3.forceManyBody().strength(-150))
            .force("center", d3.forceCenter(width / 2, height / 2));

        const link = svg.append("g")
            .attr("class", "links")
            .selectAll("line")
            .data(links)
            .enter().append("line")
            .attr("class", "link");

        const node = svg.append("g")
            .attr("class", "nodes")
            .selectAll("circle")
            .data(nodes)
            .enter().append("circle")
            .attr("class", "node")
            .attr("r", d => 5 + (d.weight * 3))
            .attr("fill", d => this.getNodeColor(d.node_type || d.type))
            .call(this.drag(simulation));

        node.append("title").text(d => `${d.id}\n${d.content}`);

        const label = svg.append("g")
            .attr("class", "labels")
            .selectAll("text")
            .data(nodes)
            .enter().append("text")
            .attr("class", "label")
            .attr("dy", -10)
            .text(d => d.id.split(':').pop());

        simulation.on("tick", () => {
            link.attr("x1", d => d.source.x)
                .attr("y1", d => d.source.y)
                .attr("x2", d => d.target.x)
                .attr("y2", d => d.target.y);

            node.attr("cx", d => d.x)
                .attr("cy", d => d.y);

            label.attr("x", d => d.x)
                .attr("y", d => d.y);
        });
    },

    getNodeColor(type) {
        const colors = {
            'concept': '#00f2fe',
            'entity': '#6a11cb',
            'experience': '#00ff88',
            'lesson': '#f9d423'
        };
        return colors[type] || '#fff';
    },

    drag(simulation) {
        function dragstarted(event) {
            if (!event.active) simulation.alphaTarget(0.3).restart();
            event.subject.fx = event.subject.x;
            event.subject.fy = event.subject.y;
        }
        function dragged(event) {
            event.subject.fx = event.x;
            event.subject.fy = event.y;
        }
        function dragended(event) {
            if (!event.active) simulation.alphaTarget(0);
            event.subject.fx = null;
            event.subject.fy = null;
        }
        return d3.drag()
            .on("start", dragstarted)
            .on("drag", dragged)
            .on("end", dragended);
    },

    // Sovereign Portability Logic
    async updateMaintenanceStatus() {
        const data = await this.fetchAPI('/api/v1/system/maintenance/status');
        if (data) {
            document.getElementById('stat-brain-size').innerText = data.brain_size_human;
            document.getElementById('stat-last-export').innerText = data.last_export;
            document.getElementById('stat-portability').innerText = 'READY';
            
            // Map status for UX
            this.addThoughtCard({
                thought: `Cerebro soberano validado: ${data.brain_size_human}. El núcleo está preparado para exportación térmica.`,
                confidence: 0.98
            });
        }
    },

    async exportBrain() {
        const btn = document.getElementById('btn-export-brain');
        const progress = document.getElementById('export-progress');
        
        btn.disabled = true;
        btn.innerText = '⌛ Exportando...';
        progress.style.width = '30%';
        progress.classList.add('exporting');
        
        this.showToast('📦 Iniciando exportación de 3.6GB...');
        
        const res = await this.fetchAPI('/api/v1/system/maintenance/export', { method: 'POST' });
        
        progress.style.width = '100%';
        progress.classList.remove('exporting');
        
        if (res && res.status === 'ok') {
            this.showToast('✅ Exportación completada con éxito.');
            this.updateMaintenanceStatus();
        } else {
            this.showToast('❌ Error en el proceso de exportación.', true);
        }
        
        btn.disabled = false;
        btn.innerText = '⚡ Exportar';
    },

    // Cycle 6: Bash Execution
    async executeBash(command) {
        const output = document.getElementById('terminal-output');
        const line = document.createElement('div');
        line.className = 'log-line system';
        line.innerText = `> ${command}`;
        output.appendChild(line);

        const res = await this.fetchAPI('/api/v1/bash/execute', {
            method: 'POST',
            body: JSON.stringify({ command })
        });

        const respLine = document.createElement('div');
        respLine.className = 'log-line';
        if (res && res.stdout) {
            respLine.innerText = res.stdout;
        } else if (res && res.error) {
            respLine.className += ' error';
            respLine.innerText = `Error: ${res.error}`;
        } else {
             respLine.className += ' error';
             respLine.innerText = 'Command failed or timed out.';
        }
        output.appendChild(respLine);
        output.scrollTop = output.scrollHeight;
    },

    // Cycle 7: Security Audit Logic
    async startAuditPolling() {
        const poll = async () => {
            const data = await this.fetchAPI('/api/v1/audit/security');
            if (!data || !data.events) return;
            
            const logPanel = document.getElementById('audit-log');
            if (data.events.length === 0 && logPanel.innerText.includes('Buscando')) {
                logPanel.innerHTML = '<div class="log-line system">🛡️ Monitoreo activo: No se han detectado brechas de seguridad.</div>';
            } else if (data.events.length > 0) {
                logPanel.innerHTML = data.events.map(ev => {
                    const levelClass = ev.level.toLowerCase();
                    return `<div class="log-line ${levelClass}">[${ev.timestamp}] ${ev.module.toUpperCase()}: ${ev.message}</div>`;
                }).join('');
                logPanel.scrollTop = logPanel.scrollHeight;
            }
        };
        poll();
        setInterval(poll, 10000);
    },

    // Cycle 8: Knowledge Export
    async exportKnowledge() {
        this.showToast('📥 Preparando exportación de conocimiento...');
        window.open(`${API_BASE}/api/v1/knowledge/export?token=${authToken}`, '_blank');
    },

    truncate(str, n) {
        if (!str) return '';
        return (str.length > n) ? str.substr(0, n - 1) + '...' : str;
    },

    // WSL Proxy Configuration
    async loadWslConfig() {
        // Simple regex-based parser for WSL section
        const configText = await this.fetchAPI('/api/v1/config');
        if (!configText || typeof configText !== 'string') return;

        const enabledMatch = configText.match(/enabled\s*=\s*(\w+)/);
        const autoMatch = configText.match(/auto_detect\s*=\s*(\w+)/);
        const portMatch = configText.match(/fallback_port\s*=\s*(\d+)/);

        if (enabledMatch) document.getElementById('wsl-enabled').value = enabledMatch[1];
        if (autoMatch) document.getElementById('wsl-auto-detect').value = autoMatch[1];
        if (portMatch) document.getElementById('wsl-fallback-port').value = portMatch[1];
    },

    async saveWslConfig() {
        const enabled = document.getElementById('wsl-enabled').value;
        const autoDetect = document.getElementById('wsl-auto-detect').value;
        const port = document.getElementById('wsl-fallback-port').value;

        // Load current config
        const configText = await this.fetchAPI('/api/v1/config');
        if (!configText || typeof configText !== 'string') {
            this.showToast('❌ Error loading config', true);
            return;
        }

        // Simple regex replacement for WSL section
        let newConfig = configText;
        
        // Replace enabled
        if (/enabled\s*=\s*\w+/.test(newConfig)) {
            newConfig = newConfig.replace(/enabled\s*=\s*\w+/, `enabled = ${enabled}`);
        } else {
            newConfig = newConfig.replace(/\[proxy\.wsl\]/, `[proxy.wsl]\nenabled = ${enabled}`);
        }

        // Replace auto_detect
        if (/auto_detect\s*=\s*\w+/.test(newConfig)) {
            newConfig = newConfig.replace(/auto_detect\s*=\s*\w+/, `auto_detect = ${autoDetect}`);
        }

        // Replace fallback_port
        if (/fallback_port\s*=\s*\d+/.test(newConfig)) {
            newConfig = newConfig.replace(/fallback_port\s*=\s*\d+/, `fallback_port = ${port}`);
        }

        const res = await this.fetchAPI('/api/v1/config', {
            method: 'POST',
            body: JSON.stringify({ content: newConfig })
        });

        if (res) {
            this.showToast('✅ WSL config saved. Restart kernel to apply.');
        } else {
            this.showToast('❌ Error saving WSL config', true);
        }
    },

    // Inference Providers Configuration
    async loadInferenceProviders() {
        const data = await this.fetchAPI('/api/v1/config/inference');
        const container = document.getElementById('providers-list');
        
        if (!data || !Array.isArray(data) || data.length === 0) {
            container.innerHTML = '<div class="stat-loading">No providers configured. Click "+" to add.</div>';
            return;
        }

        container.innerHTML = data.map((p, idx) => `
            <div class="provider-item">
                <div class="provider-info">
                    <strong>${p.name}</strong>
                    <span>${p.mcp_server} / ${p.model_id}</span>
                    <span class="badge">${(p.capability || []).join(', ')}</span>
                </div>
                <button class="btn-micro btn-danger" onclick="app.removeProvider(${idx})">🗑️</button>
            </div>
        `).join('');
    },

    async addProvider() {
        const name = prompt('Provider name (e.g., Ollama):');
        if (!name) return;
        
        const mcp_server = prompt('MCP server name (e.g., ollama):');
        if (!mcp_server) return;
        
        const model_id = prompt('Model ID (e.g., llama3.2):');
        if (!model_id) return;

        const capability = prompt('Capabilities (comma-separated: chat,vision,thinking):', 'chat');
        
        const provider = {
            name,
            mcp_server,
            model_id,
            capability: capability ? capability.split(',').map(c => c.trim()) : ['chat']
        };

        const res = await this.fetchAPI('/api/v1/config/inference', {
            method: 'POST',
            body: JSON.stringify(provider)
        });

        if (res) {
            this.showToast('✅ Provider added. Restart kernel to apply.');
            this.loadInferenceProviders();
        } else {
            this.showToast('❌ Error adding provider', true);
        }
    },

    async removeProvider(idx) {
        if (!confirm('Remove this provider?')) return;
        // For now, just reload - full removal requires backend endpoint
        this.loadInferenceProviders();
    },

    // Guilds Manager with auto-refresh, memory estimate, and Unload All
    guildManagerInterval: null,
    coreGuilds: ['bash', 'filesystem', 'memory', 'git'],

    async loadGuildsManager() {
        const data = await this.fetchAPI('/api/v1/guilds');
        const container = document.getElementById('guilds-manager-list');
        
        if (!data || !Array.isArray(data)) {
            container.innerHTML = '<div class="stat-loading">Failed to load guilds.</div>';
            return;
        }

        const running = data.filter(g => g.running).length;
        const total = data.length;
        const nonCoreRunning = running - data.filter(g => this.coreGuilds.includes(g.name) && g.running).length;
        
        const ramEstimateMB = Math.round(running * 55);
        
        container.innerHTML = `
            <div class="guilds-summary">
                <span>Active: ${running}/${total}</span>
                <span>Est. RAM: ~${ramEstimateMB}MB</span>
                ${running > 10 ? '<span class="warning">⚠️ HIGH</span>' : ''}
            </div>
            <div class="guilds-actions-bar">
                <button class="btn-micro" onclick="app.unloadAllNonCore()">UNLOAD ALL NON-CORE</button>
                <button class="btn-micro btn-success" onclick="app.loadAllLazy()">LOAD ALL LAZY</button>
            </div>
        ` + data.map(g => {
            const isCore = this.coreGuilds.includes(g.name);
            const memEst = g.running ? '~55MB' : '-';
            return `
            <div class="guild-manager-item ${isCore ? 'core-guild' : ''}">
                <div class="guild-manager-info">
                    <strong>${isCore ? '★ ' : ''}${g.name}</strong>
                    <span>${g.tools_count || 0}T</span>
                    <span class="mem-est">${memEst}</span>
                    <span class="badge ${g.running ? 'online' : 'offline'}">${g.running ? 'RUN' : 'STOP'}</span>
                </div>
                <div class="guild-manager-actions">
                    ${g.running 
                        ? `<button class="btn-micro btn-danger" onclick="app.stopGuild('${g.name}')">STOP</button>`
                        : `<button class="btn-micro btn-success" onclick="app.startGuild('${g.name}')">START</button>`
                    }
                </div>
            </div>
            `;
        }).join('');

        if (running > 10) {
            this.showToast('⚠️ Memory warning: ' + running + ' guilds', true);
        }
    },

    startGuildsAutoRefresh() {
        if (this.guildManagerInterval) return;
        this.guildManagerInterval = setInterval(() => this.loadGuildsManager(), 8000);
    },

    stopGuildsAutoRefresh() {
        if (this.guildManagerInterval) {
            clearInterval(this.guildManagerInterval);
            this.guildManagerInterval = null;
        }
    },

    async unloadAllNonCore() {
        if (!confirm('Unload all non-core guilds?')) return;
        let unloaded = 0;
        const data = await this.fetchAPI('/api/v1/guilds');
        for (const g of data || []) {
            if (g.running && !this.coreGuilds.includes(g.name)) {
                await this.fetchAPI(`/api/v1/guilds/${g.name}/stop`, { method: 'POST' });
                unloaded++;
            }
        }
        this.showToast(`Unloaded ${unloaded} guilds.`);
        this.loadGuildsManager();
    },

    async loadAllLazy() {
        if (!confirm('Load all lazy guilds? This may use significant RAM.')) return;
        let loaded = 0;
        const data = await this.fetchAPI('/api/v1/guilds');
        for (const g of data || []) {
            if (!g.running && !this.coreGuilds.includes(g.name)) {
                await this.fetchAPI(`/api/v1/guilds/${g.name}/start`, { method: 'POST' });
                loaded++;
            }
        }
        this.showToast(`Loaded ${loaded} guilds.`);
        this.loadGuildsManager();
    },

    async startGuild(name) {
        const res = await this.fetchAPI(`/api/v1/guilds/${name}/start`, { method: 'POST' });
        if (res && (res.status === 'ok' || res.running)) {
            this.showToast(`Guild "${name}" started.`);
            this.loadGuildsManager();
        } else {
            this.showToast(`Failed to start "${name}"`, true);
        }
    },

    async stopGuild(name) {
        if (!confirm(`Stop guild "${name}"?`)) return;
        const res = await this.fetchAPI(`/api/v1/guilds/${name}/stop`, { method: 'POST' });
        if (res && (res.status === 'ok' || !res.running)) {
            this.showToast(`Guild "${name}" stopped.`);
            this.loadGuildsManager();
        } else {
            this.showToast(`Failed to stop "${name}"`, true);
        }
    }
};

window.onload = () => app.init();
