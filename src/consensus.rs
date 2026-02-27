use std::collections::{HashMap, HashSet};
use crate::types::{Vertex, Hash, ValidatorId, CoA, Event, SkipCert, CertifiedVertex};
use crate::dag::Dag;
use crate::crypto::{derive_vrf_seed, hash};
use rkyv::ser::serializers::AllocSerializer;
use rkyv::ser::Serializer;

pub struct ConsensusState {
    pub dag: Dag,
    pub round: u64,
    pub validator_id: ValidatorId,
    pub n: usize,
    pub f: usize,
    pub coa_collectors: HashMap<Hash, HashMap<ValidatorId, Vec<u8>>>,
    pub skip_collectors: HashMap<(u64, u32), HashMap<ValidatorId, Vec<u8>>>,
    pub has_signed_skip: HashSet<(u64, u32)>,
    pub has_signed_coa: HashSet<Hash>,
    pub vrf_seeds: HashMap<u64, Hash>,
    pub fallback_depth: u32,
}

impl ConsensusState {
    pub fn new(validator_id: ValidatorId, n: usize) -> Self {
        let f = (n - 1) / 3;
        println!("DEBUG: Initializing ConsensusState for n={}, f={}", n, f);
        Self {
            dag: Dag::new(n, f),
            round: 1,
            validator_id,
            n,
            f,
            coa_collectors: HashMap::new(),
            skip_collectors: HashMap::new(),
            has_signed_skip: HashSet::new(),
            has_signed_coa: HashSet::new(),
            vrf_seeds: HashMap::new(),
            fallback_depth: 0,
        }
    }

    pub fn on_event(&mut self, event: Event) {
        match event {
            Event::VertexReceived(vertex) => self.handle_vertex(vertex),
            Event::CoAReceived(coa) => self.handle_coa(coa),
            _ => {}
        }
    }

    fn handle_vertex(&mut self, vertex: Vertex) {
        let v_hash = crate::crypto::hash_vertex(&vertex);
        self.dag.vertices.insert(v_hash, vertex.clone());
        self.dag.round_to_vertices.entry(vertex.round).or_default().push(v_hash);
    }

    fn handle_coa(&mut self, coa: CoA) {
        let v_hash = coa.batch_hash;
        let collector = self.coa_collectors.entry(v_hash).or_default();
        for (id, sig) in coa.signatures {
            collector.insert(id, sig);
        }
    }

    pub fn certify_vertex(&mut self, v_hash: Hash, signatures: Vec<(ValidatorId, Vec<u8>)>) {
        if let Some(vertex) = self.dag.vertices.get(&v_hash).cloned() {
            if !self.dag.certs.contains_key(&v_hash) {
                let cv = CertifiedVertex { vertex: vertex.clone(), coa: CoA { batch_hash: v_hash, signatures } };
                self.dag.insert_certified(cv, v_hash);
                if vertex.round > self.dag.committed_round {
                    self.dag.committed_round = vertex.round;
                }
            }
        }
    }

    pub fn get_pending_quorums(&self) -> Vec<(Hash, Vertex, Vec<(ValidatorId, Vec<u8>)>)> {
        let mut pending = Vec::new();
        let quorum_size = self.n - self.f;
        for (h, collector) in &self.coa_collectors {
            if !self.dag.certs.contains_key(h) && collector.len() >= quorum_size {
                if let Some(vertex) = self.dag.vertices.get(h) {
                    let signatures: Vec<_> = collector.iter().map(|(&k, v)| (k, v.clone())).collect();
                    pending.push((*h, vertex.clone(), signatures));
                }
            }
        }
        pending
    }
}
