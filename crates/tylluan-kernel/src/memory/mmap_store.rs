use anyhow::{anyhow, Result};
use memmap2::Mmap;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;

pub struct MmapEmbeddingStore {
    mmap: Mmap,
    n_vectors: u32,
    dim: u32,
    scales: Vec<f32>,
    node_to_idx: HashMap<String, u32>,
    idx_to_node: Vec<String>,
    centroids: Vec<Vec<f32>>,
    #[allow(dead_code)]
    nlist: u32,
    assignments: Vec<u32>,
}

impl MmapEmbeddingStore {
    /// Creates a new memory-mapped embedding store file with quantized int8 vectors.
    pub fn create(
        path: &Path,
        node_ids: &[String],
        vectors: &[Vec<f32>],
        dim: usize,
        nlist: u32,
        centroids: &[Vec<f32>],
        assignments: &[u32],
    ) -> Result<Self> {
        if vectors.is_empty() {
            return Err(anyhow!("Cannot create empty embedding store"));
        }
        if node_ids.len() != vectors.len() {
            return Err(anyhow!("Mismatch between node IDs and vectors count"));
        }
        if assignments.len() != vectors.len() {
            return Err(anyhow!("Mismatch between assignments and vectors count"));
        }

        let n_vectors = vectors.len() as u32;
        let dim = dim as u32;

        // 1. Calibrate scales per dimension
        let scales = calibrate_scales(vectors, dim as usize);

        // 2. Quantize vectors
        let mut quantized_data = Vec::with_capacity(n_vectors as usize * dim as usize);
        for v in vectors {
            quantized_data.extend(quantize(v, &scales));
        }

        // 3. Serialize all elements into a file
        let mut file = File::create(path)?;
        
        // Magic
        file.write_all(b"FJV1")?;
        file.write_all(&n_vectors.to_le_bytes())?;
        file.write_all(&dim.to_le_bytes())?;
        file.write_all(&nlist.to_le_bytes())?;

        // Scales
        for &s in &scales {
            file.write_all(&s.to_le_bytes())?;
        }

        // Quantized vectors
        let i8_slice: &[u8] = unsafe {
            std::slice::from_raw_parts(quantized_data.as_ptr() as *const u8, quantized_data.len())
        };
        file.write_all(i8_slice)?;

        // Assignments
        for &a in assignments {
            file.write_all(&a.to_le_bytes())?;
        }

        // Centroids (f32)
        for c in centroids {
            for &val in c {
                file.write_all(&val.to_le_bytes())?;
            }
        }

        // Node ID strings (prefixed with u32 length)
        for id in node_ids {
            let bytes = id.as_bytes();
            let len = bytes.len() as u32;
            file.write_all(&len.to_le_bytes())?;
            file.write_all(bytes)?;
        }

        file.sync_all()?;

        // 4. Load the file back via mmap to return the initialized store
        Self::load(path)
    }

    /// Loads an existing memory-mapped embedding store.
    pub fn load(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let mmap = unsafe { Mmap::map(&file)? };

        if mmap.len() < 16 {
            return Err(anyhow!("Invalid file size (too small)"));
        }

        // Verify magic
        if &mmap[0..4] != b"FJV1" {
            return Err(anyhow!("Invalid file magic header"));
        }

        let n_vectors = u32::from_le_bytes([mmap[4], mmap[5], mmap[6], mmap[7]]);
        let dim = u32::from_le_bytes([mmap[8], mmap[9], mmap[10], mmap[11]]);
        let nlist = u32::from_le_bytes([mmap[12], mmap[13], mmap[14], mmap[15]]);

        let mut offset = 16;

        // Scales
        let mut scales = Vec::with_capacity(dim as usize);
        for _ in 0..dim {
            let val = f32::from_le_bytes([
                mmap[offset],
                mmap[offset + 1],
                mmap[offset + 2],
                mmap[offset + 3],
            ]);
            scales.push(val);
            offset += 4;
        }

        // Quantized vectors (just track the range/pointer in mmap offset)
        let vec_bytes_len = n_vectors as usize * dim as usize;
        offset += vec_bytes_len;

        // Assignments
        let mut assignments = Vec::with_capacity(n_vectors as usize);
        for _ in 0..n_vectors {
            let a = u32::from_le_bytes([
                mmap[offset],
                mmap[offset + 1],
                mmap[offset + 2],
                mmap[offset + 3],
            ]);
            assignments.push(a);
            offset += 4;
        }

        // Centroids
        let mut centroids = Vec::with_capacity(nlist as usize);
        for _ in 0..nlist {
            let mut c = Vec::with_capacity(dim as usize);
            for _ in 0..dim {
                let val = f32::from_le_bytes([
                    mmap[offset],
                    mmap[offset + 1],
                    mmap[offset + 2],
                    mmap[offset + 3],
                ]);
                c.push(val);
                offset += 4;
            }
            centroids.push(c);
        }

        // Node IDs
        let mut node_to_idx = HashMap::new();
        let mut idx_to_node = Vec::with_capacity(n_vectors as usize);
        for idx in 0..n_vectors {
            if offset + 4 > mmap.len() {
                return Err(anyhow!("Corrupt file: out of bounds while reading node IDs"));
            }
            let len = u32::from_le_bytes([
                mmap[offset],
                mmap[offset + 1],
                mmap[offset + 2],
                mmap[offset + 3],
            ]) as usize;
            offset += 4;

            if offset + len > mmap.len() {
                return Err(anyhow!("Corrupt file: node ID length out of bounds"));
            }
            let id = std::str::from_utf8(&mmap[offset..offset + len])?.to_string();
            offset += len;

            node_to_idx.insert(id.clone(), idx);
            idx_to_node.push(id);
        }

        Ok(Self {
            mmap,
            n_vectors,
            dim,
            scales,
            node_to_idx,
            idx_to_node,
            centroids,
            nlist,
            assignments,
        })
    }

    /// Fetches a vector by index and dequantizes it on the fly back to f32.
    pub fn get_vector(&self, idx: u32) -> Vec<f32> {
        if idx >= self.n_vectors {
            return vec![];
        }

        let vec_offset_start = 16 + (self.dim as usize * 4);
        let start = vec_offset_start + (idx as usize * self.dim as usize);
        let end = start + self.dim as usize;
        let raw_i8 = &self.mmap[start..end];

        let mut v = Vec::with_capacity(self.dim as usize);
        for (d, &val) in raw_i8.iter().enumerate() {
            let scale = self.scales.get(d).copied().unwrap_or(1.0);
            v.push((val as i8) as f32 * scale);
        }
        v
    }

    /// Cosine similarity between an f32 query and the stored int8 vector
    /// `idx` WITHOUT materializing the dequantized vector: single fused
    /// pass computes dot + candidate norm together; the query norm is
    /// precomputed by the caller once per query. Frente 2 P2 (Buffy's
    /// measurement, 2026-10-04): 1.73x on the IVF path with identical
    /// R@10 ranking — the no-norm variant was discarded for ranking churn.
    pub fn cosine_query(&self, idx: u32, query: &[f32], query_norm: f32) -> Option<f32> {
        if idx >= self.n_vectors || query.len() != self.dim as usize || self.scales.len() < self.dim as usize {
            return None;
        }
        let vec_offset_start = 16 + (self.dim as usize * 4);
        let start = vec_offset_start + (idx as usize * self.dim as usize);
        let end = start + self.dim as usize;
        if end > self.mmap.len() {
            return None;
        }
        let raw_i8 = &self.mmap[start..end];

        let mut dot = 0.0f32;
        let mut norm_sq = 0.0f32;
        for ((&raw, &q), &scale) in raw_i8.iter().zip(query).zip(&self.scales) {
            let v = (raw as i8) as f32 * scale;
            dot += v * q;
            norm_sq += v * v;
        }
        if norm_sq == 0.0 || query_norm == 0.0 {
            return Some(0.0);
        }
        Some(dot / (query_norm * norm_sq.sqrt()))
    }

    pub fn node_to_index(&self, id: &str) -> Option<u32> {
        self.node_to_idx.get(id).copied()
    }

    pub fn index_to_node(&self, idx: u32) -> Option<&str> {
        self.idx_to_node.get(idx as usize).map(|s| s.as_str())
    }

    pub fn centroids(&self) -> &[Vec<f32>] {
        &self.centroids
    }

    pub fn assignments(&self) -> &[u32] {
        &self.assignments
    }

    pub fn dim(&self) -> u32 {
        self.dim
    }

    pub fn n_vectors(&self) -> u32 {
        self.n_vectors
    }
}

/// Dynamic range scale calibration per dimension.
pub fn calibrate_scales(vectors: &[Vec<f32>], dim: usize) -> Vec<f32> {
    let mut scales = vec![0.0f32; dim];
    for (d, scale) in scales.iter_mut().enumerate() {
        let mut max_val = 0.0f32;
        for v in vectors {
            if let Some(&val) = v.get(d) {
                let abs_val = val.abs();
                if abs_val > max_val {
                    max_val = abs_val;
                }
            }
        }
        // Scale for int8 ranges [-127, 127]
        *scale = if max_val > 0.0 { max_val / 127.0 } else { 1.0 };
    }
    scales
}

/// Quantizes a single f32 vector to i8 based on the scale parameters.
pub fn quantize(v: &[f32], scales: &[f32]) -> Vec<i8> {
    let mut quantized = Vec::with_capacity(v.len());
    for (d, &val) in v.iter().enumerate() {
        let scale = scales.get(d).copied().unwrap_or(1.0);
        let q = (val / scale).round().clamp(-127.0, 127.0) as i8;
        quantized.push(q);
    }
    quantized
}

/// Dequantizes a single i8 vector back to f32.
pub fn dequantize(v: &[i8], scales: &[f32]) -> Vec<f32> {
    let mut dequantized = Vec::with_capacity(v.len());
    for (d, &val) in v.iter().enumerate() {
        let scale = scales.get(d).copied().unwrap_or(1.0);
        dequantized.push(val as f32 * scale);
    }
    dequantized
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_store_path(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("tylluan_mmap_test_{}_{}.fjv1", std::process::id(), name));
        p
    }

    /// Frente 2 P2 (fix IVF): `cosine_query` (fused, sin alloc) debe dar
    /// el MISMO ranking que la referencia (get_vector + cosine_similarity)
    /// — el invariante que la medicion de Buffy exige (0/400 mismatches).
    #[test]
    fn cosine_query_matches_dequantized_reference() {
        let dim = 16;
        let mut vectors: Vec<Vec<f32>> = Vec::new();
        let mut ids: Vec<String> = Vec::new();
        for i in 0..40usize {
            let mut v = Vec::with_capacity(dim);
            for d in 0..dim {
                v.push(((i * 31 + d * 7) as f32 % 50.0) / 50.0 - 0.5);
            }
            let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm > 0.0 {
                for x in &mut v {
                    *x /= norm;
                }
            }
            vectors.push(v);
            ids.push(format!("node-{i}"));
        }
        let centroids = vec![
            vectors[0].clone(),
            vectors[10].clone(),
            vectors[20].clone(),
            vectors[30].clone(),
        ];
        let assignments: Vec<u32> = (0..40u32).map(|i| i % 4).collect();
        let path = temp_store_path("parity");
        let store =
            MmapEmbeddingStore::create(&path, &ids, &vectors, dim, 4, &centroids, &assignments)
                .expect("create store");

        let queries: Vec<Vec<f32>> = vec![
            vectors[0].clone(),
            vectors[17].clone(),
            vectors[39].clone(),
            {
                let q: Vec<f32> = (0..dim)
                    .map(|d| ((d * 13 + 5) as f32 % 29.0) / 29.0 - 0.5)
                    .collect();
                let n: f32 = q.iter().map(|x| x * x).sum::<f32>().sqrt();
                q.iter().map(|x| x / n).collect()
            },
        ];

        for q in &queries {
            let qn = (q.iter().map(|v| v * v).sum::<f32>()).sqrt();
            let mut ref_scored: Vec<(u32, f32)> = Vec::new();
            let mut fast_scored: Vec<(u32, f32)> = Vec::new();
            for idx in 0..store.n_vectors() {
                let v = store.get_vector(idx);
                let sim_ref = crate::memory::cosine::cosine_similarity(q, &v);
                ref_scored.push((idx, sim_ref));
                let sim_fast = store
                    .cosine_query(idx, q, qn)
                    .expect("cosine_query must return a score");
                fast_scored.push((idx, sim_fast));
                assert!(
                    (sim_ref - sim_fast).abs() < 1e-4,
                    "idx {idx}: ref {sim_ref} vs fast {sim_fast}"
                );
            }
            ref_scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            fast_scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            let ref_top: Vec<u32> = ref_scored.iter().take(10).map(|(i, _)| *i).collect();
            let fast_top: Vec<u32> = fast_scored.iter().take(10).map(|(i, _)| *i).collect();
            assert_eq!(ref_top, fast_top, "top-10 ranking must be identical");
        }

        let _ = std::fs::remove_file(&path);
    }
}
