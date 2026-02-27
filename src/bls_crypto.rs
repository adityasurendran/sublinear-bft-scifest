// Phase F.2: Optimized BLS with caching and batching
use blst::min_pk as blst_core;
use blst::blst_scalar;
use std::time::Instant;
use std::collections::HashMap;
use crate::types::{BlsSignature, BlsPublicKey, ValidatorId, SignerBitmap};
use once_cell::sync::Lazy;
use parking_lot::Mutex;

/// Metrics for BLS verification performance
#[derive(Debug, Clone)]
pub struct BlsVerifyMetrics {
    pub verify_micros: u64,
    pub pairing_count: u64,
}

/// Metrics for BLS aggregation performance
#[derive(Debug, Clone, Default)]
pub struct BlsAggregateMetrics {
    pub aggregate_micros: u64,
    pub signature_count: usize,
}

/// Global cache for uncompressed public keys to avoid redundant ECC ops
static PK_CACHE: Lazy<Mutex<HashMap<BlsPublicKey, blst_core::PublicKey>>> = Lazy::new(|| Mutex::new(HashMap::new()));

/// Global cache for uncompressed signatures
static SIG_CACHE: Lazy<Mutex<HashMap<BlsSignature, blst_core::Signature>>> = Lazy::new(|| Mutex::new(HashMap::new()));

/// Global cache for aggregate public keys by bitmap
static AGG_PK_CACHE: Lazy<Mutex<HashMap<SignerBitmap, blst_core::PublicKey>>> = Lazy::new(|| Mutex::new(HashMap::new()));

/// BLS12-381 secret key using blst
#[derive(Clone)]
pub struct BlsSecretKey {
    sk: blst_core::SecretKey,
}

impl BlsSecretKey {
    pub fn generate<R: rand::RngCore>(rng: &mut R) -> Self {
        let mut ikm = [0u8; 32];
        rng.fill_bytes(&mut ikm);
        let sk = blst_core::SecretKey::key_gen(&ikm, &[]).unwrap();
        Self { sk }
    }

    pub fn sign(&self, msg: &[u8]) -> BlsSignature {
        let dst = b"BLS_SIG_BLS12381G1_XMD:SHA-256_SSWU_RO_NUL_";
        let sig = self.sk.sign(msg, dst, &[]);
        sig.compress().to_vec()
    }

    pub fn public_key(&self) -> BlsPublicKey {
        let pk = self.sk.sk_to_pk();
        pk.compress().to_vec()
    }
}

pub fn aggregate_signatures_with_metrics(
    signatures: &[(ValidatorId, BlsSignature)],
    expected_quorum: usize,
) -> Result<(BlsSignature, SignerBitmap, BlsAggregateMetrics), &'static str> {
    let start = Instant::now();
    
    if signatures.len() < expected_quorum {
        return Err("Insufficient signatures");
    }

    let mut bitmap: SignerBitmap = 0;
    let mut decoded_sigs = Vec::new();

    {
        let mut sig_cache = SIG_CACHE.lock();
        for (id, sig_bytes) in signatures {
            if *id >= 64 { return Err("ID out of range"); }
            if (bitmap & (1u64 << id)) != 0 { return Err("Duplicate signer"); }
            
            let sig = if let Some(s) = sig_cache.get(sig_bytes) {
                s.clone()
            } else {
                let s = blst_core::Signature::uncompress(sig_bytes)
                    .map_err(|_| "Invalid signature format")?;
                sig_cache.insert(sig_bytes.clone(), s.clone());
                s
            };
            
            decoded_sigs.push(sig);
            bitmap |= 1u64 << id;
        }
    }
    
    let sig_refs: Vec<&blst_core::Signature> = decoded_sigs.iter().collect();
    let agg = blst_core::AggregateSignature::aggregate(&sig_refs, true)
        .map_err(|_| "Aggregation failed")?;

    let elapsed = start.elapsed().as_micros() as u64;
    Ok((agg.to_signature().compress().to_vec(), bitmap, BlsAggregateMetrics {
        aggregate_micros: elapsed,
        signature_count: decoded_sigs.len(),
    }))
}

fn generate_random_scalars(count: usize) -> Vec<blst_scalar> {
    use rand::RngCore;
    let mut rng = rand::thread_rng();
    (0..count)
        .map(|_| {
            let mut scalar_bytes = [0u8; 32];
            rng.fill_bytes(&mut scalar_bytes);
            scalar_bytes[31] &= 0x1F; 
            blst_scalar { b: scalar_bytes }
        })
        .collect()
}

pub fn verify_aggregated_with_metrics(
    msg: &[u8],
    agg_sig: &BlsSignature,
    public_keys: &[(ValidatorId, BlsPublicKey)],
    bitmap: SignerBitmap,
    expected_quorum: usize,
) -> (bool, BlsVerifyMetrics) {
    let items = vec![(msg, agg_sig, public_keys, bitmap, expected_quorum)];
    verify_aggregated_batch_with_metrics(items)
}

pub fn verify_aggregated_batch_with_metrics(
    items: Vec<(&[u8], &BlsSignature, &[(ValidatorId, BlsPublicKey)], SignerBitmap, usize)>,
) -> (bool, BlsVerifyMetrics) {
    let start = Instant::now();
    let dst = b"BLS_SIG_BLS12381G1_XMD:SHA-256_SSWU_RO_NUL_";
    
    if items.is_empty() {
        return (true, BlsVerifyMetrics { verify_micros: 0, pairing_count: 0 });
    }

    let mut all_msgs = Vec::with_capacity(items.len());
    let mut all_sigs = Vec::with_capacity(items.len());
    let mut all_pks = Vec::with_capacity(items.len());

    for (msg, agg_sig_bytes, pks_list, bitmap, expected_quorum) in &items {
        let sig = {
            let mut sig_cache = SIG_CACHE.lock();
            if let Some(s) = sig_cache.get(*agg_sig_bytes) {
                s.clone()
            } else {
                let s = match blst_core::Signature::uncompress(agg_sig_bytes) {
                    Ok(s) => s,
                    Err(_) => return (false, BlsVerifyMetrics { verify_micros: 0, pairing_count: 0 }),
                };
                sig_cache.insert((*agg_sig_bytes).clone(), s.clone());
                s
            }
        };
        
        // Use cache for aggregate public key
        let agg_pk: blst_core::PublicKey = {
            let mut cache = AGG_PK_CACHE.lock();
            if let Some(pk) = cache.get(bitmap) {
                pk.clone()
            } else {
                let mut group_pks = Vec::new();
                let mut pk_cache = PK_CACHE.lock();
                let mut count = 0;
                
                for (id, pk_bytes) in *pks_list {
                    if (bitmap & (1u64 << id)) != 0 {
                        let pk: blst_core::PublicKey = if let Some(p) = pk_cache.get(pk_bytes) {
                            p.clone()
                        } else {
                            let p = match blst_core::PublicKey::uncompress(pk_bytes) {
                                Ok(p) => p,
                                Err(_) => return (false, BlsVerifyMetrics { verify_micros: 0, pairing_count: 0 }),
                            };
                            pk_cache.insert(pk_bytes.clone(), p.clone());
                            p
                        };
                        group_pks.push(pk);
                        count += 1;
                    }
                }
                
                if count < *expected_quorum {
                    return (false, BlsVerifyMetrics { verify_micros: 0, pairing_count: 0 });
                }
                
                let group_pk_refs: Vec<&blst_core::PublicKey> = group_pks.iter().collect();
                let agg_pk_res = match blst_core::AggregatePublicKey::aggregate(&group_pk_refs, true) {
                    Ok(a) => a.to_public_key(),
                    Err(_) => return (false, BlsVerifyMetrics { verify_micros: 0, pairing_count: 0 }),
                };
                cache.insert(*bitmap, agg_pk_res.clone());
                agg_pk_res
            }
        };
        
        all_msgs.push(*msg);
        all_sigs.push(sig);
        all_pks.push(agg_pk);
    }

    let valid = if all_sigs.len() > 1 {
        let sig_refs: Vec<&blst_core::Signature> = all_sigs.iter().collect();
        let pk_refs: Vec<&blst_core::PublicKey> = all_pks.iter().collect();
        let randoms = generate_random_scalars(all_sigs.len());
        
        let err = blst_core::Signature::verify_multiple_aggregate_signatures(
            &all_msgs, dst, &pk_refs, true, &sig_refs, true, &randoms, 128
        );
        err == blst::BLST_ERROR::BLST_SUCCESS
    } else {
        all_sigs[0].verify(true, all_msgs[0], dst, &[], &all_pks[0], true) == blst::BLST_ERROR::BLST_SUCCESS
    };

    let elapsed = start.elapsed().as_micros() as u64;
    let pairing_count = if all_sigs.len() > 1 {
        (all_sigs.len() + 1) as u64
    } else {
        2
    };
    (valid, BlsVerifyMetrics { verify_micros: elapsed, pairing_count })
}

pub fn verify_aggregated(
    msg: &[u8],
    agg_sig: &BlsSignature,
    public_keys: &[(ValidatorId, BlsPublicKey)],
    bitmap: SignerBitmap,
    expected_quorum: usize,
) -> bool {
    verify_aggregated_with_metrics(msg, agg_sig, public_keys, bitmap, expected_quorum).0
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;

    #[test]
    fn test_blst_basic() {
        let mut rng = OsRng;
        let sk = BlsSecretKey::generate(&mut rng);
        let pk = sk.public_key();
        let msg = b"hello blst";
        let sig = sk.sign(msg);
        let (valid, _) = verify_aggregated_with_metrics(msg, &sig, &[(0, pk)], 1, 1);
        assert!(valid);
    }

    #[test]
    fn test_blst_aggregation() {
        let mut rng = OsRng;
        let sk1 = BlsSecretKey::generate(&mut rng);
        let sk2 = BlsSecretKey::generate(&mut rng);
        let msg = b"consensus message";
        let sig1 = sk1.sign(msg);
        let sig2 = sk2.sign(msg);
        let (agg, bitmap, _) = aggregate_signatures_with_metrics(&[(0, sig1), (1, sig2)], 2).unwrap();
        let pks = vec![(0, sk1.public_key()), (1, sk2.public_key())];
        let (valid, _) = verify_aggregated_with_metrics(msg, &agg, &pks, bitmap, 2);
        assert!(valid);
    }
}
