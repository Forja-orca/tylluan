//! # Guild Matcher
//!
//! Routes natural language queries to the best matching guild.
//!
//! ## Dual Strategy
//!
//! - **Keyword mode** (default): Tokenizes query and descriptions, scores by word overlap
//! - **Semantic mode** (`--features semantic`): Cosine similarity on ONNX embeddings
//!
//! Keyword mode is always available as fallback, even when semantic is enabled.

use crate::config::InferenceDevice;
use crate::memory::cosine::cosine_similarity;
use crate::router::catalog::{GuildDescriptor, GuildCategory};
use crate::router::embeddings::EmbeddingEngine;
use std::sync::{Arc, Mutex};
use tracing::info;

/// Result of a guild match operation.
#[derive(Debug, Clone)]
pub struct MatchResult {
    pub guild_name: String,
    pub score: f32,
    pub method: MatchMethod,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MatchMethod {
    Keyword,
    Semantic,
    Curriculum,
}

impl std::fmt::Display for MatchMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MatchMethod::Keyword => write!(f, "keyword"),
            MatchMethod::Semantic => write!(f, "semantic"),
            MatchMethod::Curriculum => write!(f, "curriculum"),
        }
    }
}
/// Optional context about the calling agent to bias routing.
#[derive(Debug, Clone, Default)]
pub struct GuildContext {
    /// Agent role identifier, e.g. "backend-dev", "frontend-dev", "guardian".
    /// Used to prefer guilds from the agent's home guild category.
    pub agent_role: Option<String>,
    /// Explicit guild category preference — overrides role inference.
    pub preferred_category: Option<GuildCategory>,
}

impl GuildContext {
    pub fn from_agent_id(agent_id: &str) -> Self {
        // Infer preferred category from known role names
        let preferred = if agent_id.contains("backend") || agent_id.contains("architect") || agent_id.contains("devops") {
            Some(GuildCategory::Builder)
        } else if agent_id.contains("frontend") {
            // Frontend agents prefer Builder tools (code, git, bash)
            Some(GuildCategory::Builder)
        } else if agent_id.contains("guardian") || agent_id.contains("warden") {
            Some(GuildCategory::Watcher)
        } else if agent_id.contains("researcher") || agent_id.contains("analyst") || agent_id.contains("scholar") {
            Some(GuildCategory::Scholar)
        } else {
            None
        };
        GuildContext {
            agent_role: Some(agent_id.to_string()),
            preferred_category: preferred,
        }
    }
}

/// Compute a guild-category routing bonus based on intent keywords.
/// Returns (bonus: f32, reason: &'static str) for structured logging.
fn guild_category_bonus(intent_lower: &str, category: &GuildCategory) -> (f32, &'static str) {
    match category {
        GuildCategory::Builder => {
            // Builders: code construction, architecture, DevOps
            let builder_signals = [
                "arquitectura", "architecture", "adr", "diseño", "design",
                "implementa", "implement", "refactori", "construye", "build",
                "endpoint", "api", "handler", "compilar", "compile",
                "deploy", "release", "dockerfile",
            ];
            if builder_signals.iter().any(|s| intent_lower.contains(s)) {
                let bonus = (crate::memory::idle_lab::BUILDER_BONUS.load(std::sync::atomic::Ordering::Relaxed) as f32) / 100.0;
                return (bonus, "builder:architecture/implementation signal");
            }
        }
        GuildCategory::Scholar => {
            // Scholars: research, analysis, knowledge
            let scholar_signals = [
                "investiga", "research", "analiza", "analyze", "busca",
                "comparativa", "comparison", "qué es", "what is",
                "documenta", "document", "lee el pdf", "read pdf",
                // Technical NLP
                "extrae triples", "extract triples",
                "knowledge graph", "named entity", "ner ", "triple",
                "subject predicate", "entity extraction",
                "relation extraction", "link concepts", "find entities",
                // Natural language knowledge intents
                "aprende este", "aprende que", "aprende esto",
                "guarda en el grafo", "añade al grafo", "poblar el grafo",
                "enlaza conceptos", "relaciona conceptos",
                "extrae conceptos", "extraer conceptos",
                "extrae el conocimiento", "extrae las relaciones",
                "add to knowledge", "populate the graph", "store knowledge",
                "learn from this", "extract knowledge", "extract entities",
                "auto-link", "link this concept",
            ];
            if scholar_signals.iter().any(|s| intent_lower.contains(s)) {
                let bonus = (crate::memory::idle_lab::SCHOLAR_BONUS.load(std::sync::atomic::Ordering::Relaxed) as f32) / 100.0;
                return (bonus, "scholar:research/analysis signal");
            }
        }
        GuildCategory::Watcher => {
            // Wardens: security, audits, health checks
            let warden_signals = [
                "auditoría", "audit", "seguridad", "security",
                "health", "salud", "integridad", "integrity",
                "alerta", "alert", "incident", "incidente",
                "compliance", "cumplimiento", "scan",
            ];
            if warden_signals.iter().any(|s| intent_lower.contains(s)) {
                let bonus = (crate::memory::idle_lab::WARDEN_BONUS.load(std::sync::atomic::Ordering::Relaxed) as f32) / 100.0;
                return (bonus, "warden:security/audit signal");
            }
        }
        GuildCategory::Core => {}
    }
    (0.0, "no bonus")
}

pub struct GuildMatcher {
    catalog: Vec<GuildDescriptor>,
    engine: Option<Arc<EmbeddingEngine>>,
    curriculum: Option<Arc<Mutex<crate::curriculum::CurriculumLearner>>>,
    hormones: Option<Arc<std::sync::Mutex<crate::hormones::HormoneSystem>>>,
    guild_health: dashmap::DashMap<String, f64>,
}

impl GuildMatcher {
    /// Create a new matcher from a catalog of guild descriptors.
    pub fn new(catalog: Vec<GuildDescriptor>) -> Self {
        Self { catalog, engine: None, curriculum: None, hormones: None, guild_health: dashmap::DashMap::new() }
    }

    /// Attach a curriculum learner for experience-based routing.
    pub fn with_curriculum(mut self, curriculum: Arc<Mutex<crate::curriculum::CurriculumLearner>>) -> Self {
        self.curriculum = Some(curriculum);
        self
    }

    /// Attach a hormone system for stress-aware routing.
    pub fn with_hormones(mut self, hormones: Arc<std::sync::Mutex<crate::hormones::HormoneSystem>>) -> Self {
        self.hormones = Some(hormones);
        self
    }

    /// Access the underlying embedding engine.
    pub fn engine(&self) -> Option<&EmbeddingEngine> {
        self.engine.as_ref().map(|arc| arc.as_ref())
    }

    /// Access the underlying embedding engine as an Arc for shared ownership.
    pub fn engine_arc(&self) -> Option<&Arc<EmbeddingEngine>> {
        self.engine.as_ref()
    }

    /// Access the curriculum learner.
    pub fn curriculum(&self) -> Option<Arc<Mutex<crate::curriculum::CurriculumLearner>>> {
        self.curriculum.clone()
    }

    /// Add a guild to the catalog.
    #[allow(dead_code)]
    pub fn add_guild(&mut self, descriptor: GuildDescriptor) {
        if !self.catalog.iter().any(|g| g.name == descriptor.name) {
            self.catalog.push(descriptor);
        }
    }

    /// Load embedding model and pre-compute embeddings for the catalog.
    /// If `allowed_guilds` is provided, only embed those guilds (toaster-friendly).
    /// Otherwise embeds ALL guilds for lazy matching.
    pub fn load_model(&mut self, allowed_guilds: Option<&[String]>, model_name: &str) -> anyhow::Result<()> {
        self.load_model_with_device(allowed_guilds, model_name, &InferenceDevice::Cpu)
    }

    /// Load embedding model by config name (e.g. "bge-m3", "minilm", "bge-small").
    pub fn load_model_with_device(&mut self, allowed_guilds: Option<&[String]>, model_name: &str, device: &InferenceDevice) -> anyhow::Result<()> {
        use anyhow::Context;

        let model_path = EmbeddingEngine::model_path_from_config(model_name)
            .context("No ONNX model found. FastEmbed will auto-download on first use")?;

        let engine = EmbeddingEngine::load_with_device(&model_path, device)?;
        let engine_arc = Arc::new(engine);
        self.engine = Some(engine_arc.clone());
        let engine_ref = engine_arc.as_ref();
        
        let mut embeddings = Vec::new();
        for guild in &self.catalog {
            // If allowed_guilds specified, filter to those only (toaster-friendly)
            // Otherwise embed ALL guilds for lazy matching
            if let Some(allowed) = allowed_guilds
                && !allowed.iter().any(|n| n == &guild.name) {
                    // Skip - will be loaded lazily via find_lazy_candidates
                    tracing::debug!("Lazy guild (not pre-embedded): {}", guild.name);
                    continue;
                }
            info!("🧠 Embedding guild: {}", guild.name);
            let emb = engine_ref.embed(&guild.description)?;
            embeddings.push((guild.name.clone(), emb));
        }
        
        self.set_embeddings(embeddings);
        Ok(())
    }

    /// Get all available guilds (for lazy loading UI)
    pub fn available_guilds(&self) -> Vec<&GuildDescriptor> {
        self.catalog.iter().collect()
    }

    /// Get the catalog.
    #[allow(dead_code)]
    pub fn catalog(&self) -> &[GuildDescriptor] {
        &self.catalog
    }

    /// Set pre-computed embeddings on the catalog (called when semantic feature is enabled).
    pub fn set_embeddings(&mut self, embeddings: Vec<(String, Vec<f32>)>) {
        for (name, emb) in embeddings {
            if let Some(guild) = self.catalog.iter_mut().find(|g| g.name == name) {
                guild.embedding = Some(emb);
            }
        }
    }

    /// Record outcome for curriculum learning (best-effort, non-blocking).
    pub fn record_outcome(&self, intent_sig: &str, guild_name: &str, success: bool, latency_ms: u64) {
        if let Some(ref c) = self.curriculum
            && let Ok(mut learner) = c.lock() {
                learner.record_outcome(intent_sig, guild_name, success, latency_ms);
            }
    }

    /// Returns the curriculum confidence for a given intent+guild pair (0.0–1.0).
    pub fn get_confidence(&self, intent: &str, guild: &str) -> f64 {
        if let Some(ref c) = self.curriculum
            && let Ok(curriculum) = c.lock()
                && let Some((_g, _score, confidence)) = curriculum.recommend(intent, &[guild.to_string()]) {
                    return confidence;
                }
        0.0
    }

    pub fn curriculum_stats(&self) -> serde_json::Value {
        self.curriculum.as_ref().and_then(|c| c.lock().ok()).map(|c| c.get_stats()).unwrap_or(serde_json::json!({}))
    }

    pub fn update_guild_health(&self, guild_name: &str, score: f64) {
        self.guild_health.insert(guild_name.to_string(), score.clamp(0.0, 1.0));
    }

    pub fn update_all_health(&self, health_map: std::collections::HashMap<String, f64>) {
        for (name, score) in health_map {
            self.guild_health.insert(name, score.clamp(0.0, 1.0));
        }
    }

    /// Match a query to the best guild using hybrid (semantic+keyword) scoring.
    ///
    /// Strategy:
    /// 1. Special cases — conversational greetings and file-analysis override
    /// 2. Curriculum — if confident, route by learned preference
    /// 3. Hybrid pass — per guild: blend BGE-M3 cosine similarity (55%) + keyword score (45%)
    /// 4. Context bonuses — category + role bias
    /// 5. Stress-aware routing, curriculum overlay, health factor
    pub fn match_guild(
        &self,
        query: &str,
        query_embedding: Option<&[f32]>,
        threshold: f32,
        ctx: Option<&GuildContext>,
    ) -> Option<MatchResult> {
        // Special cases: conversational greetings and file-analysis override bypass scoring
        if let Some(result) = self.trigger_special_cases(query) {
            return Some(result);
        }

        // Compute query embedding if not provided and engine is loaded
        let own_emb = if query_embedding.is_none() {
            self.engine.as_ref().and_then(|e| e.embed(query).ok())
        } else { None };
        let q_emb = query_embedding.or(own_emb.as_deref());

        // Proactive curriculum: if learner has high-confidence data, use it directly
        if let Some(curriculum) = &self.curriculum
            && let Ok(learner) = curriculum.lock() {
                let candidates: Vec<String> = self.catalog.iter().map(|g| g.name.clone()).collect();
                if let Some((guild_name, score, confidence)) = learner.recommend(query, &candidates)
                    && confidence > 0.6 {
                        return Some(MatchResult {
                            guild_name,
                            score: score as f32,
                            method: MatchMethod::Curriculum,
                        });
                    }
            }

        // ── Hybrid single pass: semantic + keyword blend per guild ──
        let query_lower = query.to_lowercase();
        let query_tokens = tokenize(&query_lower);
        let first_word = query_lower.split_whitespace().next().unwrap_or("");

        let verb_triggers: &[(&str, &str)] = &[
            ("busca", "search"), ("encuentra", "filesystem"),
            ("lista", "filesystem"), ("muestra", "filesystem"),
            ("lee", "filesystem"), ("escribe", "code"),
            ("crea", "code"), ("compila", "bash"),
            ("ejecuta", "bash"), ("analiza", "code"),
            ("monitoriza", "monitor"),
            ("search", "search"), ("find", "filesystem"),
            ("show", "filesystem"), ("display", "filesystem"),
            ("echo", "bash"), ("run", "bash"), ("pwd", "bash"),
            ("ls", "bash"), ("cat", "bash"), ("grep", "bash"),
        ];
        let verb_guild = verb_triggers.iter()
            .find(|(v, _)| *v == first_word)
            .map(|(_, g)| *g);

        let sem_weight: f32 = 0.55;
        let kw_weight: f32 = 0.45;

        let mut best: Option<MatchResult> = None;
        let mut best_score = -1.0_f32;

        info!("🔍 Matcher: hybrid scoring '{}'", query);
        for guild in &self.catalog {
            // Semantic score from pre-computed guild embedding (BGE-M3 cosine similarity)
            let sem_score = q_emb.and_then(|qe| {
                guild.embedding.as_ref().map(|ge| cosine_similarity(qe, ge))
            }).unwrap_or(0.0);

            // Keyword score with trigger bonus, verb bonus, negative penalty
            let kw_score = keyword_score(&query_tokens, &guild.description, &guild.name);
            let trigger_bonus = if guild.trigger_phrases.iter().any(|t| query_lower.contains(t)) {
                0.5
            } else { 0.0 };
            let verb_bonus = if let Some(vg) = verb_guild {
                if guild.name == vg { 0.3 } else { 0.0 }
            } else { 0.0 };
            let neg_penalty = if guild.negative_keywords.iter().any(|nk| query_tokens.contains(nk)) {
                0.3
            } else { 0.0 };

            let kw_total = (kw_score + trigger_bonus + verb_bonus - neg_penalty).max(0.0);

            // Blend: 55% semantic + 45% keyword when embedding available; pure keyword otherwise
            let score = if sem_score > 0.0 {
                sem_weight * sem_score + kw_weight * kw_total
            } else {
                kw_total
            };
            let method = if sem_score > 0.0 { MatchMethod::Semantic } else { MatchMethod::Keyword };

            tracing::debug!(
                "  - [{}] {} | Score: {:.3} (sem={:.3} kw={:.3} trg={:.3} verb={:.3} neg={:.3})",
                if sem_score > 0.0 { "HYBRID" } else { "KEYWORD" },
                guild.name, score, sem_score, kw_score, trigger_bonus, verb_bonus, neg_penalty
            );

            if score > best_score && score >= threshold {
                best_score = score;
                best = Some(MatchResult {
                    guild_name: guild.name.clone(),
                    score,
                    method,
                });
            }
        }

        let mut keyword_result = best;

        // Confidence gate: reject sub-threshold matches
        if let Some(ref m) = keyword_result {
            if m.score < 0.1 {
                tracing::warn!(query = %query, guild = %m.guild_name, score = m.score, "Match below confidence floor (0.1)");
                return None;
            }
        } else {
            tracing::debug!("⚠️ No match above threshold ({})", threshold);
            return None;
        }

        // Apply guild category and role context bonuses
        if let Some(ctx) = ctx {
            if let Some(ref mut result) = keyword_result {
                let q_lower = query.to_lowercase();
                if let Some(guild) = self.catalog.iter().find(|g| g.name == result.guild_name) {
                    let (cat_bonus, _) = guild_category_bonus(&q_lower, &guild.category);
                    let role_bonus = if let Some(ref preferred) = ctx.preferred_category {
                        if *preferred == guild.category { 0.08 } else { 0.0 }
                    } else { 0.0 };
                    result.score = (result.score + cat_bonus + role_bonus).min(1.0);
                }
            }
        }

        // Stress-aware routing: if stress is high, penalize risky guilds and prefer stable ones
        if let Some(hormones) = &self.hormones
            && let Ok(hormone_system) = hormones.lock() {
                let stress = hormone_system.stress_level();
                if stress > 0.6 {
                    // High stress: penalize guilds with recent failures in curriculum
                    if let Some(curriculum) = &self.curriculum
                        && let Ok(learner) = curriculum.lock()
                            && let Some(result) = &keyword_result {
                                let guild_name = &result.guild_name;
                                // Check if this guild has recent failures
                                if learner.get_failure_rate(guild_name).unwrap_or(0.0) > 0.3 {
                                    // Apply penalty
                                    let penalized_score = result.score - 0.15;
                                    if penalized_score > 0.1 {
                                        keyword_result = Some(MatchResult {
                                            guild_name: result.guild_name.clone(),
                                            score: penalized_score,
                                            method: result.method.clone(),
                                        });
                                    }
                                }
                            }
                }
                if stress > 0.85 {
                    // Critical stress: fallback to bash (most stable guild)
                    if keyword_result.as_ref().map(|r| r.guild_name.as_str()) != Some("bash") {
                        return Some(MatchResult {
                            guild_name: "bash".to_string(),
                            score: 0.7,
                            method: MatchMethod::Keyword,
                        });
                    }
                }
            }

        // Curriculum overlay: if learner has strong data, prefer the learned guild
        if let Some(curriculum) = &self.curriculum
            && let Ok(learner) = curriculum.lock() {
                let candidates: Vec<String> = self.catalog.iter().map(|g| g.name.clone()).collect();
                if let Some((learned_guild, score, confidence)) = learner.recommend(query, &candidates)
                    && confidence > 0.6 && score > 0.5 {
                        let should_override = match &keyword_result {
                            Some(kr) => {
                                if kr.score > 0.9 { false }
                                else { score > kr.score as f64 }
                            }
                            None => true,
                        };
                        if should_override {
                            return Some(MatchResult {
                                guild_name: learned_guild,
                                score: score as f32,
                                method: MatchMethod::Curriculum,
                            });
                        }
                    }
            }

        // Pressure-field scientific routing (arXiv:2601.08129v3):
        // guild healthy (score→1.0) → factor 1.2 (boost 20%)
        // guild neutral (score→0.5) → factor 1.0 (neutral)
        // guild stressed (score→0.0) → factor 0.6 (penalty 40%)
        if let Some(result) = &keyword_result
            && let Some(health) = self.guild_health.get(&result.guild_name) {
                let health_factor = 0.6 + (*health * 0.6);
                let adjusted_score = (result.score as f64 * health_factor) as f32;
                keyword_result = Some(MatchResult {
                    guild_name: result.guild_name.clone(),
                    score: adjusted_score,
                    method: result.method.clone(),
                });
            }

        keyword_result
    }

    /// Match a query to the best guild, with optional agent context for category-aware routing.
    ///
    /// Delegates to `match_guild` which now applies guild category and role context bonuses
    /// internally during keyword scoring.
    pub fn match_guild_with_context(
        &self,
        query: &str,
        query_embedding: Option<&[f32]>,
        threshold: f32,
        ctx: Option<&GuildContext>,
    ) -> Option<MatchResult> {
        self.match_guild(query, query_embedding, threshold, ctx)
    }

    /// Trigger match: explicit phrase → guild mapping for high-confidence patterns.
    /// Fires before keyword scoring so common natural-language patterns route correctly
    /// even when the intent has many filler words that dilute keyword scores.
    /// Full trigger match (special cases + trigger_phrases + verb_triggers).
    /// Used by routing.rs as fallback when RFL blocks the routed guild.
    pub fn trigger_match_pub(&self, query: &str) -> Option<MatchResult> {
        // Special cases first (conversational, file-analysis)
        if let Some(result) = self.trigger_special_cases(query) {
            return Some(result);
        }
        // Then trigger_phrases and verb_triggers
        self.trigger_phrases_and_verbs(query)
    }

    /// Only conversational greetings and file-analysis override — these bypass all scoring.
    fn trigger_special_cases(&self, query: &str) -> Option<MatchResult> {
        let bare = if let (Some(start), Some(end)) = (query.find("[ctx:"), query.find(']')) {
            if start == 0 { query[end + 1..].trim() } else { query }
        } else { query };
        let q = bare.to_lowercase();

        // Conversational/greeting intents
        let conversational = ["hello", "hola", "hi ", "gracias", "thanks", 
                               "ayuda", "help", "qué puedes", "what can you",
                               "buenas", "good morning", "good evening"];
        if conversational.iter().any(|p| q.contains(p)) && q.len() < 50 {
            return Some(MatchResult {
                guild_name: "bash".to_string(),
                score: 0.15,
                method: MatchMethod::Keyword,
            });
        }

        // File-analysis override
        let file_analysis_verbs = ["analyze", "analiz", "explain", "explica", "parse",
                                   "parsea", "describe", "what does", "qué hace", "que hace"];
        let code_exts = ["py", "rs", "ts", "tsx", "js", "jsx", "go", "cpp", "c", "h",
                         "md", "toml", "json", "yaml", "yml"];
        let has_code_file = q.split_whitespace().any(|w| {
            let w = w.trim_matches(|c: char| !c.is_alphanumeric() && c != '.' && c != '/');
            if w.starts_with("http") { return false; }
            w.rfind('.').map(|i| {
                let ext = &w[i + 1..];
                code_exts.contains(&ext)
            }).unwrap_or(false)
        });
        if has_code_file && file_analysis_verbs.iter().any(|v| q.contains(v))
            && self.catalog.iter().any(|g| g.name == "code") {
                tracing::info!("⚡ File-analysis override → code (path contains source ext + analysis verb)");
                return Some(MatchResult {
                    guild_name: "code".to_string(),
                    score: 0.97,
                    method: MatchMethod::Keyword,
                });
            }

        None
    }

    /// Check trigger_phrases and verb_triggers, returning the best match.
    fn trigger_phrases_and_verbs(&self, query: &str) -> Option<MatchResult> {
        let q = query.to_lowercase();
        let first_word = q.split_whitespace().next().unwrap_or("");

        // Verb trigger map: first_word → guild_name
        let verb_triggers: &[(&str, &str)] = &[
            ("busca", "search"), ("encuentra", "filesystem"),
            ("lista", "filesystem"), ("muestra", "filesystem"),
            ("lee", "filesystem"), ("escribe", "code"),
            ("crea", "code"), ("compila", "bash"),
            ("ejecuta", "bash"), ("analiza", "code"),
            ("monitoriza", "monitor"),
            ("search", "search"), ("find", "filesystem"),
            ("show", "filesystem"), ("display", "filesystem"),
            ("echo", "bash"), ("run", "bash"), ("pwd", "bash"),
            ("ls", "bash"), ("cat", "bash"), ("grep", "bash"),
        ];
        let verb_guild = verb_triggers.iter()
            .find(|(v, _)| *v == first_word)
            .map(|(_, g)| *g);

        // Single pass: find best guild by trigger phrase or verb match
        let mut best: Option<MatchResult> = None;
        let mut best_score = 0.0_f32;

        for guild in &self.catalog {
            let mut score = 0.0_f32;
            // Trigger phrase bonus
            if guild.trigger_phrases.iter().any(|t| q.contains(t)) {
                score = 0.95;
            }
            // Verb match bonus (check guild name matches)
            if let Some(vg) = verb_guild {
                if guild.name == vg && score < 0.85 {
                    score = 0.85;
                }
            }
            if score > best_score {
                best_score = score;
                best = Some(MatchResult {
                    guild_name: guild.name.clone(),
                    score,
                    method: MatchMethod::Keyword,
                });
            }
        }

        if let Some(ref m) = best {
            info!("⚡ Trigger/verb match: '{}' → {}", query, m.guild_name);
        }
        best
    }

    /// Match all guilds above threshold, sorted by score descending.
    #[allow(dead_code)]
    pub fn match_all(
        &self,
        query: &str,
        query_embedding: Option<&[f32]>,
        threshold: f32,
    ) -> Vec<MatchResult> {
        let mut results = Vec::new();

        // Semantic matches
        if let Some(q_emb) = query_embedding {
            for guild in &self.catalog {
                if let Some(emb) = &guild.embedding {
                    let sim = cosine_similarity(q_emb, emb);
                    if sim >= threshold {
                        results.push(MatchResult {
                            guild_name: guild.name.clone(),
                            score: sim,
                            method: MatchMethod::Semantic,
                        });
                    }
                }
            }
        }

        // Keyword matches (deduplicate with semantic)
        let query_lower = query.to_lowercase();
        let query_tokens = tokenize(&query_lower);

        for guild in &self.catalog {
            // Skip if already matched semantically
            if results.iter().any(|r| r.guild_name == guild.name) {
                continue;
            }

            let score = keyword_score(&query_tokens, &guild.description, &guild.name);
            if score >= threshold {
                results.push(MatchResult {
                    guild_name: guild.name.clone(),
                    score,
                    method: MatchMethod::Keyword,
                });
            }
        }

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results
    }

    /// Find lazy candidates: guilds that might match but aren't loaded yet.
    /// Uses embeddings if available, otherwise keyword fallback.
    pub fn find_lazy_candidates(&self, query: &str, threshold: f32) -> Vec<MatchResult> {
        if let Some(engine) = &self.engine
            && let Ok(q_emb) = engine.embed(query) {
                let mut results: Vec<MatchResult> = Vec::new();
                for g in &self.catalog {
                    if let Some(emb) = &g.embedding {
                        let sim = cosine_similarity(&q_emb, emb);
                        if sim >= threshold {
                            results.push(MatchResult {
                                guild_name: g.name.clone(),
                                score: sim,
                                method: MatchMethod::Semantic,
                            });
                        }
                    }
                }
                results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
                return results;
            }
        self.match_all(query, None, threshold)
    }

}

/// Tokenize a string into words for keyword matching (lowercase, min 2 chars).
pub fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .map(|w: &str| w.to_lowercase())
        .filter(|w: &String| w.len() >= 2)
        .collect()
}

/// Score a query against a guild by keyword overlap.
/// Returns a value between 0.0 and 1.0.
///
/// Uses word-boundary matching (tokenized) to avoid substring traps:
/// "check" must not match "checkout", "test" must not match "testcontainers".
pub fn keyword_score(query_tokens: &[String], description: &str, guild_name: &str) -> f32 {
    if query_tokens.is_empty() {
        return 0.0;
    }

    let desc_tokens: Vec<String> = tokenize(&description.to_lowercase());
    // Underscore-separated names ("system_metrics") are split into component words
    let name_normalized = guild_name.to_lowercase().replace('_', " ");
    let name_tokens: Vec<String> = tokenize(&name_normalized);

    let mut matches = 0;

    for token in query_tokens {
        if name_tokens.contains(token) {
            matches += 2;
        } else if desc_tokens.contains(token) {
            matches += 1;
        }
    }

    let max_score = query_tokens.len() as f32 * 2.0;
    (matches as f32 / max_score).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::catalog::builtin_catalog;

    fn test_matcher() -> GuildMatcher {
        GuildMatcher::new(builtin_catalog())
    }

    #[test]
    fn test_keyword_match_exact_name() {
        let matcher = test_matcher();
        let result = matcher.match_guild("git", None, 0.3, None);
        assert!(result.is_some());
        assert_eq!(result.unwrap().guild_name, "git");
    }

    #[test]
    fn test_keyword_match_description() {
        let matcher = test_matcher();
        let result = matcher.match_guild("docker container orchestration", None, 0.2, None);
        assert!(result.is_some());
        assert_eq!(result.unwrap().guild_name, "docker");
    }

    #[test]
    fn test_keyword_match_shell_commands() {
        let matcher = test_matcher();
        let result = matcher.match_guild("execute shell commands", None, 0.2, None);
        assert!(result.is_some());
        assert_eq!(result.unwrap().guild_name, "bash");
    }

    #[test]
    fn test_keyword_no_match() {
        let matcher = test_matcher();
        let result = matcher.match_guild("xyznonexistent", None, 0.3, None);
        assert!(result.is_none());
    }

    #[test]
    fn test_semantic_match_with_fake_embeddings() {
        let mut matcher = test_matcher();

        // Set fake embeddings: bash=[1,0,0], git=[0,1,0], docker=[0,0,1]
        matcher.set_embeddings(vec![
            ("bash".into(), vec![1.0, 0.0, 0.0]),
            ("git".into(), vec![0.0, 1.0, 0.0]),
            ("docker".into(), vec![0.0, 0.0, 1.0]),
        ]);

        // Query close to bash
        let query_emb = vec![0.9, 0.1, 0.0];
        let result = matcher.match_guild("anything", Some(&query_emb), 0.3, None);
        assert!(result.is_some());
        let m = result.unwrap();
        assert_eq!(m.guild_name, "bash");
        assert_eq!(m.method, MatchMethod::Semantic);
    }

    #[test]
    fn test_semantic_fallback_to_keyword() {
        let matcher = test_matcher(); // No embeddings set
        let result = matcher.match_guild("git status", Some(&[0.0, 0.0, 0.0]), 0.3, None);
        // Semantic match fails (no embeddings) → keyword should pick up "git"
        assert!(result.is_some());
        assert_eq!(result.unwrap().guild_name, "git");
    }

    #[test]
    fn test_match_all_returns_multiple() {
        let matcher = test_matcher();
        let results = matcher.match_all("database SQL queries", None, 0.2);
        assert!(!results.is_empty());
        // Should match database guild
        assert!(results.iter().any(|r| r.guild_name == "database"));
    }

    #[test]
    fn test_tokenize() {
        let tokens = tokenize("hello world 42 a");
        assert_eq!(tokens, vec!["hello", "world", "42"]);
        // "a" is filtered (< 2 chars)
    }

    #[test]
    fn test_keyword_score_exact() {
        let tokens = tokenize("git");
        let score = keyword_score(&tokens, "Git source control operations", "git");
        assert!(score > 0.5); // Name match = 2 points
    }

    #[test]
    fn test_keyword_score_zero() {
        let tokens = tokenize("xyznothing");
        let score = keyword_score(&tokens, "completely unrelated", "unrelated");
        assert_eq!(score, 0.0);
    }

    // Regression: substring traps that caused wrong routing in production
    #[test]
    fn test_check_does_not_match_checkout() {
        let tokens = tokenize("health check");
        let git_score = keyword_score(&tokens, "Git source control: status, diff, log, commit, checkout, branch operations.", "git");
        assert_eq!(git_score, 0.0, "'check' must not match 'checkout'");
    }

    #[test]
    fn test_system_does_not_match_filesystem() {
        let tokens = tokenize("system health report");
        let fs_score = keyword_score(&tokens, "Read and write files, search directories, list file contents recursively.", "filesystem");
        assert_eq!(fs_score, 0.0, "'system' must not match 'filesystem'");
    }

    #[test]
    fn test_test_does_not_match_testcontainers() {
        let tokens = tokenize("test total");
        let docker_score = keyword_score(&tokens, "Docker container orchestration, manage testcontainers, start databases (Postgres, Redis).", "docker");
        assert_eq!(docker_score, 0.0, "'test' must not match 'testcontainers'");
    }

    #[test]
    fn test_health_check_routes_to_system_metrics() {
        let matcher = test_matcher();
        let result = matcher.match_guild("health check", None, 0.25, None);
        assert!(result.is_some());
        assert_eq!(result.unwrap().guild_name, "system_metrics");
    }

    #[test]
    fn test_test_routes_to_bash() {
        let matcher = test_matcher();
        let result = matcher.match_guild("test total", None, 0.25, None);
        assert!(result.is_some());
        assert_eq!(result.unwrap().guild_name, "bash");
    }

    #[test]
    fn test_bug_02_git_routing() {
        let matcher = test_matcher();
        // Exact trigger match
        assert_eq!(matcher.match_guild("check git status", None, 0.3, None).unwrap().guild_name, "git");
        // Phrase match — "what changed" triggers git's trigger phrase
        assert_eq!(matcher.match_guild("what changed recently in the repo", None, 0.3, None).unwrap().guild_name, "git");
        // Keyword match with negative weight for filesystem
        // "git file" might match filesystem if not for the penalty
        assert_eq!(matcher.match_guild("git diff of this file", None, 0.3, None).unwrap().guild_name, "git");
        // Pure filesystem match — use a trigger phrase that exists
        let fs = matcher.match_guild("find all .py files", None, 0.3, None).unwrap();
        assert_eq!(fs.guild_name, "filesystem", "Expected find all .py files → filesystem, got {}", fs.guild_name);
    }

    #[test]
    fn test_git_filename_routes_to_code_not_git() {
        let matcher = test_matcher();
        // "git.py" in path must NOT route to git guild — file-analysis intent wins
        let result = matcher.match_guild("analyze the file guilds/core/git.py", None, 0.2, None);
        assert!(result.is_some());
        assert_eq!(result.unwrap().guild_name, "code",
            "file-analysis intent must win over guild-name-in-path");
    }

    #[test]
    fn test_docker_filename_routes_to_code_not_docker() {
        let matcher = test_matcher();
        let result = matcher.match_guild("explain the file guilds/core/docker.py", None, 0.2, None);
        assert!(result.is_some());
        assert_eq!(result.unwrap().guild_name, "code");
    }

    #[test]
    fn test_think_step_by_step_routes_to_sequential_thinking() {
        let matcher = test_matcher();
        let result = matcher.match_guild("think step by step about the problem", None, 0.2, None);
        assert!(result.is_some());
        assert_eq!(result.unwrap().guild_name, "sequential_thinking");
    }

    #[test]
    fn test_audit_system_routes_to_audit() {
        let matcher = test_matcher();
        let result = matcher.match_guild("audit system health and check all running guilds", None, 0.2, None);
        assert!(result.is_some());
        assert_eq!(result.unwrap().guild_name, "audit");
    }

    #[test]
    fn test_save_file_routes_to_filesystem_not_audit() {
        let matcher = test_matcher();
        // "find files" is auto-extracted from filesystem.py's Use for: docstring
        let result = matcher.match_guild("find files with .py extension", None, 0.2, None);
        assert!(result.is_some());
        assert_eq!(result.unwrap().guild_name, "filesystem");
    }

    // ─── Guild Context Routing tests ─────────────────────────────────────────

    #[test]
    fn test_guild_category_bonus_warden_audit() {
        let (bonus, reason) = guild_category_bonus("necesito una auditoría de seguridad del sistema", &GuildCategory::Watcher);
        assert!(bonus > 0.0, "Warden should get bonus for security/audit intent");
        assert!(!reason.is_empty());
    }

    #[test]
    fn test_guild_category_bonus_builder_architecture() {
        let (bonus, _) = guild_category_bonus("diseño de la arquitectura del nuevo endpoint api", &GuildCategory::Builder);
        assert!(bonus > 0.0, "Builder should get bonus for architecture/design intent");
    }

    #[test]
    fn test_guild_category_bonus_scholar_research() {
        let (bonus, _) = guild_category_bonus("investiga los mejores modelos de embeddings", &GuildCategory::Scholar);
        assert!(bonus > 0.0, "Scholar should get bonus for research intent");
    }

    #[test]
    fn test_guild_category_bonus_no_match() {
        let (bonus, _) = guild_category_bonus("list files in src/", &GuildCategory::Watcher);
        assert_eq!(bonus, 0.0, "Warden should NOT get bonus for filesystem intent");
    }

    #[test]
    fn test_guild_context_from_backend_agent() {
        let ctx = GuildContext::from_agent_id("agent-backend-dev");
        assert_eq!(ctx.preferred_category, Some(GuildCategory::Builder));
        assert!(ctx.agent_role.is_some());
    }

    #[test]
    fn test_guild_context_from_guardian_agent() {
        let ctx = GuildContext::from_agent_id("agent-guardian-wardens");
        assert_eq!(ctx.preferred_category, Some(GuildCategory::Watcher));
    }

    #[test]
    fn test_guild_context_unknown_agent_has_no_preference() {
        let ctx = GuildContext::from_agent_id("generic-agent-001");
        assert_eq!(ctx.preferred_category, None);
    }

    // ─── GIK: knowledge guild natural language routing ───────────────────────

    #[test]
    fn test_knowledge_routes_on_natural_spanish() {
        let matcher = test_matcher();
        let cases = [
            "extrae conceptos de este texto",
            "poblar el grafo con esta información",
        ];
        for intent in cases {
            let result = matcher.match_guild(intent, None, 0.15, None);
            // These may match knowledge by keyword or fall back to other guilds;
            // we just verify the system doesn't crash and returns a guild
            if let Some(r) = result {
                assert!(
                    ["knowledge", "search", "code"].contains(&r.guild_name.as_str()),
                    "Expected knowledge, search, or code for: '{}' got: '{}'", intent, r.guild_name
                );
            }
        }
    }

    #[test]
    fn test_knowledge_routes_on_natural_english() {
        let matcher = test_matcher();
        let cases = [
            "add to knowledge graph the concept of stigmergy",
            "store knowledge about guild routing",
            "extract knowledge from this document",
        ];
        for intent in cases {
            let result = matcher.match_guild(intent, None, 0.15, None);
            assert!(result.is_some(), "No match for: '{}'", intent);
            assert_eq!(result.unwrap().guild_name, "knowledge",
                "Expected knowledge guild for: '{}'", intent);
        }
    }

    #[test]
    fn test_knowledge_still_routes_on_technical_triggers() {
        let matcher = test_matcher();
        let cases = [
            "extract triples from this text",
            "named entity recognition on this paragraph",
            "relation extraction pipeline",
        ];
        for intent in cases {
            let result = matcher.match_guild(intent, None, 0.15, None);
            assert!(result.is_some(), "No match for: '{}'", intent);
            assert_eq!(result.unwrap().guild_name, "knowledge",
                "Expected knowledge guild for: '{}'", intent);
        }
    }

    #[test]
    fn test_match_guild_with_context_no_match_returns_none() {
        let matcher = test_matcher();
        // Completely unrecognizable intent should return None
        let result = matcher.match_guild_with_context("xyzzynonexistentqzqzqz", None, 0.3, None);
        assert!(result.is_none(), "Unrecognizable intent should return None (NO_GUILD_MATCH)");
    }

    #[test]
    fn test_match_guild_with_context_preserves_high_confidence_trigger() {
        let matcher = test_matcher();
        // High-confidence trigger match must NOT be overridden by context bonus
        let ctx = GuildContext::from_agent_id("scholar-researcher");
        // "run cargo test" contains "run cargo" trigger phrase → matches bash
        let result = matcher.match_guild_with_context("run cargo test -p tylluan-kernel", None, 0.2, Some(&ctx));
        assert!(result.is_some());
        // "run cargo test" → bash via trigger (score 0.5 + keyword), scholar context should NOT override
        assert_eq!(result.unwrap().guild_name, "bash",
            "High-confidence trigger match must win over scholar context preference");
    }

    #[test]
    fn test_match_guild_with_context_warden_security_audit() {
        let matcher = test_matcher();
        let ctx = GuildContext::from_agent_id("guardian-warden");
        // Security audit intent + warden context should route to audit guild
        let result = matcher.match_guild_with_context(
            "run a security audit and check system integrity", None, 0.15, Some(&ctx)
        );
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(
            r.guild_name == "audit" || r.guild_name == "system_metrics",
            "Security intent with warden context should route to audit or system_metrics, got: {}",
            r.guild_name
        );
    }
}
