use serde::{Deserialize, Serialize};
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use bytecheck::CheckBytes;

pub type ValidatorId = u32;
pub type Hash = [u8; 32];
pub type Signature = Vec<u8>;

// Phase E.4: BLS Aggregated Signature Types
// Using Vec for serde compatibility (protocol enforces exact byte sizes)
pub type BlsSignature = Vec<u8>;  // BLS12-381 G1 signature (48 bytes compressed)
pub type BlsPublicKey = Vec<u8>;  // BLS12-381 G2 public key (96 bytes compressed)
pub type SignerBitmap = u64;       // Supports up to 64 validators

#[derive(Clone, Serialize, Deserialize, Archive, RkyvDeserialize, RkyvSerialize, Debug)]
#[archive(check_bytes)]
pub struct Vertex {
    pub round: u64,
    pub author: ValidatorId,
    pub batch_hash: Hash,
    pub parent_indices: Vec<u32>,
}

#[derive(Clone, Serialize, Deserialize, Archive, RkyvDeserialize, RkyvSerialize, Debug)]
#[archive(check_bytes)]
pub struct CoA {
    pub batch_hash: Hash,
    pub signatures: Vec<(ValidatorId, Signature)>,
}

// Phase E.4: Aggregated Certificate of Availability (O(1) size)
// Total size: 32 (hash) + 48 (BLS G1 sig) + 8 (bitmap) = 88 bytes fixed
#[derive(Clone, Serialize, Deserialize, Archive, RkyvDeserialize, RkyvSerialize, Debug)]
#[archive(check_bytes)]
pub struct AggregatedCoA {
    pub batch_hash: Hash,
    pub aggregated_signature: BlsSignature,  // 48 bytes (G1 compressed)
    pub signer_bitmap: SignerBitmap,         // 8 bytes (up to 64 validators)
}

#[derive(Clone, Serialize, Deserialize, Archive, RkyvDeserialize, RkyvSerialize, Debug)]
#[archive(check_bytes)]
pub struct CertifiedVertex {
    pub vertex: Vertex,
    pub coa: CoA,
}

// Phase E.4: Certified vertex with aggregated certificate
#[derive(Clone, Serialize, Deserialize, Archive, RkyvDeserialize, RkyvSerialize, Debug)]
#[archive(check_bytes)]
pub struct AggregatedCertifiedVertex {
    pub vertex: Vertex,
    pub agg_coa: AggregatedCoA,
}

#[derive(Clone, Serialize, Deserialize, Archive, RkyvDeserialize, RkyvSerialize, Debug)]
#[archive(check_bytes)]
pub struct SkipCert {
    pub round: u64,
    pub anchor_index: ValidatorId,
    pub signatures: Vec<(ValidatorId, Signature)>,
}

pub enum VertexState {
    Pending,
    Certified(CoA),
}

pub enum Event {
    VertexReceived(Vertex),
    CoAReceived(CoA),
    AggregatedCoAReceived(AggregatedCoA),  // Phase E.4
    SkipVoteReceived(u64, u32, ValidatorId, Signature),
    Timeout(u64),
}

#[derive(Clone, Serialize, Deserialize, Archive, RkyvDeserialize, RkyvSerialize, Debug)]
#[archive(check_bytes)]
pub enum Message {
    Vertex(Vertex),
    CoA(CoA),
    AggregatedCoA(AggregatedCoA),  // Phase E.4
    SkipVote(u64, u32, ValidatorId, Signature),
}

impl Message {
    pub fn coa(&self) -> Option<&CoA> {
        match self {
            Message::CoA(coa) => Some(coa),
            _ => None,
        }
    }
}
