//! # Louvain Community Detection
//!
//! Implementation of the Louvain method for community detection in large graphs.
//! Optimizes modularity by iteratively moving nodes between communities.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct WeightedGraph {
    pub nodes: Vec<String>,
    pub node_to_idx: HashMap<String, usize>,
    pub adj: Vec<Vec<(usize, f64)>>,
    pub total_edge_weight: f64,
}

impl WeightedGraph {
    pub fn new(nodes: Vec<String>) -> Self {
        let node_count = nodes.len();
        let mut node_to_idx = HashMap::with_capacity(node_count);
        for (i, id) in nodes.iter().enumerate() {
            node_to_idx.insert(id.clone(), i);
        }

        Self {
            nodes,
            node_to_idx,
            adj: vec![Vec::new(); node_count],
            total_edge_weight: 0.0,
        }
    }

    pub fn add_edge(&mut self, s: &str, t: &str, weight: f64) {
        if let (Some(&si), Some(&ti)) = (self.node_to_idx.get(s), self.node_to_idx.get(t)) {
            self.adj[si].push((ti, weight));
            self.adj[ti].push((si, weight));
            self.total_edge_weight += weight;
        }
    }

    /// Run the Leiden community detection algorithm (optimized Louvain with community refinement).
    pub fn find_communities(&self) -> HashMap<String, usize> {
        let mut current_graph = self.clone();
        let partition: HashMap<String, usize> = self.nodes.iter().enumerate().map(|(i, id)| (id.clone(), i)).collect();
        let mut final_partition = partition;

        let mut phase = 0;

        while phase < 5 {
            phase += 1;

            let (new_partition, improved) = current_graph.optimize_modularity();
            if !improved {
                break;
            }

            // Refine partition to split disconnected communities into separate sub-communities (Leiden style)
            let refined_partition = current_graph.refine_partition(&new_partition);

            // Update final partition by mapping current nodes to the new communities
            let mut updated_final = HashMap::new();
            for (node_id, current_comm) in &final_partition {
                let node_name = if phase == 1 {
                    current_graph.nodes[*current_comm].clone()
                } else {
                    format!("comm_{}", current_comm)
                };
                if let Some(&new_comm) = refined_partition.get(&node_name) {
                    updated_final.insert(node_id.clone(), new_comm);
                }
            }
            final_partition = updated_final;

            // Aggregate graph for the next phase based on the refined partition
            current_graph = current_graph.aggregate(&refined_partition);
        }

        final_partition
    }

    /// Refines the partition to split disconnected communities into separate sub-communities.
    fn refine_partition(&self, partition: &HashMap<String, usize>) -> HashMap<String, usize> {
        let n = self.nodes.len();
        let mut refined = HashMap::new();
        
        let mut comm_groups: HashMap<usize, Vec<usize>> = HashMap::new();
        for i in 0..n {
            let &comm = partition.get(&self.nodes[i]).unwrap_or(&i);
            comm_groups.entry(comm).or_default().push(i);
        }

        let mut next_comm_id = 0;

        for (_comm, group) in comm_groups {
            let mut sub_partition = HashMap::new();
            for &node_idx in &group {
                sub_partition.insert(node_idx, node_idx);
            }

            for &i in &group {
                let mut best_target = i;
                let mut max_conn = 0.0;
                for &(neighbor, weight) in &self.adj[i] {
                    if group.contains(&neighbor)
                        && weight > max_conn {
                            max_conn = weight;
                            best_target = neighbor;
                        }
                }
                
                if best_target != i {
                    let root_i = find_root(&sub_partition, i);
                    let root_target = find_root(&sub_partition, best_target);
                    if root_i != root_target {
                        sub_partition.insert(root_i, root_target);
                    }
                }
            }

            let mut root_to_new_comm = HashMap::new();
            for &node_idx in &group {
                let root = find_root(&sub_partition, node_idx);
                let comm_id = *root_to_new_comm.entry(root).or_insert_with(|| {
                    let id = next_comm_id;
                    next_comm_id += 1;
                    id
                });
                refined.insert(self.nodes[node_idx].clone(), comm_id);
            }
        }

        refined
    }

    /// Phase 1: Iterative modularity optimization by moving nodes.
    fn optimize_modularity(&self) -> (HashMap<String, usize>, bool) {
        let n = self.nodes.len();
        if n == 0 { return (HashMap::new(), false); }

        let mut partition: Vec<usize> = (0..n).collect();
        let mut k_i = vec![0.0; n];
        for i in 0..n {
            for (_, w) in &self.adj[i] {
                k_i[i] += w;
            }
        }

        let m = self.total_edge_weight;
        if m == 0.0 {
            let mut res = HashMap::new();
            for i in 0..n { res.insert(self.nodes[i].clone(), i); }
            return (res, false);
        }

        let mut sigma_tot = k_i.clone();
        let mut improved_any = false;
        let mut improved_phase = true;
        
        while improved_phase {
            improved_phase = false;
            for i in 0..n {
                let current_comm = partition[i];
                let mut neighbor_comms = HashMap::new();
                for &(neighbor, weight) in &self.adj[i] {
                    let comm = partition[neighbor];
                    *neighbor_comms.entry(comm).or_insert(0.0) += weight;
                }

                sigma_tot[current_comm] -= k_i[i];
                
                let mut best_comm = current_comm;
                let mut max_gain = 0.0;

                for (&comm, &ki_in) in &neighbor_comms {
                    let gain = (ki_in / m) - (sigma_tot[comm] * k_i[i] / (2.0 * m * m));
                    if gain > max_gain {
                        max_gain = gain;
                        best_comm = comm;
                    }
                }

                partition[i] = best_comm;
                sigma_tot[best_comm] += k_i[i];

                if best_comm != current_comm {
                    improved_any = true;
                    improved_phase = true;
                }
            }
        }

        let mut res = HashMap::new();
        for i in 0..n {
            res.insert(self.nodes[i].clone(), partition[i]);
        }
        (res, improved_any)
    }

    /// Phase 2: Community aggregation into a coarser graph.
    fn aggregate(&self, partition: &HashMap<String, usize>) -> Self {
        let mut community_nodes = HashMap::new();
        let mut new_nodes = Vec::new();

        for &comm_id in partition.values() {
            if let std::collections::hash_map::Entry::Vacant(e) = community_nodes.entry(comm_id) {
                let new_node_id = format!("comm_{}", comm_id);
                e.insert(new_nodes.len());
                new_nodes.push(new_node_id);
            }
        }

        let mut new_graph = Self::new(new_nodes);

        for i in 0..self.nodes.len() {
            let comm_i = partition[&self.nodes[i]];
            let new_i = community_nodes[&comm_i];

            for &(neighbor, weight) in &self.adj[i] {
                let comm_j = partition[&self.nodes[neighbor]];
                let new_j = community_nodes[&comm_j];
                
                // Add edge in aggregate graph
                new_graph.add_edge_by_idx(new_i, new_j, weight);
            }
        }

        new_graph
    }

    fn add_edge_by_idx(&mut self, si: usize, ti: usize, weight: f64) {
        // Search if edge already exists to update weight (aggregation merges edges)
        if let Some(edge) = self.adj[si].iter_mut().find(|(idx, _)| *idx == ti) {
            edge.1 += weight;
        } else {
            self.adj[si].push((ti, weight));
        }
        
        if si != ti {
            if let Some(edge) = self.adj[ti].iter_mut().find(|(idx, _)| *idx == si) {
                edge.1 += weight;
            } else {
                self.adj[ti].push((si, weight));
            }
        }
        
        self.total_edge_weight += weight;
    }
}

fn find_root(parent: &HashMap<usize, usize>, mut node: usize) -> usize {
    while let Some(&p) = parent.get(&node) {
        if p == node {
            break;
        }
        node = p;
    }
    node
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_louvain_simple_cliques() {
        // Create two cliques (communities)
        let mut nodes = Vec::new();
        for i in 1..=4 { nodes.push(format!("a{}", i)); }
        for i in 1..=4 { nodes.push(format!("b{}", i)); }
        
        let mut graph = WeightedGraph::new(nodes);
        
        // Clique A
        for i in 1..=4 {
            for j in i+1..=4 {
                graph.add_edge(&format!("a{}", i), &format!("a{}", j), 1.0);
            }
        }
        
        // Clique B
        for i in 1..=4 {
            for j in i+1..=4 {
                graph.add_edge(&format!("b{}", i), &format!("b{}", j), 1.0);
            }
        }
        
        // Single bridge edge
        graph.add_edge("a1", "b1", 0.1);
        
        let partition = graph.find_communities();
        
        // Ensure nodes in same clique are in same community
        assert_eq!(partition["a1"], partition["a2"]);
        assert_eq!(partition["b1"], partition["b2"]);
        // Ensure different cliques are in different communities
        assert_ne!(partition["a1"], partition["b1"]);
    }

    #[test]
    fn test_louvain_hierarchical_aggregation() {
        // Test that it handles multiple phases (aggregation)
        let mut nodes = Vec::new();
        for i in 1..=10 { nodes.push(format!("n{}", i)); }
        let mut graph = WeightedGraph::new(nodes);
        
        // Create a chain of triangles (highly clustered)
        for i in 0..8 {
            graph.add_edge(&format!("n{}", i+1), &format!("n{}", i+2), 1.0);
            graph.add_edge(&format!("n{}", i+1), &format!("n{}", i+3), 1.0);
            graph.add_edge(&format!("n{}", i+2), &format!("n{}", i+3), 1.0);
        }
        
        let partition = graph.find_communities();
        assert!(partition.len() == 10);
        // We just care it doesn't crash and returns a valid map
    }

    #[test]
    fn test_leiden_refinement_splits_disconnected_cliques() {
        // Create 2 completely disconnected cliques of nodes but pretend they are in the same community
        let mut nodes = Vec::new();
        for i in 1..=3 { nodes.push(format!("c1_{}", i)); }
        for i in 1..=3 { nodes.push(format!("c2_{}", i)); }
        
        let mut graph = WeightedGraph::new(nodes);
        
        // Clique 1
        graph.add_edge("c1_1", "c1_2", 1.0);
        graph.add_edge("c1_2", "c1_3", 1.0);
        
        // Clique 2
        graph.add_edge("c2_1", "c2_2", 1.0);
        graph.add_edge("c2_2", "c2_3", 1.0);
        
        // Let's create a partition mapping all of them to community 99
        let mut partition = HashMap::new();
        for i in 1..=3 {
            partition.insert(format!("c1_{}", i), 99);
            partition.insert(format!("c2_{}", i), 99);
        }
        
        let refined = graph.refine_partition(&partition);
        
        // Since clique 1 and clique 2 have absolutely NO connecting edges between them,
        // the refinement phase MUST split them into distinct community IDs!
        let comm_c1 = refined.get("c1_1").unwrap();
        let comm_c2 = refined.get("c2_1").unwrap();
        assert_ne!(comm_c1, comm_c2, "Disconnected components must be split during refinement");
    }
}

