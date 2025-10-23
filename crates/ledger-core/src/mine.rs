use crate::{
    block_data_hash, block_header_hash, merkle_root, pow::count_leading_zero_bits, Block,
    BlockHeader, Transaction,
};
use rayon::prelude::*;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;

/// Mines a block by searching nonces in parallel until a header hash has at least `target` leading zero bits.
/// Returns the mined Block (with header.nonce set) and its hash.
pub fn mine_block_parallel(
    index: u64,
    prev_hash: [u8; 32],
    txs: Vec<Transaction>,
    data: Option<String>,
    target: u32,
) -> (Block, [u8; 32]) {
    // Construct a header "template" (we'll vary only the nonce per attempt).
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went backwards")
        .as_secs();

    let merkle = merkle_root(&txs);
    let data_hash = block_data_hash(&data);

    // We’ll reuse this structure and mutate the nonce per attempt.
    let mut header = BlockHeader::new(index, prev_hash, data_hash, merkle, timestamp);
    header.nonce = 0;

    // Prepare immutable parts for hashing closure
    let base_header = header;

    // Parallel search over the entire u64 range. Rayon will split this range across threads.
    let found = (0u64..u64::MAX)
        .into_par_iter()
        .find_any(|nonce| {
            let mut h = base_header;
            h.nonce = *nonce;
            let hash = block_header_hash(h);
            count_leading_zero_bits(&hash) >= target
        })
        .expect("nonce space exhausted (practically impossible)");

    // Build final block with the winning nonce and hash
    let mut final_header = base_header;
    final_header.nonce = found;
    let final_hash = block_header_hash(final_header);

    info!(
        "Mined block {} with nonce {} and hash {:x?}",
        index, found, final_hash
    );

    let block = Block {
        header: final_header,
        data,
        txs,
    };
    (block, final_hash)
}
