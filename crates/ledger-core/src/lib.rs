use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

pub type Hash = [u8; 32];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transaction {
  pub from: String,
  pub to: String,
  pub amount: u64,
  pub timestamp: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockHeader {
  pub index: u64,
  pub previous_hash: Hash,
  pub merkle_root: Hash,
  pub timestamp: u64,
  pub nonce: u64,
}

impl BlockHeader {
  pub fn new(index: u64, previous_hash: Hash, merkle_root: Hash, nonce: u64) -> Self {
    Self {
      index,
      previous_hash,
      merkle_root,
      timestamp: SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs(),
      nonce,
    }
  }

  pub fn hash_bytes(&self) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(8 + 32 + 32 + 8 + 8);
    bytes.extend_from_slice(&self.index.to_le_bytes());
    bytes.extend_from_slice(&self.previous_hash);
    bytes.extend_from_slice(&self.merkle_root);
    bytes.extend_from_slice(&self.timestamp.to_le_bytes());
    bytes.extend_from_slice(&self.nonce.to_le_bytes());
    bytes
  }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Block {
  pub header: BlockHeader,
  pub txs: Vec<Transaction>,
}

impl Block {
  pub fn hash(&self) -> Hash {
    let mut hasher = Sha256::new();
    hasher.update(self.header.hash_bytes());
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest[..]);
    out
  }
}

pub fn merkle_root(txs: &[Transaction]) -> Hash {
  if txs.is_empty() {
    return [0u8; 32];
  }
  let mut level: Vec<Hash> = txs
    .iter()
    .map(|t| {
      let mut hasher = Sha256::new();
      hasher.update(serde_json::to_vec(t).unwrap());
      let digest = hasher.finalize();
      let mut out = [0u8; 32];
      out.copy_from_slice(&digest[..]);
      out
    })
    .collect();

  while level.len() > 1 {
    let mut next = Vec::with_capacity((level.len() + 1) / 2);
    for pair in level.chunks(2) {
      let (a, b) = if pair.len() == 2 {
        (pair[0], pair[1])
      } else {
        (pair[0], pair[0])
      };
      let mut hasher = Sha256::new();
      hasher.update(a);
      hasher.update(b);
      let digest = hasher.finalize();
      let mut out = [0u8; 32];
      out.copy_from_slice(&digest[..]);
      next.push(out);
    }
    level = next;
  }
  level[0]
}

pub mod pow {
  use super::{Block, Hash};
  use sha2::{Digest, Sha256};

  /// Mine the block by incrementing nonce until the number of leading zero bits
  /// in the block hash >= `target_zeros`.
  pub fn mine_block(mut block: Block, target_zeros: u32) -> Block {
    loop {
      let mut hasher = Sha256::new();
      hasher.update(block.header.hash_bytes());
      let digest = hasher.finalize();
      let mut h = [0u8; 32];
      h.copy_from_slice(&digest[..]);

      if count_leading_zero_bits(&h) >= target_zeros {
        return block;
      }
      block.header.nonce = block.header.nonce.wrapping_add(1);
    }
  }

  pub fn count_leading_zero_bits(hash: &Hash) -> u32 {
    let mut total = 0u32;
    for b in hash {
      if *b == 0 {
        total += 8;
      } else {
        total += b.leading_zeros();
        break;
      }
    }
    total
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn leading_zero_bits_examples() {
    let mut h = [0u8; 32];
    assert_eq!(pow::count_leading_zero_bits(&h), 256);
    h[0] = 0x0F; // 00001111
    assert_eq!(pow::count_leading_zero_bits(&h), 4);
    h = [0u8; 32];
    h[1] = 0x80; // 00000000 10000000
    assert_eq!(pow::count_leading_zero_bits(&h), 8);
    h[1] = 0x40; // 01000000
    assert_eq!(pow::count_leading_zero_bits(&h), 9);
  }
}

pub mod chain {
  use super::*;
  use anyhow::Result;
  use std::sync::Arc;

  /// Trait the storage backends should implement for the chain to operate.
  /// This lives in `ledger-core` to avoid a circular dependency.
  pub trait ChainStore: Send + Sync {
    fn put_block(&self, block: &Block) -> Result<()>;
    fn get_block(&self, index: u64) -> Result<Option<Block>>;
    fn tip_height(&self) -> Result<u64>;
    fn tip_hash(&self) -> Result<Option<Hash>>;
  }

  /// Simple chain façade that delegates persistence to a `ChainStore`.
  #[derive(Clone)]
  pub struct Chain<S: ChainStore> {
    store: Arc<S>,
  }

  impl<S: ChainStore> Chain<S> {
    pub fn new(store: Arc<S>) -> Self {
      Self { store }
    }

    pub fn store(&self) -> &Arc<S> {
      &self.store
    }

    /// Ensure a genesis block exists. Idempotent.
    pub fn ensure_genesis(&self) -> Result<()> {
      let height = self.store.tip_height()?;
      // Height 0 can mean "empty" or "genesis at index 0". Check presence of block 0.
      if height == 0 {
        if self.store.get_block(0)?.is_none() {
          let genesis = genesis_block();
          self.store.put_block(&genesis)?;
        }
      }
      Ok(())
    }

    /// Return (height, tip_hash). Height is 0 for empty or at genesis index 0.
    pub fn tip(&self) -> Result<(u64, Option<Hash>)> {
      Ok((self.store.tip_height()?, self.store.tip_hash()?))
    }
  }

  /// A zero-transaction genesis block with zeroed prev-hash and merkle-root.
  pub fn genesis_block() -> Block {
    let header = BlockHeader::new(0, [0u8; 32], [0u8; 32], 0);
    Block {
      header,
      txs: vec![],
    }
  }
}
