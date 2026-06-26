use rand::Rng;

#[derive(Clone, Debug)]
pub struct IVFOptions {
    pub nlist: u32,  // number of centroids (default: sqrt(n_vectors))
    pub nprobe: u32, // cells to search (default: sqrt(nlist))
    pub top_k: usize,
}

impl Default for IVFOptions {
    fn default() -> Self {
        Self {
            nlist: 100,
            nprobe: 10,
            top_k: 10,
        }
    }
}

pub struct IVFSearcher {
    centroids: Vec<Vec<f32>>,       // centroids kept in RAM
    inverted_lists: Vec<Vec<u32>>,  // per-centroid: list of vector indices (idx in store)
    #[allow(dead_code)]
    nprobe: u32,
}

impl IVFSearcher {
    pub fn new(centroids: Vec<Vec<f32>>, assignments: &[u32], nprobe: u32) -> Self {
        let nlist = centroids.len();
        let mut inverted_lists = vec![Vec::new(); nlist];
        for (vec_idx, &centroid_idx) in assignments.iter().enumerate() {
            if (centroid_idx as usize) < nlist {
                inverted_lists[centroid_idx as usize].push(vec_idx as u32);
            }
        }
        Self {
            centroids,
            inverted_lists,
            nprobe,
        }
    }

    /// Finds the `nprobe` nearest centroid indices for a query vector.
    pub fn find_nearest_centroids(&self, query: &[f32], nprobe: usize) -> Vec<usize> {
        let mut scored: Vec<(usize, f32)> = self.centroids.iter()
            .enumerate()
            .map(|(idx, centroid)| {
                let dist = l2_distance(query, centroid);
                (idx, dist)
            })
            .collect();

        // Sort by L2 distance ascending (closest first)
        scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.into_iter().take(nprobe).map(|(idx, _)| idx).collect()
    }

    pub fn inverted_lists(&self) -> &[Vec<u32>] {
        &self.inverted_lists
    }
}

/// Compute Euclidean (L2) distance squared.
pub fn l2_distance(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len().min(b.len());
    let a = &a[..len];
    let b = &b[..len];
    let mut dist = 0.0f32;
    for i in 0..len {
        let diff = a[i] - b[i];
        dist += diff * diff;
    }
    dist
}

/// Assigns a set of vectors to their closest centroids based on L2 distance.
pub fn assign_to_centroids(vectors: &[Vec<f32>], centroids: &[Vec<f32>]) -> Vec<u32> {
    let mut assignments = Vec::with_capacity(vectors.len());
    for v in vectors {
        let mut best_idx = 0;
        let mut min_dist = f32::MAX;
        for (c_idx, c) in centroids.iter().enumerate() {
            let dist = l2_distance(v, c);
            if dist < min_dist {
                min_dist = dist;
                best_idx = c_idx;
            }
        }
        assignments.push(best_idx as u32);
    }
    assignments
}

/// Simple, robust K-Means++ clustering algorithm implementation in pure Rust.
pub fn kmeans_plus_plus(vectors: &[Vec<f32>], nlist: u32, max_iterations: usize) -> (Vec<Vec<f32>>, Vec<u32>) {
    if vectors.is_empty() || nlist == 0 {
        return (vec![], vec![]);
    }
    let nlist = nlist.min(vectors.len() as u32) as usize;
    let dim = vectors[0].len();
    let mut rng = rand::thread_rng();

    // 1. Initialize centroids using k-means++ strategy
    let mut centroids: Vec<Vec<f32>> = Vec::with_capacity(nlist);
    
    // First centroid is chosen uniformly at random
    let first_idx = rng.gen_range(0..vectors.len());
    centroids.push(vectors[first_idx].clone());

    // Remaining centroids
    for _ in 1..nlist {
        let mut distances = vec![f32::MAX; vectors.len()];
        let mut sum_dist_sq = 0.0f64;
        
        for (i, v) in vectors.iter().enumerate() {
            // Find distance to the closest centroid already chosen
            let mut min_dist = f32::MAX;
            for c in &centroids {
                let dist = l2_distance(v, c);
                if dist < min_dist {
                    min_dist = dist;
                }
            }
            distances[i] = min_dist;
            sum_dist_sq += (min_dist * min_dist) as f64;
        }

        // Weighted random selection based on squared distance
        let mut threshold = rng.gen_range(0.0..sum_dist_sq);
        let mut selected_idx = vectors.len() - 1;
        for (i, &dist) in distances.iter().enumerate() {
            let dist_sq = (dist * dist) as f64;
            if threshold <= dist_sq {
                selected_idx = i;
                break;
            }
            threshold -= dist_sq;
        }
        centroids.push(vectors[selected_idx].clone());
    }

    // 2. Main K-Means Lloyd iteration loop
    let mut assignments = vec![0; vectors.len()];
    for _iter in 0..max_iterations {
        // Step A: Assignment
        assignments = assign_to_centroids(vectors, &centroids);

        // Step B: Update Centroids
        let mut new_centroids = vec![vec![0.0f32; dim]; nlist];
        let mut counts = vec![0usize; nlist];

        for (v_idx, &c_idx) in assignments.iter().enumerate() {
            let c_idx = c_idx as usize;
            if c_idx < nlist {
                counts[c_idx] += 1;
                for d in 0..dim {
                    new_centroids[c_idx][d] += vectors[v_idx][d];
                }
            }
        }

        let mut shifted = false;
        for c_idx in 0..nlist {
            let count = counts[c_idx];
            if count > 0 {
                for d in 0..dim {
                    new_centroids[c_idx][d] /= count as f32;
                }
                // Check shift/stabilization threshold
                if l2_distance(&centroids[c_idx], &new_centroids[c_idx]) > 1e-4 {
                    shifted = true;
                }
            } else {
                // If a centroid has no assignments, re-initialize it to a random vector
                new_centroids[c_idx] = vectors[rng.gen_range(0..vectors.len())].clone();
                shifted = true;
            }
        }

        centroids = new_centroids;
        if !shifted {
            break; // stable configuration reached
        }
    }

    let final_assignments = assign_to_centroids(vectors, &centroids);
    (centroids, final_assignments)
}
