//! Integration test for the knowledge guild.
//! Requires: .venv and guilds/core/knowledge.py

#[cfg(feature = "guild_knowledge_test")]
mod knowledge_guild_tests {
    use std::process::{Command, Stdio};
    use std::time::Duration;

    #[test]
    #[ignore = "requires .venv and guilds/core/knowledge.py"]
    fn knowledge_guild_handshake_response() {
        let venv_python = if cfg!(windows) {
            "E:/TylluanMCPo3/.venv/Scripts/python.exe"
        } else {
            "E:/TylluanMCPo3/.venv/bin/python"
        };

        let init_json = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "test",
                    "version": "1.0"
                }
            }
        });

        let mut child = Command::new(venv_python)
            .args(&["-m", "guilds.core.knowledge"])
            .current_dir("E:/TylluanMCPo3")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to spawn knowledge guild process");

        let stdin = child.stdin.as_mut().expect("Failed to open stdin");
        stdin
            .write_all(format!("{}\n", init_json).as_bytes())
            .expect("Failed to write init JSON");

        std::thread::sleep(Duration::from_secs(3));

        let output = child.kill().ok().and_then(|_| child.wait().ok()).map(|s| s.success()).unwrap_or(false);
        assert!(output, "Knowledge guild did not respond to handshake within 3s");
    }

    #[test]
    #[ignore = "requires .venv and guilds/core/knowledge.py"]
    fn knowledge_guild_extract_triples_valid_json() {
        let venv_python = if cfg!(windows) {
            "E:/TylluanMCPo3/.venv/Scripts/python.exe"
        } else {
            "E:/TylluanMCPo3/.venv/bin/python"
        };

        let init_json = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "test",
                    "version": "1.0"
                }
            }
        });

        let call_json = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "extract_triples",
                "arguments": {
                    "text": "The kernel uses SQLite for persistent memory storage with SilvaDB as the knowledge graph layer. Each node has weight, type, and metadata fields.",
                    "max_triples": 5
                }
            }
        });

        let mut child = Command::new(venv_python)
            .args(&["-m", "guilds.core.knowledge"])
            .current_dir("E:/TylluanMCPo3")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to spawn knowledge guild process");

        let stdin = child.stdin.as_mut().expect("Failed to open stdin");
        stdin.write_all(format!("{}\n", init_json).as_bytes()).expect("Failed to write init");
        stdin.write_all(format!("{}\n", call_json).as_bytes()).expect("Failed to write call");

        std::thread::sleep(Duration::from_secs(3));

        let output = child.wait_with_output().expect("Failed to read output");
        let stdout = String::from_utf8_lossy(&output.stdout);

        assert!(stdout.contains("triples"), "extract_triples response should contain 'triples' field: {}", stdout);
    }
}
