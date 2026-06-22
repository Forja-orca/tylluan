import os
import subprocess
import time
import requests
import sqlite3
import json
import uuid
import sys

# --- CONFIGURACIÓN SOBERANA ---
BASE_DIR = r"e:\TylluanMCPo3"
KERNEL_BIN = os.path.join(BASE_DIR, "target", "release", "tylluan-nexus.exe")
DB_MAILBOX = os.path.join(BASE_DIR, "data", "mailbox.db")
DB_SILVA = os.path.join(BASE_DIR, "data", "silva.db")
PORT = 3033
TOKEN = os.environ.get("TYLLUAN_TOKEN", "")
BASE_URL = f"http://localhost:{PORT}"

class TylluanE2EHarness:
    def __init__(self):
        self.process = None
        self.results = {"P1": "FAILED_INIT", "P2": "NOT_STARTED", "P4": "NOT_STARTED", "P5": "NOT_STARTED"}
        self.db_mailbox = DB_MAILBOX
        self.db_silva = DB_SILVA

    def log(self, msg):
        print(f"[E2E] {msg}", flush=True)

    def run_all(self):
        try:
            self.p1_start_and_handshake()
            if self.results["P1"] == "PASSED":
                self.p2_lesson_flow_mcp()
                self.p4_endpoints_check()
                self.p5_security_check()
            else:
                self.log("Skipping remaining tests due to P1 failure.")
        except Exception as e:
            msg = str(e).encode('ascii', 'replace').decode('ascii')
            self.log(f"CRITICAL ERROR during harness execution: {msg}")
        finally:
            self.generate_report()
            self.cleanup()

    def p1_start_and_handshake(self):
        self.log("P1: Starting Kernel and verifying Handshaking...")
        
        try:
            import psutil
            for proc in psutil.process_iter(['pid', 'name']):
                if "tylluan-nexus" in (proc.info['name'] or ""):
                    self.log(f"Cleaning up existing kernel process {proc.info['pid']}")
                    proc.terminate()
        except:
            pass

        self.process = subprocess.Popen(
            [KERNEL_BIN, "--no-tui", "--port", str(PORT)],
            cwd=BASE_DIR,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            encoding='utf-8',
            errors='replace',
            bufsize=1
        )
        
        start_time = time.time()
        timeout = 240 # 4 mins
        ready_guilds = set()
        required_guilds = {"bash", "memory", "filesystem", "docker", "code", "monitor", "browser"}
        
        while time.time() - start_time < timeout:
            line = self.process.stdout.readline()
            if not line:
                if self.process.poll() is not None:
                    self.log(f"Kernel process exited prematurely with code {self.process.returncode}")
                    break
                continue
                
            line_str = line.strip()
            clean_line = line_str.encode('ascii', 'replace').decode('ascii')
            if clean_line:
                print(f"  [KERNEL] {clean_line}", flush=True)

            if "Mailbox path:" in clean_line:
                self.db_mailbox = clean_line.split("Mailbox path:")[1].strip()
                self.log(f"DETECTED MAILBOX DB: {self.db_mailbox}")
            
            if "SilvaDB path:" in clean_line:
                self.db_silva = clean_line.split("SilvaDB path:")[1].strip()
                self.log(f"DETECTED SILVA DB: {self.db_silva}")
            
            # Handle emoji prefix (🌲 SilvaDB path:)
            if "SilvaDB path:" in clean_line and "🌲" in clean_line:
                self.db_silva = clean_line.split("SilvaDB path:")[1].strip()
                self.log(f"DETECTED SILVA DB: {self.db_silva}")

            if "connected to '" in clean_line:
                for rg in required_guilds:
                    if f"connected to '{rg}'" in clean_line:
                        ready_guilds.add(rg)
                        self.log(f"Guild '{rg}' is CONNECTED.")
            
            if "' ready:" in clean_line and "registered" in clean_line:
                for rg in required_guilds:
                    if f"'{rg}' ready:" in clean_line:
                        ready_guilds.add(rg)
                        self.log(f"Guild '{rg}' is READY.")
            
            if "HTTP server listening" in clean_line:
                self.log(f"Kernel listening on port {PORT}.")
            
            # Ensure HTTP server is actually responsive before declaring P1 passed
            if required_guilds.issubset(ready_guilds):
                self.log(f"All {len(ready_guilds)} guilds ready. Waiting for HTTP server to be fully responsive...")
                # Poll health endpoint until it's ready
                import requests
                http_ready = False
                for _ in range(20):  # Try for up to 10 seconds
                    try:
                        r = requests.get(f"{BASE_URL}/health", timeout=1)
                        if r.status_code == 200:
                            http_ready = True
                            break
                    except:
                        pass
                    time.sleep(0.5)
                
                if http_ready:
                    self.results["P1"] = "PASSED"
                    self.log(f"P1 PASSED: All {len(ready_guilds)} core guilds verified and HTTP ready.")
                    return
                else:
                    self.log("WARNING: HTTP server not responding but proceeding anyway...")

        self.results["P1"] = f"FAILED (Connected: {list(ready_guilds)})"

    def p2_lesson_flow_mcp(self):
        self.log("P2: Validating Lesson Flow via MCP Gatillo...")
        payload_path = os.path.join(BASE_DIR, "tests", "e2e", "data", "sample_lesson.json")
        try:
            with open(payload_path, "r") as f:
                lesson_content = json.load(f)

            # Formatear como MCP tool call para agent_send_mail
            mcp_payload = {
                "jsonrpc": "2.0",
                "id": str(uuid.uuid4()),
                "method": "tools/call",
                "params": {
                    "name": "agent_send_mail",
                    "arguments": {
                        "receiverId": "hub",
                        "payload": lesson_content
                    }
                }
            }

            self.log(f"Sending MCP lesson proposal via /messages...")
            headers = {"Authorization": f"Bearer {TOKEN}", "Content-Type": "application/json"}
            r = requests.post(f"{BASE_URL}/messages", headers=headers, json=mcp_payload, timeout=20)
            
            if r.status_code != 200:
                self.results["P2"] = f"FAILED_GATEWAY (Status: {r.status_code})"
                self.log(f"Gateway Error: {r.text}")
                return

            self.log("MCP Trigger sent successfully. Polling SilvaDB for persistence...")
            
            start_poll = time.time()
            found = False
            while time.time() - start_poll < 60: # 60s max for consensus/indexing
                if os.path.exists(self.db_silva):
                    conn = sqlite3.connect(self.db_silva)
                    cursor = conn.cursor()
                    # Querying for content present in sample_lesson.json
                    cursor.execute("SELECT id, content FROM nodes WHERE type = 'lesson' AND content LIKE '%autonomous guild orchestration%'")
                    node = cursor.fetchone()
                    conn.close()
                    
                    if node:
                        self.log(f"Node found in SilvaDB: {node[0]}")
                        # Verificar embedding
                        conn = sqlite3.connect(self.db_silva)
                        cursor = conn.cursor()
                        cursor.execute("SELECT node_id FROM node_embeddings WHERE node_id = ?", (node[0],))
                        emb = cursor.fetchone()
                        conn.close()
                        
                        if emb:
                            self.results["P2"] = "PASSED"
                            self.log("P2 PASSED: Lesson processed via MCP, persisted and indexed.")
                            found = True
                            break
                        else:
                            self.log("Node exists, waiting for embedding...")
                
                time.sleep(5)

            if not found:
                self.results["P2"] = "FAILED_PERSISTENCE (Timeout)"
        except Exception as e:
            self.results["P2"] = f"ERROR: {str(e)}"
            self.log(f"P2 Error: {e}")

    def p4_endpoints_check(self):
        self.log("P4: Verifying HTTP Endpoints...")
        try:
            r = requests.get(f"{BASE_URL}/health", timeout=10)
            if r.status_code == 200:
                self.results["P4"] = "PASSED"
            else:
                self.results["P4"] = f"FAILED (Status: {r.status_code})"
        except Exception as e:
            self.results["P4"] = f"FAILED ({e})"

    def p5_security_check(self):
        self.log("P5: Testing Security (Tokens)...")
        try:
            r1 = requests.get(f"{BASE_URL}/api/v1/guilds", timeout=10) # Protected path
            r2 = requests.get(f"{BASE_URL}/api/v1/guilds", headers={"Authorization": f"Bearer {TOKEN}"}, timeout=10)
            if r1.status_code in [401, 403] and r2.status_code == 200:
                self.results["P5"] = "PASSED"
                self.log("P5 PASSED: Access control verified.")
            else:
                self.results["P5"] = f"FAILED (no-auth={r1.status_code}, auth={r2.status_code})"
        except Exception as e:
            self.results["P5"] = f"ERROR: {e}"

    def generate_report(self):
        report_dir = os.path.join(BASE_DIR, "tests", "e2e", "reports")
        if not os.path.exists(report_dir):
            os.makedirs(report_dir)
        report_path = os.path.join(report_dir, f"report_{int(time.time())}.json")
        with open(report_path, "w") as f:
            json.dump(self.results, f, indent=2)
        self.log(f"Report generated at: {report_path}")

    def cleanup(self):
        if self.process:
            self.log("Terminating kernel process...")
            self.process.terminate()
            try:
                self.process.wait(timeout=10)
            except:
                self.process.kill()

if __name__ == "__main__":
    harness = TylluanE2EHarness()
    harness.run_all()
