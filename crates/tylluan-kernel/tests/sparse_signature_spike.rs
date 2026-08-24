//! Spike (T289, "memoria como firma"): does BGE-M3 sparse give a cheap,
//! comparable "signature" for near-duplicate memory content, the way an
//! immune system recognizes an epitope without re-reading the whole
//! pathogen? Isolated from SilvaDB/production on purpose -- this only
//! answers the measurement question. GO/NO-GO based on real numbers, not
//! assumed.
//!
//! Downloads a separate ONNX model (SparseTextEmbedding::BGEM3, distinct
//! from the dense BGE-M3 already used by EmbeddingEngine) on first run --
//! authorized explicitly by José before running this.
//!
//! Run: cargo test --test sparse_signature_spike -- --ignored --nocapture
//! (--ignored because it downloads a model; not part of the normal suite)

use fastembed::{SparseTextEmbedding, SparseInitOptions, SparseModel};

/// Jaccard overlap over the nonzero dimension indices of two sparse vectors
/// -- the cheap "does this look like the same signature" comparison a real
/// integration would use before ever touching full dense/content compare.
fn sparse_overlap(a: &fastembed::SparseEmbedding, b: &fastembed::SparseEmbedding) -> f64 {
    use std::collections::HashSet;
    let ia: HashSet<_> = a.indices.iter().collect();
    let ib: HashSet<_> = b.indices.iter().collect();
    let inter = ia.intersection(&ib).count();
    let union = ia.union(&ib).count();
    if union == 0 { 0.0 } else { inter as f64 / union as f64 }
}

#[test]
#[ignore]
fn spike_sparse_signature_overlap_for_near_duplicate_memories() {
    let mut model = SparseTextEmbedding::try_new(
        SparseInitOptions::new(SparseModel::BGEM3).with_show_download_progress(true),
    )
    .expect("SparseTextEmbedding::BGEM3 init failed -- if this fails, the spike is NO-GO on library grounds, not measurement grounds");

    // Three real-shaped cases, not synthetic noise:
    // 1) near-duplicate memory content (same fact, different phrasing) --
    //    the case the immune-signature idea is supposed to catch cheaply.
    // 2) unrelated memory content -- must NOT look like a signature match.
    // 3) exact duplicate -- sanity floor, must be the highest overlap.
    let near_dup_a = "El kernel usa BGE-M3 para embeddings densos de 1024 dimensiones.";
    let near_dup_b = "BGE-M3 genera embeddings densos de 1024d, usado por el kernel.";
    let unrelated = "El gremio bash ejecuta comandos de shell bajo demanda via stdio.";
    let exact_dup = near_dup_a;

    let docs = vec![near_dup_a, near_dup_b, unrelated, exact_dup];
    let embeddings = model.embed(docs, None).expect("sparse embed failed");

    let overlap_near_dup = sparse_overlap(&embeddings[0], &embeddings[1]);
    let overlap_unrelated = sparse_overlap(&embeddings[0], &embeddings[2]);
    let overlap_exact = sparse_overlap(&embeddings[0], &embeddings[3]);

    println!("overlap(near-dup)  = {overlap_near_dup:.4}");
    println!("overlap(unrelated) = {overlap_unrelated:.4}");
    println!("overlap(exact-dup) = {overlap_exact:.4}");
    println!("nnz[0]={} nnz[1]={} nnz[2]={}", embeddings[0].indices.len(), embeddings[1].indices.len(), embeddings[2].indices.len());

    // GO signal: near-duplicate content must score meaningfully higher than
    // unrelated content, and exact duplicate must be the ceiling (1.0).
    assert!(
        overlap_exact > 0.99,
        "exact duplicate should have ~perfect overlap, got {overlap_exact}"
    );
    assert!(
        overlap_near_dup > overlap_unrelated,
        "near-duplicate overlap ({overlap_near_dup}) should exceed unrelated overlap ({overlap_unrelated}) for this to be GO"
    );
}
