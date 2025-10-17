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

impl PartialEq for Transaction {
    fn eq(&self, other: &Self) -> bool {
        self.timestamp == other.timestamp
            && self.amount == other.amount
            && self.from == other.from
            && self.to == other.to
    }
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
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
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
        fn close(&self) -> Result<()>;
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
            if height == 0 && self.store.get_block(0)?.is_none() {
                let genesis = genesis_block();
                self.store.put_block(&genesis)?;
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

#[cfg(test)]
mod tests {
    use std::thread::sleep;

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

    #[test]
    fn mine_block_example() {
        let txs = vec![
            Transaction {
                from: "Alice".to_string(),
                to: "Bob".to_string(),
                amount: 10,
                timestamp: 1_600_000_000,
            },
            Transaction {
                from: "Bob".to_string(),
                to: "Charlie".to_string(),
                amount: 5,
                timestamp: 1_600_000_100,
            },
        ];
        let merkle = merkle_root(&txs);
        let header = BlockHeader::new(1, [0u8; 32], merkle, 0);
        let block = Block { header, txs };
        let mined = pow::mine_block(block, 16); // Require at least 20 leading zero bits
        let hash = mined.hash();
        assert!(pow::count_leading_zero_bits(&hash) >= 16);
    }

    #[test]
    fn merkle_root_example() {
        let txs = vec![
            Transaction {
                from: "Alice".to_string(),
                to: "Bob".to_string(),
                amount: 10,
                timestamp: 1_600_000_000,
            },
            Transaction {
                from: "Bob".to_string(),
                to: "Charlie".to_string(),
                amount: 5,
                timestamp: 1_600_000_100,
            },
            Transaction {
                from: "Charlie".to_string(),
                to: "Dave".to_string(),
                amount: 2,
                timestamp: 1_600_000_200,
            },
        ];
        let root = merkle_root(&txs);
        let expected_hex = "7f1f34ec53937fbf52547ea1bc9ed5f8d7103752dfdb67cb39698a72b28fa04a";
        assert_eq!(hex::encode(root), expected_hex);
    }

    #[test]
    fn genesis_block_example() {
        let genesis = chain::genesis_block();
        assert_eq!(genesis.header.index, 0);
        assert_eq!(genesis.header.previous_hash, [0u8; 32]);
        assert_eq!(genesis.header.merkle_root, [0u8; 32]);
        assert_eq!(genesis.txs.len(), 0);
    }

    #[test]
    fn block_hash_example() {
        let txs = vec![
            Transaction {
                from: "Alice".to_string(),
                to: "Bob".to_string(),
                amount: 10,
                timestamp: 1_600_000_000,
            },
            Transaction {
                from: "Bob".to_string(),
                to: "Charlie".to_string(),
                amount: 5,
                timestamp: 1_600_000_100,
            },
        ];
        let merkle = merkle_root(&txs);
        let header = BlockHeader::new(1, [0u8; 32], merkle, 0);
        let mut block = Block {
            header: header,
            txs,
        };
        block.header.timestamp = 1_600_000_200; // Fix timestamp for test consistency
        let hash = block.hash();
        let expected_hex = "be8f84bd861af54eb5d6bd8d167c322e77f291d6cff94b3a859d3d75c53a925b";
        assert_eq!(hex::encode(hash), expected_hex);
    }

    #[test]
    fn transaction_equality_example() {
        let tx1 = Transaction {
            from: "Alice".to_string(),
            to: "Bob".to_string(),
            amount: 10,
            timestamp: 1_600_000_000,
        };
        let tx2 = Transaction {
            from: "Alice".to_string(),
            to: "Bob".to_string(),
            amount: 10,
            timestamp: 1_600_000_000,
        };
        let tx3 = Transaction {
            from: "Alice".to_string(),
            to: "Charlie".to_string(),
            amount: 10,
            timestamp: 1_600_000_000,
        };
        assert_eq!(tx1, tx2);
        assert_ne!(tx1, tx3);
    }

    #[test]
    fn block_header_hash_bytes_example() {
        let header = BlockHeader::new(1, [0u8; 32], [1u8; 32], 42);
        let bytes = header.hash_bytes();
        assert_eq!(bytes.len(), 88);
        assert_eq!(&bytes[0..8], &1u64.to_le_bytes());
        assert_eq!(&bytes[8..40], &[0u8; 32]);
        assert_eq!(&bytes[40..72], &[1u8; 32]);
        assert_eq!(&bytes[72..80], &header.timestamp.to_le_bytes());
        assert_eq!(&bytes[80..88], &42u64.to_le_bytes());
    }

    #[test]
    fn block_header_new_example() {
        let header = BlockHeader::new(1, [0u8; 32], [1u8; 32], 42);
        assert_eq!(header.index, 1);
        assert_eq!(header.previous_hash, [0u8; 32]);
        assert_eq!(header.merkle_root, [1u8; 32]);
        assert_eq!(header.nonce, 42);
        assert!(header.timestamp > 0);
    }

    #[test]
    fn transaction_serialization_example() {
        let tx = Transaction {
            from: "Alice".to_string(),
            to: "Bob".to_string(),
            amount: 10,
            timestamp: 1_600_000_000,
        };
        let json = serde_json::to_string(&tx).unwrap();
        let expected_json = r#"{"from":"Alice","to":"Bob","amount":10,"timestamp":1600000000}"#;
        assert_eq!(json, expected_json);
        let deserialized: Transaction = serde_json::from_str(&json).unwrap();
        assert_eq!(tx, deserialized);
    }

    #[test]
    fn block_serialization_example() {
        let txs = vec![
            Transaction {
                from: "Alice".to_string(),
                to: "Bob".to_string(),
                amount: 10,
                timestamp: 1_600_000_000,
            },
            Transaction {
                from: "Bob".to_string(),
                to: "Charlie".to_string(),
                amount: 5,
                timestamp: 1_600_000_100,
            },
        ];
        let merkle = merkle_root(&txs);
        let header = BlockHeader::new(1, [0u8; 32], merkle, 0);
        let block = Block { header, txs };
        let json = serde_json::to_string(&block).unwrap();
        let deserialized: Block = serde_json::from_str(&json).unwrap();
        assert_eq!(block.header.index, deserialized.header.index);
        assert_eq!(
            block.header.previous_hash,
            deserialized.header.previous_hash
        );
        assert_eq!(block.header.merkle_root, deserialized.header.merkle_root);
        assert_eq!(block.header.nonce, deserialized.header.nonce);
        assert_eq!(block.txs, deserialized.txs);
    }

    #[test]
    fn merkle_root_empty_txs() {
        let txs: Vec<Transaction> = vec![];
        let root = merkle_root(&txs);
        assert_eq!(root, [0u8; 32]);
    }

    #[test]
    fn merkle_root_single_tx() {
        let txs = vec![Transaction {
            from: "Alice".to_string(),
            to: "Bob".to_string(),
            amount: 10,
            timestamp: 1_600_000_000,
        }];
        let root = merkle_root(&txs);
        let mut hasher = Sha256::new();
        hasher.update(serde_json::to_vec(&txs[0]).unwrap());
        let digest = hasher.finalize();
        let mut expected = [0u8; 32];
        expected.copy_from_slice(&digest[..]);
        assert_eq!(root, expected);
    }

    #[test]
    fn merkle_root_two_txs() {
        let txs = vec![
            Transaction {
                from: "Alice".to_string(),
                to: "Bob".to_string(),
                amount: 10,
                timestamp: 1_600_000_000,
            },
            Transaction {
                from: "Bob".to_string(),
                to: "Charlie".to_string(),
                amount: 5,
                timestamp: 1_600_000_100,
            },
        ];
        let root = merkle_root(&txs);
        let mut hasher1 = Sha256::new();
        hasher1.update(serde_json::to_vec(&txs[0]).unwrap());
        let digest1 = hasher1.finalize();
        let mut h1 = [0u8; 32];
        h1.copy_from_slice(&digest1[..]);
        let mut hasher2 = Sha256::new();
        hasher2.update(serde_json::to_vec(&txs[1]).unwrap());
        let digest2 = hasher2.finalize();
        let mut h2 = [0u8; 32];
        h2.copy_from_slice(&digest2[..]);
        let mut hasher_root = Sha256::new();
        hasher_root.update(h1);
        hasher_root.update(h2);
        let digest_root = hasher_root.finalize();
        let mut expected = [0u8; 32];
        expected.copy_from_slice(&digest_root[..]);
        assert_eq!(root, expected);
    }

    #[test]
    fn merkle_root_three_txs() {
        let txs = vec![
            Transaction {
                from: "Alice".to_string(),
                to: "Bob".to_string(),
                amount: 10,
                timestamp: 1_600_000_000,
            },
            Transaction {
                from: "Bob".to_string(),
                to: "Charlie".to_string(),
                amount: 5,
                timestamp: 1_600_000_100,
            },
            Transaction {
                from: "Charlie".to_string(),
                to: "Dave".to_string(),
                amount: 2,
                timestamp: 1_600_000_200,
            },
        ];
        let root = merkle_root(&txs);
        let expected_hex = "7f1f34ec53937fbf52547ea1bc9ed5f8d7103752dfdb67cb39698a72b28fa04a";
        assert_eq!(hex::encode(root), expected_hex);
    }

    #[test]
    fn merkle_root_one_thousand_txs() {
        let mut txs = Vec::new();
        for i in 0..1000 {
            txs.push(Transaction {
                from: format!("User{}", i),
                to: format!("User{}", i + 1),
                amount: i as u64,
                timestamp: 1_600_000_000 + i as u64 * 100,
            });
        }
        let root = merkle_root(&txs);
        let expected_hex = "76953d5e2af4062dc5ab092d962f6a6da17f2bfe95c6d8e92b28b747e9253cf8";
        assert_eq!(hex::encode(root), expected_hex);
        // Just check that we get a non-zero root for a large number of transactions.
        assert_ne!(root, [0u8; 32]);
    }

    #[test]
    fn block_hash_consistency() {
        let txs = vec![
            Transaction {
                from: "Alice".to_string(),
                to: "Bob".to_string(),
                amount: 10,
                timestamp: 1_600_000_000,
            },
            Transaction {
                from: "Bob".to_string(),
                to: "Charlie".to_string(),
                amount: 5,
                timestamp: 1_600_000_100,
            },
        ];
        let merkle = merkle_root(&txs);
        let header = BlockHeader::new(1, [0u8; 32], merkle, 0);
        let mut block = Block {
            header: header,
            txs,
        };
        block.header.timestamp = 1_600_000_200; // Fix timestamp for test consistency
        let hash1 = block.hash();
        let hash2 = block.hash();
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn block_equality() {
        // Two blocks with identical headers and transactions should have the same hash.
        // ITRW, they are not `Eq` because timestamps would differ.
        // Test passes because we only timestanp to nearest second, so they are equal here.
        let txs1 = vec![
            Transaction {
                from: "Alice".to_string(),
                to: "Bob".to_string(),
                amount: 10,
                timestamp: 1_600_000_000,
            },
            Transaction {
                from: "Bob".to_string(),
                to: "Charlie".to_string(),
                amount: 5,
                timestamp: 1_600_000_100,
            },
        ];
        let merkle1 = merkle_root(&txs1);
        let header1 = BlockHeader::new(1, [0u8; 32], merkle1, 0);
        let block1 = Block {
            header: header1,
            txs: txs1.clone(),
        };
        let merkle2 = merkle_root(&txs1);
        let header2 = BlockHeader::new(1, [0u8; 32], merkle2, 0);
        let block2 = Block {
            header: header2,
            txs: txs1,
        };
        assert_eq!(block1.hash(), block2.hash());
    }

    #[test]
    fn block_inequality() {
        // Two blocks with identical headers and transactions should have the same hash.
        // But if timestamps differ, hashes should differ.
        let txs1 = vec![
            Transaction {
                from: "Alice".to_string(),
                to: "Bob".to_string(),
                amount: 10,
                timestamp: 1_600_000_000,
            },
            Transaction {
                from: "Bob".to_string(),
                to: "Charlie".to_string(),
                amount: 5,
                timestamp: 1_600_000_100,
            },
        ];
        let merkle1 = merkle_root(&txs1);
        let header1 = BlockHeader::new(1, [0u8; 32], merkle1, 0);
        let block1 = Block {
            header: header1,
            txs: txs1.clone(),
        };
        sleep(std::time::Duration::from_millis(1000)); // Ensure timestamp would differ
        let merkle2 = merkle_root(&txs1);
        let header2 = BlockHeader::new(1, [0u8; 32], merkle2, 0);
        let block2 = Block {
            header: header2,
            txs: txs1,
        };
        assert_ne!(block1.hash(), block2.hash());
    }

    #[test]
    fn transaction_inequality() {
        let tx1 = Transaction {
            from: "Alice".to_string(),
            to: "Bob".to_string(),
            amount: 10,
            timestamp: 1_600_000_000,
        };
        let tx2 = Transaction {
            from: "Alice".to_string(),
            to: "Bob".to_string(),
            amount: 10,
            timestamp: 1_600_000_001,
        };
        assert_ne!(tx1, tx2);
    }

    #[test]
    fn transaction_inequality_different_amount() {
        let tx1 = Transaction {
            from: "Alice".to_string(),
            to: "Bob".to_string(),
            amount: 10,
            timestamp: 1_600_000_000,
        };
        let tx2 = Transaction {
            from: "Alice".to_string(),
            to: "Bob".to_string(),
            amount: 20,
            timestamp: 1_600_000_000,
        };
        assert_ne!(tx1, tx2);
    }

    #[test]
    fn transaction_inequality_different_recipient() {
        let tx1 = Transaction {
            from: "Alice".to_string(),
            to: "Bob".to_string(),
            amount: 10,
            timestamp: 1_600_000_000,
        };
        let tx2 = Transaction {
            from: "Alice".to_string(),
            to: "Charlie".to_string(),
            amount: 10,
            timestamp: 1_600_000_000,
        };
        assert_ne!(tx1, tx2);
    }

    #[test]
    fn transaction_inequality_different_sender() {
        let tx1 = Transaction {
            from: "Alice".to_string(),
            to: "Bob".to_string(),
            amount: 10,
            timestamp: 1_600_000_000,
        };
        let tx2 = Transaction {
            from: "Eve".to_string(),
            to: "Bob".to_string(),
            amount: 10,
            timestamp: 1_600_000_000,
        };
        assert_ne!(tx1, tx2);
    }

    #[test]
    fn block_hash_changes_with_nonce() {
        let txs = vec![
            Transaction {
                from: "Alice".to_string(),
                to: "Bob".to_string(),
                amount: 10,
                timestamp: 1_600_000_000,
            },
            Transaction {
                from: "Bob".to_string(),
                to: "Charlie".to_string(),
                amount: 5,
                timestamp: 1_600_000_100,
            },
        ];
        let merkle = merkle_root(&txs);
        let header = BlockHeader::new(1, [0u8; 32], merkle, 0);
        let mut block = Block {
            header: header,
            txs,
        };
        block.header.timestamp = 1_600_000_200; // Fix timestamp for test consistency
        let hash1 = block.hash();
        block.header.nonce += 1;
        let hash2 = block.hash();
        assert_ne!(hash1, hash2);
    }
}
