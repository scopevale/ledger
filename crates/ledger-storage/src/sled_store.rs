use crate::Storage;
use anyhow::{Ok, Result};
use ledger_core::constants::HASH_SIZE;
use ledger_core::{Block, Hash};
use sled::{Db, IVec};
use std::path::Path;
use tracing::info;

const TREE_BLOCKS: &str = "blocks";
const KEY_TIP_HEIGHT: &[u8] = b"tip_height";
const KEY_TIP_HASH: &[u8] = b"tip_hash";

#[derive(Clone, Debug)]
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

    // Additional method to clear the database (for testing purposes)
    pub fn clear(&self) -> Result<()> {
        self.db.drop_tree(TREE_BLOCKS)?;
        self.db.remove(KEY_TIP_HEIGHT)?;
        self.db.remove(KEY_TIP_HASH)?;
        self.db.flush()?;
        Ok(())
    }

    pub fn list_blocks_range(
        &self,
        start: u64,
        limit: u32,
        desc: bool,
    ) -> anyhow::Result<Vec<ledger_core::Block>> {
        let tree = self.blocks();
        let mut out = Vec::with_capacity(limit as usize);
        if desc {
            // iterate downwards from `start`
            let start_key = start.to_be_bytes();
            for kv in tree.range(..=start_key).rev().take(limit as usize) {
                let (_, v) = kv?;
                out.push(bincode::deserialize(&v)?);
            }
        } else {
            let start_key = start.to_be_bytes();
            for kv in tree.range(start_key..).take(limit as usize) {
                let (_, v) = kv?;
                out.push(bincode::deserialize(&v)?);
            }
        }
        Ok(out)
    }
}

impl Storage for SledStore {
    fn put_block(&self, block: &Block) -> Result<()> {
        if self.get_block(block.header.index)?.is_some() {
            // Block already exists, no-op
            tracing::debug!("Block {:?} already exists, skipping insert", block.hash());
            return Ok(());
        }

        let tree = self.blocks();
        let key = block.header.index.to_be_bytes();
        let bytes = bincode::serialize(block)?;

        if let Err(e) = tree.insert(key, bytes) {
            tracing::error!("Error inserting Block {:?}: {:?}", key, e);
            return Err(e.into());
        }

        // update tip
        self.db
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
        Ok(self
            .db
            .get(KEY_TIP_HEIGHT)?
            .map(|v| {
                let mut arr = [0u8; 8];
                arr.copy_from_slice(&v);
                u64::from_be_bytes(arr)
            })
            .unwrap_or(0))
    }

    fn tip_hash(&self) -> Result<Option<Hash>> {
        Ok(self.db.get(KEY_TIP_HASH)?.map(|v| {
            let mut arr = [0u8; HASH_SIZE];
            arr.copy_from_slice(&v);
            arr
        }))
    }

    // Additional method to close the database
    fn close(&self) -> Result<()> {
        self.db.flush()?;
        Ok(())
    }
}

/// Implement the core ChainStore trait for our SledStore backend.
impl ledger_core::chain::ChainStore for SledStore {
    fn put_block(&self, block: &Block) -> anyhow::Result<()> {
        <Self as crate::Storage>::put_block(self, block)
    }
    fn get_block(&self, index: u64) -> anyhow::Result<Option<Block>> {
        <Self as crate::Storage>::get_block(self, index)
    }
    fn tip_height(&self) -> anyhow::Result<u64> {
        <Self as crate::Storage>::tip_height(self)
    }
    fn tip_hash(&self) -> anyhow::Result<Option<Hash>> {
        <Self as crate::Storage>::tip_hash(self)
    }
    fn close(&self) -> anyhow::Result<()> {
        <Self as crate::Storage>::close(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// test database open/close
    #[test]
    fn test_open_close() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store1 = SledStore::open(temp_dir.path()).unwrap();
        assert_eq!(store1.tip_height().unwrap(), 0);
        drop(store1);
        // reopen
        let store2 = SledStore::open(temp_dir.path()).unwrap();
        assert_eq!(store2.tip_height().unwrap(), 0);
    }

    /// test put/get block
    #[test]
    fn test_put_get_block() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = SledStore::open(temp_dir.path()).unwrap();
        let block = Block {
            header: ledger_core::BlockHeader {
                index: 1,
                previous_hash: [0u8; HASH_SIZE],
                data_hash: [0u8; HASH_SIZE],
                merkle_root: [0u8; HASH_SIZE],
                timestamp: 0,
                nonce: 0,
            },
            txs: vec![],
            data: None,
        };
        store.put_block(&block).unwrap();
        assert_eq!(store.tip_height().unwrap(), 1);
        assert_eq!(store.tip_hash().unwrap().unwrap(), block.hash());
        let fetched = store.get_block(1).unwrap().unwrap();
        assert_eq!(fetched.header.index, 1);
        assert_eq!(fetched.hash(), block.hash());
        let none = store.get_block(2).unwrap();
        assert!(none.is_none());
    }

    /// test genesis block handling
    #[test]
    fn test_genesis_block() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = SledStore::open(temp_dir.path()).unwrap();
        let chain = ledger_core::chain::Chain::new(std::sync::Arc::new(store.clone()));
        chain.ensure_genesis().unwrap();
        assert_eq!(store.tip_height().unwrap(), 0);
        let genesis = store.get_block(0).unwrap().unwrap();
        assert_eq!(genesis.header.index, 0);
        // ensure_genesis is idempotent
        chain.ensure_genesis().unwrap();
        assert_eq!(store.tip_height().unwrap(), 0);
    }

    /// test multiple blocks
    #[test]
    fn test_multiple_blocks() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = SledStore::open(temp_dir.path()).unwrap();
        let chain = ledger_core::chain::Chain::new(std::sync::Arc::new(store.clone()));
        chain.ensure_genesis().unwrap();
        for i in 1..=5 {
            let prev_hash = store.tip_hash().unwrap().unwrap_or([0u8; HASH_SIZE]);
            let block = Block {
                header: ledger_core::BlockHeader {
                    index: i,
                    previous_hash: prev_hash,
                    data_hash: [0u8; HASH_SIZE],
                    merkle_root: [0u8; HASH_SIZE],
                    timestamp: 0,
                    nonce: 0,
                },
                txs: vec![],
                data: None,
            };
            store.put_block(&block).unwrap();
            assert_eq!(store.tip_height().unwrap(), i);
            assert_eq!(store.tip_hash().unwrap().unwrap(), block.hash());
        }
        for i in 0..=5 {
            let fetched = store.get_block(i).unwrap().unwrap();
            assert_eq!(fetched.header.index, i);
        }
        let none = store.get_block(6).unwrap();
        assert!(none.is_none());
    }

    /// test persistence across reopen
    #[test]
    fn test_persistence() {
        let temp_dir = tempfile::tempdir().unwrap();
        {
            let store = SledStore::open(temp_dir.path()).unwrap();
            let block = Block {
                header: ledger_core::BlockHeader {
                    index: 1,
                    previous_hash: [0u8; HASH_SIZE],
                    data_hash: [0u8; HASH_SIZE],
                    merkle_root: [0u8; HASH_SIZE],
                    timestamp: 0,
                    nonce: 0,
                },
                txs: vec![],
                data: None,
            };
            store.put_block(&block).unwrap();
            assert_eq!(store.tip_height().unwrap(), 1);
        }
        // reopen
        {
            let store = SledStore::open(temp_dir.path()).unwrap();
            assert_eq!(store.tip_height().unwrap(), 1);
            let fetched = store.get_block(1).unwrap().unwrap();
            assert_eq!(fetched.header.index, 1);
        }
    }

    /// test empty store
    #[test]
    fn test_empty_store() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = SledStore::open(temp_dir.path()).unwrap();
        assert_eq!(store.tip_height().unwrap(), 0);
        assert!(store.tip_hash().unwrap().is_none());
        assert!(store.get_block(0).unwrap().is_none());
    }

    /// test concurrent access
    #[test]
    fn test_concurrent_access() {
        use std::sync::Arc;
        use std::thread;
        let temp_dir = tempfile::tempdir().unwrap();
        let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());
        let mut handles = vec![];
        for i in 0..10 {
            let store = store.clone();
            let handle = thread::spawn(move || {
                let block = Block {
                    header: ledger_core::BlockHeader {
                        index: i,
                        previous_hash: [0u8; HASH_SIZE],
                        data_hash: [0u8; HASH_SIZE],
                        merkle_root: [0u8; HASH_SIZE],
                        timestamp: 0,
                        nonce: 0,
                    },
                    txs: vec![],
                    data: None,
                };
                store.put_block(&block).unwrap();
            });
            handles.push(handle);
        }
        for handle in handles {
            handle.join().unwrap();
        }
        // Check that all blocks are present
        for i in 0..10 {
            let fetched = store.get_block(i).unwrap().unwrap();
            assert_eq!(fetched.header.index, i);
        }
    }

    /// test large number of blocks
    #[test]
    fn test_large_number_of_blocks() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = SledStore::open(temp_dir.path()).unwrap();
        let num_blocks = 1000;
        for i in 0..num_blocks {
            let prev_hash = store.tip_hash().unwrap().unwrap_or([0u8; HASH_SIZE]);
            let block = Block {
                header: ledger_core::BlockHeader {
                    index: i,
                    previous_hash: prev_hash,
                    data_hash: [0u8; HASH_SIZE],
                    merkle_root: [0u8; HASH_SIZE],
                    timestamp: 0,
                    nonce: 0,
                },
                txs: vec![],
                data: None,
            };
            store.put_block(&block).unwrap();
            assert_eq!(store.tip_height().unwrap(), i);
            assert_eq!(store.tip_hash().unwrap().unwrap(), block.hash());
        }
        for i in 0..num_blocks {
            let fetched = store.get_block(i).unwrap().unwrap();
            assert_eq!(fetched.header.index, i);
        }
    }

    /// test re-adding the same block (idempotency)
    #[test]
    fn test_readding_same_block() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = SledStore::open(temp_dir.path()).unwrap();
        let block = Block {
            header: ledger_core::BlockHeader {
                index: 1,
                previous_hash: [0u8; HASH_SIZE],
                data_hash: [0u8; HASH_SIZE],
                merkle_root: [0u8; HASH_SIZE],
                timestamp: 0,
                nonce: 0,
            },
            txs: vec![],
            data: None,
        };
        store.put_block(&block).unwrap();
        assert_eq!(store.tip_height().unwrap(), 1);
        assert_eq!(store.tip_hash().unwrap().unwrap(), block.hash());
        // Re-add the same block
        store.put_block(&block).unwrap();
        assert_eq!(store.tip_height().unwrap(), 1);
        assert_eq!(store.tip_hash().unwrap().unwrap(), block.hash());
        let fetched = store.get_block(1).unwrap().unwrap();
        assert_eq!(fetched.header.index, 1);
        assert_eq!(fetched.hash(), block.hash());
    }

    /// test deleting a block (not supported, should be handled by caller)
    #[test]
    fn test_deleting_block() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = SledStore::open(temp_dir.path()).unwrap();
        let block = Block {
            header: ledger_core::BlockHeader {
                index: 1,
                previous_hash: [0u8; HASH_SIZE],
                data_hash: [0u8; HASH_SIZE],
                merkle_root: [0u8; HASH_SIZE],
                timestamp: 0,
                nonce: 0,
            },
            txs: vec![],
            data: None,
        };
        store.put_block(&block).unwrap();
        assert_eq!(store.tip_height().unwrap(), 1);
        // Deleting is not supported in this implementation
        // But we can simulate by removing directly from sled (not recommended)
        let tree = store.blocks();
        tree.remove(1u64.to_be_bytes()).unwrap();
        assert!(store.get_block(1).unwrap().is_none());
        // Tip height and hash remain unchanged; application logic should handle consistency
        assert_eq!(store.tip_height().unwrap(), 1);
    }

    /// test storing and retrieving a block with transactions
    #[test]
    fn test_block_with_transactions() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = SledStore::open(temp_dir.path()).unwrap();
        let tx1 = ledger_core::Transaction {
            from: "Alice".to_string(),
            to: "Bob".to_string(),
            amount: 10,
            timestamp: 1_600_000_000,
        };
        let tx2 = ledger_core::Transaction {
            from: "Bob".to_string(),
            to: "Charlie".to_string(),
            amount: 5,
            timestamp: 1_600_000_100,
        };
        let block = Block {
            header: ledger_core::BlockHeader {
                index: 1,
                previous_hash: [0u8; HASH_SIZE],
                data_hash: [0u8; HASH_SIZE],
                merkle_root: ledger_core::merkle_root(&[tx1.clone(), tx2.clone()]),
                timestamp: 0,
                nonce: 0,
            },
            txs: vec![tx1.clone(), tx2.clone()],
            data: None,
        };

        store.put_block(&block).unwrap();
        assert_eq!(store.tip_height().unwrap(), 1);
        assert_eq!(store.tip_hash().unwrap().unwrap(), block.hash());
        let fetched = store.get_block(1).unwrap().unwrap();
        assert_eq!(fetched.header.index, 1);
        assert_eq!(fetched.txs.len(), 2);
        assert_eq!(fetched.txs[0], tx1);
        assert_eq!(fetched.txs[1], tx2);
    }

    /// test storing and retrieving blocks with non-sequential indices (should be handled by caller)
    #[test]
    fn test_non_sequential_blocks() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = SledStore::open(temp_dir.path()).unwrap();
        let block1 = Block {
            header: ledger_core::BlockHeader {
                index: 1,
                previous_hash: [0u8; HASH_SIZE],
                data_hash: [0u8; HASH_SIZE],
                merkle_root: [0u8; HASH_SIZE],
                timestamp: 0,
                nonce: 0,
            },
            txs: vec![],
            data: None,
        };
        let block3 = Block {
            header: ledger_core::BlockHeader {
                index: 3,
                data_hash: [0u8; HASH_SIZE],
                previous_hash: [0u8; HASH_SIZE],
                merkle_root: [0u8; HASH_SIZE],
                timestamp: 0,
                nonce: 0,
            },
            txs: vec![],
            data: None,
        };
        store.put_block(&block1).unwrap();
        store.put_block(&block3).unwrap();
        assert_eq!(store.tip_height().unwrap(), 3);
        assert_eq!(store.tip_hash().unwrap().unwrap(), block3.hash());
        let fetched1 = store.get_block(1).unwrap().unwrap();
        assert_eq!(fetched1.header.index, 1);
        let fetched3 = store.get_block(3).unwrap().unwrap();
        assert_eq!(fetched3.header.index, 3);
        assert!(store.get_block(2).unwrap().is_none());
    }

    /// test storing and retrieving a large block
    #[test]
    fn test_large_block() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = SledStore::open(temp_dir.path()).unwrap();
        let mut txs = vec![];
        for i in 0..1000 {
            let tx = ledger_core::Transaction {
                from: format!("User{}", i),
                to: format!("User{}", i + 1),
                amount: i as u64,
                timestamp: 1_600_000_000 + i as u64,
            };
            txs.push(tx);
        }
        let block = Block {
            header: ledger_core::BlockHeader {
                index: 1,
                previous_hash: [0u8; HASH_SIZE],
                data_hash: [0u8; HASH_SIZE],
                merkle_root: ledger_core::merkle_root(&txs),
                timestamp: 0,
                nonce: 0,
            },
            txs: txs.clone(),
            data: None,
        };
        store.put_block(&block).unwrap();
        assert_eq!(store.tip_height().unwrap(), 1);
        assert_eq!(store.tip_hash().unwrap().unwrap(), block.hash());
        let fetched = store.get_block(1).unwrap().unwrap();
        assert_eq!(fetched.header.index, 1);
        assert_eq!(fetched.txs.len(), 1000);
        for i in 0..1000 {
            assert_eq!(fetched.txs[i], txs[i]);
        }
    }

    /// test storing and retrieving blocks with the same index (should be handled by caller)
    #[test]
    fn test_blocks_with_same_index() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = SledStore::open(temp_dir.path()).unwrap();
        let block1 = Block {
            header: ledger_core::BlockHeader {
                index: 1,
                previous_hash: [0u8; HASH_SIZE],
                data_hash: [0u8; HASH_SIZE],
                merkle_root: [0u8; HASH_SIZE],
                timestamp: 0,
                nonce: 0,
            },
            txs: vec![],
            data: None,
        };
        let block2 = Block {
            header: ledger_core::BlockHeader {
                index: 1, // same index as block1
                previous_hash: [1u8; HASH_SIZE],
                data_hash: [0u8; HASH_SIZE],
                merkle_root: [1u8; HASH_SIZE],
                timestamp: 0,
                nonce: 0,
            },
            txs: vec![],
            data: None,
        };
        store.put_block(&block1).unwrap();
        // Storing block2 with the same index will NOT overwrite block1
        store.put_block(&block2).unwrap();
        assert_eq!(store.tip_height().unwrap(), 1);
        assert_eq!(store.tip_hash().unwrap().unwrap(), block1.hash());
        let fetched = store.get_block(1).unwrap().unwrap();
        assert_eq!(fetched.header.index, 1);
        assert_eq!(fetched.hash(), block1.hash());
    }

    /// test storing and retrieving blocks with large indices
    #[test]
    fn test_blocks_with_large_indices() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = SledStore::open(temp_dir.path()).unwrap();
        let block1 = Block {
            header: ledger_core::BlockHeader {
                index: u64::MAX - 1,
                previous_hash: [0u8; HASH_SIZE],
                data_hash: [0u8; HASH_SIZE],
                merkle_root: [0u8; HASH_SIZE],
                timestamp: 0,
                nonce: 0,
            },
            txs: vec![],
            data: None,
        };
        let block2 = Block {
            header: ledger_core::BlockHeader {
                index: u64::MAX,
                previous_hash: block1.hash(),
                data_hash: [0u8; HASH_SIZE],
                merkle_root: [1u8; HASH_SIZE],
                timestamp: 0,
                nonce: 0,
            },
            txs: vec![],
            data: None,
        };
        store.put_block(&block1).unwrap();
        store.put_block(&block2).unwrap();
        assert_eq!(store.tip_height().unwrap(), u64::MAX);
        assert_eq!(store.tip_hash().unwrap().unwrap(), block2.hash());
        let fetched1 = store.get_block(u64::MAX - 1).unwrap().unwrap();
        assert_eq!(fetched1.header.index, u64::MAX - 1);
        let fetched2 = store.get_block(u64::MAX).unwrap().unwrap();
        assert_eq!(fetched2.header.index, u64::MAX);
    }

    /// test storing and retrieving blocks with non-ASCII characters in transactions
    #[test]
    fn test_blocks_with_non_ascii_transactions() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = SledStore::open(temp_dir.path()).unwrap();
        let tx1 = ledger_core::Transaction {
            from: "Алиса".to_string(), // "Alice" in Russian
            to: "Боб".to_string(),     // "Bob" in Russian
            amount: 10,
            timestamp: 1_600_000_000,
        };
        let tx2 = ledger_core::Transaction {
            from: "ボブ".to_string(),     // "Bob" in Japanese
            to: "チャーリー".to_string(), // "Charlie" in Japanese
            amount: 5,
            timestamp: 1_600_000_100,
        };
        let block = Block {
            header: ledger_core::BlockHeader {
                index: 1,
                previous_hash: [0u8; HASH_SIZE],
                data_hash: [0u8; HASH_SIZE],
                merkle_root: ledger_core::merkle_root(&[tx1.clone(), tx2.clone()]),
                timestamp: 0,
                nonce: 0,
            },
            txs: vec![tx1.clone(), tx2.clone()],
            data: None,
        };
        store.put_block(&block).unwrap();
        assert_eq!(store.tip_height().unwrap(), 1);
        assert_eq!(store.tip_hash().unwrap().unwrap(), block.hash());
        let fetched = store.get_block(1).unwrap().unwrap();
        assert_eq!(fetched.header.index, 1);
        assert_eq!(fetched.txs.len(), 2);
        assert_eq!(fetched.txs[0], tx1);
        assert_eq!(fetched.txs[1], tx2);
    }

    /// test storing and retrieving blocks with zero transactions
    #[test]
    fn test_blocks_with_zero_transactions() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = SledStore::open(temp_dir.path()).unwrap();
        let block = Block {
            header: ledger_core::BlockHeader {
                index: 1,
                previous_hash: [0u8; HASH_SIZE],
                data_hash: [0u8; HASH_SIZE],
                merkle_root: [0u8; HASH_SIZE], // merkle root of empty txs
                timestamp: 0,
                nonce: 0,
            },
            txs: vec![], // zero transactions
            data: None,
        };
        store.put_block(&block).unwrap();
        assert_eq!(store.tip_height().unwrap(), 1);
        assert_eq!(store.tip_hash().unwrap().unwrap(), block.hash());
        let fetched = store.get_block(1).unwrap().unwrap();
        assert_eq!(fetched.header.index, 1);
        assert_eq!(fetched.txs.len(), 0);
    }

    /// test storing and retrieving blocks with maximum number of transactions
    #[test]
    fn test_blocks_with_max_transactions() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = SledStore::open(temp_dir.path()).unwrap();
        let max_txs = 10000; // arbitrary large number for testing
        let mut txs = vec![];
        for i in 0..max_txs {
            let tx = ledger_core::Transaction {
                from: format!("User{}", i),
                to: format!("User{}", i + 1),
                amount: i as u64,
                timestamp: 1_600_000_000 + i as u64,
            };
            txs.push(tx);
        }
        let block = Block {
            header: ledger_core::BlockHeader {
                index: 1,
                previous_hash: [0u8; HASH_SIZE],
                data_hash: [0u8; HASH_SIZE],
                merkle_root: ledger_core::merkle_root(&txs),
                timestamp: 0,
                nonce: 0,
            },
            txs: txs.clone(),
            data: None,
        };
        store.put_block(&block).unwrap();
        assert_eq!(store.tip_height().unwrap(), 1);
        assert_eq!(store.tip_hash().unwrap().unwrap(), block.hash());
        let fetched = store.get_block(1).unwrap().unwrap();
        assert_eq!(fetched.header.index, 1);
        assert_eq!(fetched.txs.len(), max_txs);
        for i in 0..max_txs {
            assert_eq!(fetched.txs[i], txs[i]);
        }
    }

    /// test storing and retrieving blocks with duplicate transactions
    #[test]
    fn test_blocks_with_duplicate_transactions() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = SledStore::open(temp_dir.path()).unwrap();
        let tx = ledger_core::Transaction {
            from: "Alice".to_string(),
            to: "Bob".to_string(),
            amount: 10,
            timestamp: 1_600_000_000,
        };
        let block = Block {
            header: ledger_core::BlockHeader {
                index: 1,
                previous_hash: [0u8; HASH_SIZE],
                data_hash: [0u8; HASH_SIZE],
                merkle_root: ledger_core::merkle_root(&[tx.clone(), tx.clone()]),
                timestamp: 0,
                nonce: 0,
            },
            txs: vec![tx.clone(), tx.clone()], // duplicate transactions
            data: None,
        };
        store.put_block(&block).unwrap();
        assert_eq!(store.tip_height().unwrap(), 1);
        assert_eq!(store.tip_hash().unwrap().unwrap(), block.hash());
        let fetched = store.get_block(1).unwrap().unwrap();
        assert_eq!(fetched.header.index, 1);
        assert_eq!(fetched.txs.len(), 2);
        assert_eq!(fetched.txs[0], tx);
        assert_eq!(fetched.txs[1], tx);
    }

    /// test storing and retrieving blocks with very large indices
    #[test]
    fn test_blocks_with_very_large_indices() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = SledStore::open(temp_dir.path()).unwrap();
        let block1 = Block {
            header: ledger_core::BlockHeader {
                index: u64::MAX - 10,
                previous_hash: [0u8; HASH_SIZE],
                data_hash: [0u8; HASH_SIZE],
                merkle_root: [0u8; HASH_SIZE],
                timestamp: 0,
                nonce: 0,
            },
            txs: vec![],
            data: None,
        };
        let block2 = Block {
            header: ledger_core::BlockHeader {
                index: u64::MAX - 5,
                previous_hash: block1.hash(),
                data_hash: [0u8; HASH_SIZE],
                merkle_root: [1u8; HASH_SIZE],
                timestamp: 0,
                nonce: 0,
            },
            txs: vec![],
            data: None,
        };
        let block3 = Block {
            header: ledger_core::BlockHeader {
                index: u64::MAX,
                previous_hash: block2.hash(),
                data_hash: [0u8; HASH_SIZE],
                merkle_root: [2u8; HASH_SIZE],
                timestamp: 0,
                nonce: 0,
            },
            txs: vec![],
            data: None,
        };
        store.put_block(&block1).unwrap();
        store.put_block(&block2).unwrap();
        store.put_block(&block3).unwrap();
        assert_eq!(store.tip_height().unwrap(), u64::MAX);
        assert_eq!(store.tip_hash().unwrap().unwrap(), block3.hash());
        let fetched1 = store.get_block(u64::MAX - 10).unwrap().unwrap();
        assert_eq!(fetched1.header.index, u64::MAX - 10);
        let fetched2 = store.get_block(u64::MAX - 5).unwrap().unwrap();
        assert_eq!(fetched2.header.index, u64::MAX - 5);
        let fetched3 = store.get_block(u64::MAX).unwrap().unwrap();
        assert_eq!(fetched3.header.index, u64::MAX);
    }

    /// test storing and retrieving blocks with very small indices
    #[test]
    fn test_blocks_with_very_small_indices() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = SledStore::open(temp_dir.path()).unwrap();
        let block0 = Block {
            header: ledger_core::BlockHeader {
                index: 0,
                previous_hash: [0u8; HASH_SIZE],
                data_hash: [0u8; HASH_SIZE],
                merkle_root: [0u8; HASH_SIZE],
                timestamp: 0,
                nonce: 0,
            },
            txs: vec![],
            data: None,
        };
        let block1 = Block {
            header: ledger_core::BlockHeader {
                index: 1,
                previous_hash: block0.hash(),
                data_hash: [0u8; HASH_SIZE],
                merkle_root: [1u8; HASH_SIZE],
                timestamp: 0,
                nonce: 0,
            },
            txs: vec![],
            data: None,
        };
        store.put_block(&block0).unwrap();
        store.put_block(&block1).unwrap();
        assert_eq!(store.tip_height().unwrap(), 1);
        assert_eq!(store.tip_hash().unwrap().unwrap(), block1.hash());
        let fetched0 = store.get_block(0).unwrap().unwrap();
        assert_eq!(fetched0.header.index, 0);
        let fetched1 = store.get_block(1).unwrap().unwrap();
        assert_eq!(fetched1.header.index, 1);
    }

    /// test storing and retrieving blocks with very high frequency of transactions
    #[test]
    fn test_blocks_with_high_frequency_transactions() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = SledStore::open(temp_dir.path()).unwrap();
        let mut txs = vec![];
        for i in 0..1000 {
            let tx = ledger_core::Transaction {
                from: format!("User{}", i),
                to: format!("User{}", i + 1),
                amount: i as u64,
                timestamp: 1_600_000_000 + i as u64,
            };
            txs.push(tx);
        }
        let block = Block {
            header: ledger_core::BlockHeader {
                index: 1,
                previous_hash: [0u8; HASH_SIZE],
                data_hash: [0u8; HASH_SIZE],
                merkle_root: ledger_core::merkle_root(&txs),
                timestamp: 0,
                nonce: 0,
            },
            txs: txs.clone(),
            data: None,
        };
        store.put_block(&block).unwrap();
        assert_eq!(store.tip_height().unwrap(), 1);
        assert_eq!(store.tip_hash().unwrap().unwrap(), block.hash());
        let fetched = store.get_block(1).unwrap().unwrap();
        assert_eq!(fetched.header.index, 1);
        assert_eq!(fetched.txs.len(), 1000);
        for i in 0..1000 {
            assert_eq!(fetched.txs[i], txs[i]);
        }
    }

    /// test storing and retrieving blocks with very large merkle roots
    #[test]
    fn test_blocks_with_large_merkle_roots() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = SledStore::open(temp_dir.path()).unwrap();
        let mut txs = vec![];
        for i in 0..1000 {
            let tx = ledger_core::Transaction {
                from: format!("User{}", i),
                to: format!("User{}", i + 1),
                amount: i as u64,
                timestamp: 1_600_000_000 + i as u64,
            };
            txs.push(tx);
        }
        let merkle_root = ledger_core::merkle_root(&txs);
        let block = Block {
            header: ledger_core::BlockHeader {
                index: 1,
                previous_hash: [0u8; HASH_SIZE],
                data_hash: [0u8; HASH_SIZE],
                merkle_root,
                timestamp: 0,
                nonce: 0,
            },
            txs: txs.clone(),
            data: None,
        };
        store.put_block(&block).unwrap();
        assert_eq!(store.tip_height().unwrap(), 1);
        assert_eq!(store.tip_hash().unwrap().unwrap(), block.hash());
        let fetched = store.get_block(1).unwrap().unwrap();
        assert_eq!(fetched.header.index, 1);
        assert_eq!(fetched.header.merkle_root, merkle_root);
        assert_eq!(fetched.txs.len(), 1000);
        for i in 0..1000 {
            assert_eq!(fetched.txs[i], txs[i]);
        }
    }

    /// test tip height and hash after multiple operations
    #[test]
    fn test_tip_after_multiple_operations() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = SledStore::open(temp_dir.path()).unwrap();
        assert_eq!(store.tip_height().unwrap(), 0);
        assert!(store.tip_hash().unwrap().is_none());
        let block1 = Block {
            header: ledger_core::BlockHeader {
                index: 1,
                previous_hash: [0u8; HASH_SIZE],
                data_hash: [0u8; HASH_SIZE],
                merkle_root: [0u8; HASH_SIZE],
                timestamp: 0,
                nonce: 0,
            },
            txs: vec![],
            data: None,
        };
        store.put_block(&block1).unwrap();
        assert_eq!(store.tip_height().unwrap(), 1);
        assert_eq!(store.tip_hash().unwrap().unwrap(), block1.hash());
        let block2 = Block {
            header: ledger_core::BlockHeader {
                index: 2,
                previous_hash: block1.hash(),
                data_hash: [0u8; HASH_SIZE],
                merkle_root: [1u8; HASH_SIZE],
                timestamp: 0,
                nonce: 0,
            },
            txs: vec![],
            data: None,
        };
        store.put_block(&block2).unwrap();
        dbg!(&store);
        assert_eq!(store.tip_height().unwrap(), 2);
        assert_eq!(store.tip_hash().unwrap().unwrap(), block2.hash());
        // Re-add block1
        store.put_block(&block1).unwrap();
        dbg!(&store);
        // Tip should remain unchanged as re-adding the same block is idempotent
        assert_eq!(store.tip_height().unwrap(), 2);
        assert_eq!(store.tip_hash().unwrap().unwrap(), block2.hash());
    }
}
