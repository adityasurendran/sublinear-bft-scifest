use std::collections::{HashMap, HashSet};
use crate::types::{Vertex, Hash, ValidatorId, CoA, CertifiedVertex};
use crate::crypto::{hash, vrf_sort_key};

pub struct Dag {
    pub vertices: HashMap<Hash, Vertex>,
    pub certs: HashMap<Hash, CoA>,
    pub round_to_vertices: HashMap<u64, Vec<Hash>>,
    pub committed_round: u64,
    pub n: usize,
    pub f: usize,
}

impl Dag {
    pub fn new(n: usize, f: usize) -> Self {
        Self {
            vertices: HashMap::new(),
            certs: HashMap::new(),
            round_to_vertices: HashMap::new(),
            committed_round: 0,
            n,
            f,
        }
    }

    pub fn validate_vertex(&self, vertex: &Vertex, parent_hashes: &[Hash]) -> bool {
        // 1. parents.len() == n - f
        if parent_hashes.len() != (self.n - self.f) {
            return false;
        }

        // 2. All authors distinct and all parents exist and are certified
        let mut authors = HashSet::new();
        for p_hash in parent_hashes {
            if let Some(p_vertex) = self.vertices.get(p_hash) {
                // 3. All parents from round r-1
                if p_vertex.round != vertex.round - 1 {
                    return false;
                }
                if !authors.insert(p_vertex.author) {
                    return false; // Not distinct
                }
                // 4. Parent must be certified
                if !self.certs.contains_key(p_hash) {
                    return false;
                }
            } else {
                return false; // Parent missing
            }
        }
        true
    }

    pub fn insert_certified(&mut self, cv: CertifiedVertex, v_hash: Hash) {
        self.vertices.insert(v_hash, cv.vertex.clone());
        self.certs.insert(v_hash, cv.coa);
        self.round_to_vertices.entry(cv.vertex.round).or_default().push(v_hash);
    }

    pub fn aether_sort(&self, anchor_hash: &Hash, seed: &Hash, dag_lookup: &HashMap<u64, Vec<Hash>>) -> Vec<Hash> {
        let mut reachable = Vec::new();
        let mut stack = vec![*anchor_hash];
        let mut visited = HashSet::new();

        while let Some(current) = stack.pop() {
            if visited.insert(current) {
                if let Some(vertex) = self.vertices.get(&current) {
                    if vertex.round > self.committed_round {
                        reachable.push(current);
                        if vertex.round > 0 {
                            for &idx in &vertex.parent_indices {
                                if let Some(p_hash) = dag_lookup.get(&(vertex.round - 1)).and_then(|list| list.get(idx as usize)) {
                                    stack.push(*p_hash);
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut layers: HashMap<u64, Vec<Hash>> = HashMap::new();
        for h in reachable {
            let v = &self.vertices[&h];
            layers.entry(v.round).or_default().push(h);
        }

        let mut sorted_log = Vec::new();
        let mut rounds: Vec<u64> = layers.keys().cloned().collect();
        rounds.sort();

        for r in rounds {
            let mut round_vertices = layers.remove(&r).unwrap();
            round_vertices.sort_by_key(|h| vrf_sort_key(h, seed));
            sorted_log.extend(round_vertices);
        }
        sorted_log
    }
}
