use crate::Storage;
use anyhow::{Context, Result};
use ledger_core::{Block, Hash};
use sled::{Db, IVec};
use std::path::Path;
use tracing::info;

const TREE_BLOCKS: &str = "blocks";
const KEY_TIP_HEIGHT: &[u8] = b"tip_height";
const KEY_TIP_HASH: &[u8] = b"tip_hash";

#[derive(Clone)]
pub struct SledStore {
  db: Db,
}

impl SledStore {
  pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
    let db = sled::open(path)?;
    info!("sled store opened");
    Ok(Self { db })
  }

  fn blocks(&self) -> sled::Tree {
    self.db.open_tree(TREE_BLOCKS).expect("open tree")
  }
}

impl Storage for SledStore {
  fn put_block(&self, block: &Block) -> Result<()> {
    let tree = self.blocks();
    let key = block.header.index.to_be_bytes();
    let bytes = bincode::serialize(block)?;
    tree.insert(key, bytes)?;

    // update tip
    self
      .db
      .insert(KEY_TIP_HEIGHT, &block.header.index.to_be_bytes())?;
    self.db.insert(KEY_TIP_HASH, &block.hash())?;

    self.db.flush()?;
    Ok(())
  }

  fn get_block(&self, index: u64) -> Result<Option<Block>> {
    let tree = self.blocks();
    let key = index.to_be_bytes();
    let opt = tree.get(key)?;
    Ok(opt.map(|ivec: IVec| bincode::deserialize(&ivec).unwrap()))
  }

  fn tip_height(&self) -> Result<u64> {
    Ok(
      self
        .db
        .get(KEY_TIP_HEIGHT)?
        .map(|v| {
          let mut arr = [0u8; 8];
          arr.copy_from_slice(&v);
          u64::from_be_bytes(arr)
        })
        .unwrap_or(0),
    )
  }

  fn tip_hash(&self) -> Result<Option<Hash>> {
    Ok(self.db.get(KEY_TIP_HASH)?.map(|v| {
      let mut arr = [0u8; 32];
      arr.copy_from_slice(&v);
      arr
    }))
  }
}
