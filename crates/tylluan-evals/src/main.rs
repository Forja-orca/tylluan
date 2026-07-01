mod corpus;
mod longmemeval;
mod metrics;
mod runner;
mod beam_scale;

#[cfg(test)]
mod tests;

use std::path::Path;

enum Suite {
    Synthetic,
    Real { db_path: String },
    AutoLink { db_path: String },
    LongMemEval { limit: usize },
    LongMemEvalReranked { limit: usize },
    BeamScale { questions: usize, use_embedding: bool },
}

enum CliMode {
    Suite(Suite),
    GenerateOracle { db_path: String, output: String },
}

fn parse_args() -> CliMode {
    let args: Vec<String> = std::env::args().collect();
    let mut suite = "synthetic".to_string();
    let mut limit: usize = 50;
    let mut db_path = "data/silva.db".to_string();
    let mut use_reranker = false;
    let mut generate_oracle = false;
    let mut oracle_output = "data/idle_lab_oracle.json".to_string();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--suite" | "-s" => {
                if i + 1 < args.len() {
                    suite = args[i + 1].clone();
                    i += 1;
                }
            }
            "--limit" | "-l" => {
                if i + 1 < args.len() {
                    limit = args[i + 1].parse().unwrap_or(50);
                    i += 1;
                }
            }
            "--db" => {
                if i + 1 < args.len() {
                    db_path = args[i + 1].clone();
                    i += 1;
                }
            }
            "--reranker" | "-r" => {
                use_reranker = true;
            }
            "--generate-oracle" => {
                generate_oracle = true;
            }
            "--oracle-output" => {
                if i + 1 < args.len() {
                    oracle_output = args[i + 1].clone();
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }

    if generate_oracle {
        return CliMode::GenerateOracle { db_path, output: oracle_output };
    }

    let suite = match suite.as_str() {
        "real" | "r" => Suite::Real { db_path },
        "autolink" | "al" => Suite::AutoLink { db_path },
        "longmemeval" | "lme" => {
            if use_reranker {
                Suite::LongMemEvalReranked { limit }
            } else {
                Suite::LongMemEval { limit }
            }
        }
        "beam" => Suite::BeamScale { questions: limit.min(10), use_embedding: false },
        "beam-emb" => Suite::BeamScale { questions: limit.min(3), use_embedding: true },
        _ => Suite::Synthetic,
    };
    CliMode::Suite(suite)
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    println!();
    println!("  ╔═══════════════════════════════════════════╗");
    println!("  ║      Tylluan-Evals — Honest SilvaDB Bench   ║");
    println!("  ║         BrainBench v2 — Real SilvaDB      ║");
    println!("  ╚═══════════════════════════════════════════╝");
    println!();

    let mode = parse_args();

    match &mode {
        CliMode::GenerateOracle { db_path, output } => {
            println!("  Mode: GENERATE IDLELAB ORACLE");
            println!();
            runner::generate_oracle(db_path, std::path::Path::new(output)).await;
            return;
        }
        _ => {}
    }

    println!("  Loading embedding engine (fastembed BGE-M3)...");
    let engine = match tylluan_kernel::router::embeddings::EmbeddingEngine::load("models/bge-m3") {
        Ok(e) => {
            println!("  Engine loaded (ONNX hybrid search active)");
            Some(e)
        }
        Err(e) => {
            println!("  Embedding engine unavailable: {:?}", e);
            println!("  Falling back to FTS5-only search");
            None
        }
    };

    let suite = match mode {
        CliMode::Suite(s) => s,
        _ => unreachable!(),
    };

    match suite {
        Suite::Synthetic => {
            println!("  Suite: SYNTHETIC CORPUS (25 nodes, 10 queries)");
            println!();
            let corpus = corpus::build_synthetic_corpus();
            let report = runner::run_synthetic_benchmark(&corpus, engine.as_ref()).await;
            metrics::print_report(&report);
            metrics::print_comparison(&report);
        }
        Suite::Real { db_path } => {
            println!("  Suite: REAL SilvaDB — {}", db_path);
            println!("  (read-only via WAL — kernel can stay running)");
            println!();
            let report = runner::run_real_benchmark(&db_path, engine.as_ref()).await;
            metrics::print_report(&report);
            metrics::print_comparison(&report);
        }
        Suite::AutoLink { db_path } => {
            println!("  Suite: AUTOLINK CERO-LLM — {}", db_path);
            println!("  (connects orphan nodes, file refs, topic linking)");
            println!();
            runner::run_auto_link(&db_path).await;
        }
        Suite::LongMemEval { limit } => {
            let data_path = Path::new("data/longmemeval_s_subset.json");
            if !data_path.exists() {
                eprintln!("ERROR: LongMemEval dataset not found at {:?}", data_path);
                std::process::exit(1);
            }
            println!("  Suite: LONGMEMEVAL-S ({} questions from {:?})", limit, data_path);
            println!();
            let bench = longmemeval::LongMemEvalBench::load(data_path)
                .expect("Failed to load LongMemEval dataset");
            let report = bench.run_limit(engine.as_ref(), limit).await;
            metrics::print_report(&report);
            metrics::print_comparison(&report);
        }
        Suite::LongMemEvalReranked { limit } => {
            let data_path = Path::new("data/longmemeval_s_subset.json");
            if !data_path.exists() {
                eprintln!("ERROR: LongMemEval dataset not found at {:?}", data_path);
                std::process::exit(1);
            }
            println!("  Loading Jina Reranker...");
            let reranker = match tylluan_kernel::router::embeddings::RerankEngine::load() {
                Ok(r) => {
                    println!("  Jina Reranker loaded (cross-encoder second pass)");
                    r
                }
                Err(e) => {
                    eprintln!("  ERROR: Reranker load failed: {:?}", e);
                    std::process::exit(1);
                }
            };
            println!("  Suite: LONGMEMEVAL-S ({} questions, Jina Reranker) from {:?}", limit, data_path);
            println!();
            let bench = longmemeval::LongMemEvalBench::load(data_path)
                .expect("Failed to load LongMemEval dataset");
            let report = bench.run_limit_reranked(engine.as_ref(), &reranker, limit).await;
            metrics::print_report(&report);
            metrics::print_comparison(&report);
        }
        Suite::BeamScale { questions, use_embedding } => {
            let data_path = Path::new("data/longmemeval_s_subset.json");
            if !data_path.exists() {
                eprintln!("ERROR: LongMemEval dataset not found at {:?}", data_path);
                eprintln!("       BEAM scale test requires the LongMemEval-S dataset.");
                std::process::exit(1);
            }
            println!("  Suite: BEAM SCALE STRESS TEST");
            println!("  Tiers: 100K / 500K / 1M tokens  |  {} questions per tier", questions);
            if use_embedding {
                println!("  Mode: BGE-M3 embeddings (slow — ~10-30 min per tier on CPU)");
            } else {
                println!("  Mode: BM25 FTS5 only (fast — ~1-3 min per tier)");
                println!("  Tip: use --suite beam-emb for BGE-M3 mode (more accurate, much slower)");
            }
            println!();
            let config = beam_scale::BeamScaleConfig {
                questions_per_tier: questions,
                use_embedding,
                ..Default::default()
            };
            let results = beam_scale::run_beam_scale(data_path, engine.as_ref(), &config).await;
            beam_scale::print_beam_report(&results);
        }
    }
}
