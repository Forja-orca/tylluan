"""Unit tests for TEB-Pilot-50 orchestrator and real agent evaluation."""
import unittest
import sqlite3
import tempfile
import json
import os
import sys
from pathlib import Path

# Add repo root to sys.path
REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT))

from benchmarks.teb.teb_orchestrator import (  # noqa: E402
    query_silva_memory,
    log_real_audit,
    score_task_exact_tokens,
    evaluate_task_c0,
    evaluate_task_c1,
    export_traffic_to_recall_feedback,
    LlmClient,
    run_orchestrator
)


class TestTebOrchestrator(unittest.TestCase):
    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.audit_db = Path(self.temp_dir.name) / "audit.db"
        self.silva_db = Path(self.temp_dir.name) / "silva.db"
        
        # Initialize test audit.db
        conn = sqlite3.connect(self.audit_db)
        c = conn.cursor()
        c.execute("""
            CREATE TABLE guild_audit_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL,
                guild TEXT NOT NULL,
                tool_name TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                intent TEXT,
                status TEXT NOT NULL,
                result_preview TEXT,
                prev_hash TEXT NOT NULL,
                hash TEXT NOT NULL,
                latency_ms INTEGER,
                human_intervention INTEGER DEFAULT 0
            )
        """)
        conn.commit()
        conn.close()
        
        # Initialize test silva.db with nodes and FTS
        conn = sqlite3.connect(self.silva_db)
        c = conn.cursor()
        c.execute("""
            CREATE TABLE nodes (
                id TEXT PRIMARY KEY,
                type TEXT NOT NULL,
                content TEXT NOT NULL,
                metadata TEXT DEFAULT '{}',
                weight REAL DEFAULT 1.0,
                conflicted INTEGER DEFAULT 0
            )
        """)
        c.execute("""
            CREATE VIRTUAL TABLE nodes_fts USING fts5(
                id UNINDEXED,
                content,
                metadata UNINDEXED
            )
        """)
        c.execute("""
            CREATE TABLE recall_feedback (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                memory_id TEXT,
                agent_id TEXT,
                task_hash TEXT,
                query_text TEXT,
                rank_position INTEGER,
                useful INTEGER,
                accessed_at TEXT,
                resolved_at TEXT,
                signal_kind TEXT
            )
        """)
        
        # Insert test knowledge nodes
        nodes = [
            ("node_1", "config_rule", "vector_dimensions must be 1024; reducing to 768 breaks all embeddings in SilvaDB.", "{}", 1.0, 0),
            ("node_2", "graph_rule", "Uses degree penalty pr_score / (1 + deg * 0.1) to penalize generic hubs.", "{}", 1.0, 0),
            ("node_3", "cutover_rule", "Requires >=5000 resolved rows in recall_feedback table for Phase 3 ADR-011.", "{}", 1.0, 0),
        ]
        for n in nodes:
            c.execute("INSERT INTO nodes (id, type, content, metadata, weight, conflicted) VALUES (?, ?, ?, ?, ?, ?)", n)
            c.execute("INSERT INTO nodes_fts (id, content, metadata) VALUES (?, ?, ?)", (n[0], n[2], n[3]))
            
        conn.commit()
        conn.close()

    def tearDown(self):
        self.temp_dir.cleanup()

    def test_query_silva_memory_schema_fix(self):
        """Test query_silva_memory uses n.type and finds matching nodes in FTS."""
        results = query_silva_memory("vector dimensions mandatory 1024", limit=5, db_path=self.silva_db)
        self.assertGreater(len(results), 0)
        self.assertEqual(results[0]["id"], "node_1")
        self.assertEqual(results[0]["type"], "config_rule")
        self.assertIn("1024", results[0]["content"])

    def test_log_real_audit_chain(self):
        """Test log_real_audit inserts valid SHA-256 chained rows."""
        id1 = log_real_audit("kernel", "tylluan_recall", "test_agent", "intent1", "ok", "result1", 42.5, db_path=self.audit_db)
        id2 = log_real_audit("kernel", "tylluan_recall", "test_agent", "intent2", "ok", "result2", 55.0, db_path=self.audit_db)
        
        self.assertIsNotNone(id1)
        self.assertIsNotNone(id2)
        
        conn = sqlite3.connect(self.audit_db)
        rows = conn.cursor().execute("SELECT id, prev_hash, hash, latency_ms FROM guild_audit_log ORDER BY id ASC").fetchall()
        conn.close()
        
        self.assertEqual(len(rows), 2)
        self.assertEqual(rows[0][1], "0000000000000000000000000000000000000000000000000000000000000000")
        self.assertEqual(rows[1][1], rows[0][2])  # prev_hash matches previous entry's hash
        self.assertEqual(rows[0][3], 42)
        self.assertEqual(rows[1][3], 55)

    def test_score_task_exact_tokens(self):
        """Test scoring function with keyword matching."""
        task = {
            "keywords": ["1024", "768", "breaks", "dimensions"],
            "verifier_mode": "exact_token_match"
        }
        
        # Complete match
        passed, score = score_task_exact_tokens("The vector_dimensions must be 1024; reducing to 768 breaks all embeddings.", task)
        self.assertTrue(passed)
        self.assertEqual(score, 1.0)
        
        # Partial match >= 0.75 (3 out of 4)
        passed, score = score_task_exact_tokens("Setting dimensions to 768 breaks everything.", task)
        self.assertTrue(passed)
        self.assertEqual(score, 0.75)
        
        # Insufficient match (1 out of 4)
        passed, score = score_task_exact_tokens("General software architecture uses modular dimensions.", task)
        self.assertFalse(passed)
        self.assertEqual(score, 0.25)

    def test_no_ground_truth_leakage(self):
        """Verify C0 and C1 prompts do NOT contain ground truth or keywords."""
        task = {
            "id": "mem_01",
            "family": "long_term_memory",
            "title": "BGE-M3 Vector Dimensions Invariant",
            "prompt": "What is the mandatory vector embedding dimension for SilvaDB in Tylluan and what happens if reduced to 768?",
            "ground_truth": "SECRET_GROUND_TRUTH_1024_MUST_NOT_LEAK",
            "keywords": ["1024", "768", "breaks", "dimensions"]
        }
        
        prompts_recorded = []
        
        class RecordingLlmClient(LlmClient):
            def generate(self, system_prompt, user_prompt, temperature=0.1, max_tokens=512):
                prompts_recorded.append((system_prompt, user_prompt))
                return "Mock response", 10.0, {"prompt_tokens": 20, "completion_tokens": 10}, None

        client = RecordingLlmClient(provider_type="mock")
        
        evaluate_task_c0(task, llm_client=client)
        evaluate_task_c1(task, llm_client=client, silva_db_path=self.silva_db, audit_db_path=self.audit_db)
        
        for sys_p, usr_p in prompts_recorded:
            self.assertNotIn("SECRET_GROUND_TRUTH_1024_MUST_NOT_LEAK", sys_p)
            self.assertNotIn("SECRET_GROUND_TRUTH_1024_MUST_NOT_LEAK", usr_p)

    def test_export_traffic_to_recall_feedback(self):
        """Test feedback loop export to recall_feedback in silva.db."""
        task_results = [
            {"task_id": "mem_01", "title": "Task 1", "c1": {"passed": True}},
            {"task_id": "mem_02", "title": "Task 2", "c1": {"passed": False}},
        ]
        
        inserted = export_traffic_to_recall_feedback(task_results, db_path=self.silva_db)
        self.assertEqual(inserted, 2)
        
        conn = sqlite3.connect(self.silva_db)
        rows = conn.cursor().execute("SELECT memory_id, useful, signal_kind FROM recall_feedback ORDER BY id ASC").fetchall()
        conn.close()
        
        self.assertEqual(len(rows), 2)
        self.assertEqual(rows[0][0], "teb_pilot:mem_01")
        self.assertEqual(rows[0][1], 1)
        self.assertEqual(rows[1][0], "teb_pilot:mem_02")
        self.assertEqual(rows[1][1], 0)


if __name__ == "__main__":
    unittest.main()
