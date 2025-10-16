pub mod sled_store;

use anyhow::Result;
use ledger_core::{Block, Hash};

/// Legacy trait kept for internal use; Chain uses the core's ChainStore trait.
pub trait Storage: Send + Sync {
    fn put_block(&self, block: &Block) -> Result<()>;
    fn get_block(&self, index: u64) -> Result<Option<Block>>;
    fn tip_height(&self) -> Result<u64>;
    fn tip_hash(&self) -> Result<Option<Hash>>;
}
