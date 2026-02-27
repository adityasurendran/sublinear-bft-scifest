use ed25519_dalek::{Signature, VerifyingKey, Verifier};
use blake3::Hasher;
use crate::types::{Hash, Signature as SigType};

pub fn hash(data: &[u8]) -> Hash {
    let mut hasher = Hasher::new();
    hasher.update(data);
    *hasher.finalize().as_bytes()
}

pub fn verify_signature(msg: &[u8], sig: &[u8], public_key: &VerifyingKey) -> bool {
    if let Ok(signature) = Signature::from_slice(sig) {
        return public_key.verify(msg, &signature).is_ok();
    }
    false
}

/// seed_r = Hash(sorted(CoA_signatures || round_number))
pub fn hash_vertex(vertex: &crate::types::Vertex) -> Hash {
    use rkyv::ser::serializers::AllocSerializer;
    use rkyv::ser::Serializer;
    let mut ser = AllocSerializer::<1024>::default();
    ser.serialize_value(vertex).unwrap();
    hash(&ser.into_serializer().into_inner())
}

pub fn derive_vrf_seed(mut signatures: Vec<SigType>, round: u64) -> Hash {
    signatures.sort();
    let mut hasher = Hasher::new();
    for sig in signatures {
        hasher.update(&sig);
    }
    hasher.update(&round.to_le_bytes());
    *hasher.finalize().as_bytes()
}

pub fn vrf_sort_key(vertex_hash: &Hash, seed: &Hash) -> Hash {
    let mut hasher = Hasher::new();
    hasher.update(vertex_hash);
    hasher.update(seed);
    *hasher.finalize().as_bytes()
}
