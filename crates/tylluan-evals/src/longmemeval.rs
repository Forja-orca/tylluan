use anyhow::{Result, Context};
use serde::Deserialize;
use std::path::Path;
use std::time::Instant;
use tracing::{info, warn};

use tylluan_kernel::memory::silva::SilvaDB;
use tylluan_kernel::router::embeddings::EmbeddingEngine;
use tylluan_kernel::router::embeddings::RerankEngine;

use crate::metrics;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct LongMemEvalQuestion {
    pub question_id: String,
    pub question_type: String,
    pub question: String,
    pub question_date: String,
    pub answer: serde_json::Value,
    pub answer_session_ids: Vec<String>,
    pub haystack_dates: Vec<String>,
    pub haystack_session_ids: Vec<String>,
    pub haystack_sessions: Vec<Vec<HistoryTurn>>,
}

#[derive(Debug, Deserialize)]
pub struct HistoryTurn {
    pub role: String,
    pub content: String,
}

pub struct LongMemEvalBench {
    questions: Vec<LongMemEvalQuestion>,
}

impl LongMemEvalBench {
    pub fn load(path: &Path) -> Result<Self> {
        let data = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read LongMemEval file: {:?}", path))?;
        let questions: Vec<LongMemEvalQuestion> = serde_json::from_str(&data)
            .with_context(|| "Failed to parse LongMemEval JSON")?;
        info!("Loaded {} LongMemEval questions", questions.len());
        Ok(Self { questions })
    }

    pub fn len(&self) -> usize {
        self.questions.len()
    }

    pub fn questions(&self) -> &[LongMemEvalQuestion] {
        &self.questions
    }

    #[allow(dead_code)]
    pub async fn run(&self, engine: Option<&EmbeddingEngine>) -> metrics::BenchmarkReport {
        self.run_limit(engine, self.questions.len()).await
    }

    #[allow(dead_code)]
    pub async fn run_reranked(&self, engine: Option<&EmbeddingEngine>, reranker: &RerankEngine) -> metrics::BenchmarkReport {
        self.run_limit_reranked(engine, reranker, self.questions.len()).await
    }

    pub async fn run_limit(&self, engine: Option<&EmbeddingEngine>, limit: usize) -> metrics::BenchmarkReport {
        let num_questions = self.questions.len().min(limit);
        println!("  Running {} questions (haystack embed per question)...\n", num_questions);
        let mut results = Vec::with_capacity(num_questions);

        for (i, q) in self.questions.iter().take(num_questions).enumerate() {
            print!("  [{:02}/{}] {} ... ", i + 1, num_questions,
                &q.question_id[..q.question_id.len().min(12)]);
            let result = self.evaluate_question(q, engine).await;
            let icon = if result.correct_in_top5 { "+" } else { "x" };
            println!("[{}]  {:.0}ms", icon, result.latency_ms);
            results.push(result);
        }

        metrics::compute_report(
            &format!("LongMemEval-S ({} questions)", num_questions),
            0, 0, results,
            engine.is_some(), false, None,
        )
    }

    pub async fn run_limit_reranked(&self, engine: Option<&EmbeddingEngine>, reranker: &RerankEngine, limit: usize) -> metrics::BenchmarkReport {
        let num_questions = self.questions.len().min(limit);
        println!("  Running {} questions with Jina Reranker (haystack embed per question)...\n", num_questions);
        let mut results = Vec::with_capacity(num_questions);

        for (i, q) in self.questions.iter().take(num_questions).enumerate() {
            print!("  [{:02}/{}] {} ... ", i + 1, num_questions,
                &q.question_id[..q.question_id.len().min(12)]);
            let result = self.evaluate_question_reranked(q, engine, reranker).await;
            let icon = if result.correct_in_top5 { "+" } else { "x" };
            println!("[{}]  {:.0}ms", icon, result.latency_ms);
            results.push(result);
        }

        metrics::compute_report(
            &format!("LongMemEval-S ({} questions, Jina Reranker)", num_questions),
            0, 0, results,
            engine.is_some(), true, None,
        )
    }

    async fn evaluate_question_reranked(
        &self,
        q: &LongMemEvalQuestion,
        engine: Option<&EmbeddingEngine>,
        reranker: &RerankEngine,
    ) -> metrics::QueryResult {
        let db = SilvaDB::in_memory().await
            .expect("Failed to create in-memory SilvaDB");

        let _answer_text = q.answer.as_str()
            .or_else(|| q.answer.as_array().and_then(|a| a.first().and_then(|v| v.as_str())))
            .unwrap_or("");

        let mut answer_node_ids: Vec<String> = Vec::new();
        for (session_idx, session) in q.haystack_sessions.iter().enumerate() {
            let session_id = q.haystack_session_ids.get(session_idx)
                .map(|s| s.as_str())
                .unwrap_or("unknown");
            let is_answer_session = q.answer_session_ids.iter()
                .any(|aid| aid == session_id);

            let qid_prefix = &q.question_id[..q.question_id.len().min(8)];
            let metadata_base = serde_json::json!({
                "session_id": session_id,
                "session_index": session_idx,
                "is_answer_session": is_answer_session,
            }).to_string();

            // Index each user turn as a separate node to avoid semantic dilution.
            // Long sessions (10+ turns) bury relevant facts in unrelated context,
            // causing the session-level embedding to miss the query.
            for (turn_idx, turn) in session.iter().enumerate() {
                if turn.role != "user" {
                    continue;
                }
                let chunk_id = format!("lme:{}:s{}:t{}", qid_prefix, session_idx, turn_idx);
                // Include 1 preceding assistant turn for context
                let context = if turn_idx > 0 {
                    let prev = &session[turn_idx - 1];
                    format!("{}: {}\nuser: {}", prev.role, prev.content, turn.content)
                } else {
                    format!("user: {}", turn.content)
                };

                if let Err(e) = db.upsert_node(&chunk_id, "lme_context", &context, &metadata_base).await {
                    warn!("Failed to upsert chunk {}: {:?}", chunk_id, e);
                    continue;
                }
                if let Some(ref engine) = engine {
                    if let Ok(emb) = engine.embed(&context) {
                        let _ = db.save_embedding(&chunk_id, &emb, "bge-m3", None).await;
                    }
                }
                if is_answer_session {
                    answer_node_ids.push(chunk_id);
                }
            }
        }

        if answer_node_ids.is_empty() {
            return metrics::compute_query_result(&[], &[]);
        }

        let query_embedding = engine.and_then(|e| e.embed(&q.question).ok());
        let start = Instant::now();
        let retrieved = db.search_hybrid_reranked(&q.question, query_embedding.as_deref(), 10, reranker)
            .await
            .unwrap_or_default();
        let elapsed = start.elapsed();

        let paired: Vec<(String, f32)> = retrieved.iter()
            .map(|(node, score)| (node.id.clone(), *score))
            .collect();

        let mut qr = metrics::compute_query_result(&paired, &answer_node_ids);
        qr.latency_ms = elapsed.as_secs_f64() * 1000.0;
        qr
    }

    async fn evaluate_question(
        &self,
        q: &LongMemEvalQuestion,
        engine: Option<&EmbeddingEngine>,
    ) -> metrics::QueryResult {
        let db = SilvaDB::in_memory().await
            .expect("Failed to create in-memory SilvaDB");

        let _answer_text = q.answer.as_str()
            .or_else(|| q.answer.as_array().and_then(|a| a.first().and_then(|v| v.as_str())))
            .unwrap_or("");

        let mut answer_node_ids: Vec<String> = Vec::new();
        for (session_idx, session) in q.haystack_sessions.iter().enumerate() {
            let session_id = q.haystack_session_ids.get(session_idx)
                .map(|s| s.as_str())
                .unwrap_or("unknown");
            let is_answer_session = q.answer_session_ids.iter()
                .any(|aid| aid == session_id);

            let session_content: String = session.iter()
                .map(|turn| format!("{}: {}", turn.role, turn.content))
                .collect::<Vec<_>>()
                .join("\n");

            let node_id = format!("lme:{}:session:{}",
                &q.question_id[..q.question_id.len().min(8)], session_idx);
            let metadata = serde_json::json!({
                "session_id": session_id,
                "session_index": session_idx,
                "is_answer_session": is_answer_session,
            }).to_string();

            if let Err(e) = db.upsert_node(&node_id, "lme_context", &session_content, &metadata).await {
                warn!("Failed to upsert node {}: {:?}", node_id, e);
                continue;
            }

            if let Some(ref engine) = engine {
                if let Ok(emb) = engine.embed(&session_content) {
                    let _ = db.save_embedding(&node_id, &emb, "bge-m3", None).await;
                }
            }

            if is_answer_session {
                answer_node_ids.push(node_id);
            }
        }

        if answer_node_ids.is_empty() {
            return metrics::compute_query_result(&[], &[]);
        }

        let query_embedding = engine.and_then(|e| e.embed(&q.question).ok());
        let start = Instant::now();
        let retrieved = db.search_hybrid(&q.question, query_embedding.as_deref(), 10)
            .await
            .unwrap_or_default();
        let elapsed = start.elapsed();

        let paired: Vec<(String, f32)> = retrieved.iter()
            .map(|(node, score)| (node.id.clone(), *score))
            .collect();

        let mut qr = metrics::compute_query_result(&paired, &answer_node_ids);
        qr.latency_ms = elapsed.as_secs_f64() * 1000.0;
        qr
    }
}
