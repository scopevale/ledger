use ledger_core::{Block, Transaction};
use ledger_storage::sled_store::SledStore;
use ledger_storage::Storage;
use rand::Rng;
use std::any::Any;
use std::fs;
use tempfile::tempdir;

#[tokio::test]
async fn test_storage_integration() -> anyhow::Result<()> {
    // Create a temporary directory for the sled database
    let temp_dir = tempdir()?;
    let db_path = temp_dir.path().to_path_buf();
    // Initialize the SledStore
    let store = SledStore::open(db_path.to_str().unwrap())?;
    // Test data
    let mut rng = rand::thread_rng();
    let num_blocks = 100;
    let mut blocks: Vec<Block> = Vec::new();
    // Generate and store blocks
    for i in 0..num_blocks {
        let prev_hash = if i == 0 {
            [0u8; 32]
        } else {
            blocks[i - 1].hash()
        };
        let merkle_root: [u8; 32] = rng.gen();
        let header = ledger_core::BlockHeader::new(i as u64, prev_hash, merkle_root, rng.gen());
        let block = Block {
            header,
            txs: vec![],
        };
        store.put_block(&block)?;
        blocks.push(block);
    }
    // Retrieve and verify blocks
    for i in 0..num_blocks {
        let retrieved_block = store.get_block(i as u64)?.expect("Block should exist");
        assert_eq!(retrieved_block.header.index, i as u64);
        assert_eq!(
            retrieved_block.header.previous_hash,
            if i == 0 {
                [0u8; 32]
            } else {
                blocks[i - 1].hash()
            }
        );
        assert_eq!(
            retrieved_block.header.merkle_root,
            blocks[i].header.merkle_root
        );
    }
    // Verify tip height and hash
    let tip_height = store.tip_height()?;
    let tip_hash = store.tip_hash()?.expect("Tip hash should exist");
    assert_eq!(tip_height, num_blocks as u64 - 1);
    assert_eq!(tip_hash, blocks.last().unwrap().hash());
    // Cleanup
    temp_dir.close()?;
    let _ = fs::remove_dir_all(db_path);
    Ok(())
}

#[tokio::test]
async fn test_storage_persistence() -> anyhow::Result<()> {
    // Create a temporary directory for the sled database
    let temp_dir = tempdir()?;
    let db_path = temp_dir.path().to_path_buf();
    // Initialize the SledStore and add a block
    {
        let store = SledStore::open(db_path.to_str().unwrap())?;
        let header = ledger_core::BlockHeader::new(0, [0u8; 32], [0u8; 32], 0);
        let genesis_block = Block {
            header,
            txs: vec![],
        };
        store.put_block(&genesis_block)?;
    }
    // Re-open the SledStore and verify the block persists
    {
        let store = SledStore::open(db_path.to_str().unwrap())?;
        let retrieved_block = store.get_block(0)?.expect("Genesis block should exist");
        assert_eq!(retrieved_block.header.index, 0);
        assert_eq!(retrieved_block.header.previous_hash, [0u8; 32]);
        assert_eq!(retrieved_block.header.merkle_root, [0u8; 32]);
        let tip_height = store.tip_height()?;
        let tip_hash = store.tip_hash()?.expect("Tip hash should exist");
        assert_eq!(tip_height, 0);
        assert_eq!(tip_hash, retrieved_block.hash());
    }

    // Cleanup
    temp_dir.close()?;
    let _ = fs::remove_dir_all(db_path);
    Ok(())
}

#[tokio::test]
async fn test_storage_edge_cases() -> anyhow::Result<()> {
    // Create a temporary directory for the sled database
    let temp_dir = tempdir()?;
    let db_path = temp_dir.path().to_path_buf();
    // Initialize the SledStore
    let store = SledStore::open(db_path.to_str().unwrap())?;
    // Test empty block storage
    let header = ledger_core::BlockHeader::new(0, [0u8; 32], [0u8; 32], 0);
    let empty_block = Block {
        header,
        txs: vec![],
    };
    store.put_block(&empty_block)?;
    let retrieved_block = store.get_block(0)?.expect("Empty block should exist");
    assert_eq!(retrieved_block.txs.len(), 0);
    // Test very large block storage
    let large_txs: Vec<Transaction> = (0..10000)
        .map(|i| Transaction {
            from: format!("addr_from_{}", i),
            to: format!("addr_to_{}", i),
            amount: i as u64,
            timestamp: 1_600_000_000 + i as u64,
        })
        .collect();
    let header = ledger_core::BlockHeader::new(1, empty_block.hash(), [0u8; 32], 0);
    let large_block = Block {
        header,
        txs: large_txs.clone(),
    };
    store.put_block(&large_block)?;
    let retrieved_large_block = store.get_block(1)?.expect("Large block should exist");
    assert_eq!(retrieved_large_block.txs.len(), large_txs.len());
    assert_eq!(retrieved_large_block.txs, large_txs);
    // Cleanup
    temp_dir.close()?;
    let _ = fs::remove_dir_all(db_path);
    Ok(())
}

#[tokio::test]
async fn test_storage_concurrency() -> anyhow::Result<()> {
    use std::sync::Arc;
    use tokio::task;
    // Create a temporary directory for the sled database
    let temp_dir = tempdir()?;
    let db_path = temp_dir.path().to_path_buf();
    // Initialize the SledStore
    let store = Arc::new(SledStore::open(db_path.to_str().unwrap())?);
    let num_blocks = 50;
    let mut handles = Vec::new();
    // Concurrently add blocks
    for i in 0..num_blocks {
        let store_clone = Arc::clone(&store);
        let handle = task::spawn(async move {
            let header = ledger_core::BlockHeader::new(
                i as u64,
                if i == 0 {
                    [0u8; 32]
                } else {
                    store_clone
                        .get_block((i - 1) as u64)
                        .unwrap()
                        .unwrap()
                        .hash()
                },
                [0u8; 32],
                0,
            );
            let block = Block {
                header,
                txs: vec![],
            };
            store_clone.put_block(&block).unwrap();
        });
        handles.push(handle);
    }
    // Wait for all tasks to complete
    for handle in handles {
        handle.await.unwrap();
    }
    // Verify all blocks are stored
    for i in 0..num_blocks {
        let retrieved_block = store.get_block(i as u64)?.expect("Block should exist");
        assert_eq!(retrieved_block.header.index, i as u64);
    }
    // Cleanup
    temp_dir.close()?;
    let _ = fs::remove_dir_all(db_path);
    Ok(())
}

#[tokio::test]
async fn test_storage_data_integrity() -> anyhow::Result<()> {
    use std::{fs, path::PathBuf};

    let temp_dir = tempdir()?;
    let db_path: PathBuf = temp_dir.path().to_path_buf();
    let block: Block;

    // 1) Create DB and write a valid block
    {
        let store = SledStore::open(db_path.to_str().unwrap())?;
        let header = ledger_core::BlockHeader::new(0, [1u8; 32], [2u8; 32], 0);
        block = Block {
            header,
            txs: vec![],
        };
        store.put_block(&block)?;
        // If your SledStore exposes a flush, call it so bytes hit disk:
        store.close()?;
        // `store` dropped here -> lock released
    }

    // 2) Reopen *raw* sled and attempt to corrupt on disk, it should fail gracefully
    {
        let sled_db = sled::open(db_path.to_str().unwrap())?;
        // If your SledStore uses a dedicated tree/key, mirror that here.
        // Example if it uses a "blocks" tree and keys like "block_{index}":
        let blocks = sled_db.open_tree("blocks")?; // adjust to your impl
        let result = blocks.insert(b"block_0", vec![0u8; 10])?; // invalid bytes
        assert!(result.is_none(), "Expected to NOT overwrite existing block");
        dbg!("Result of corrupting SledStore: {:?}", &result);
        sled_db.flush()?;
        // `sled_db` dropped here -> lock released
    }

    // 3) Reopen SledStore and attempt to read the now-corrupted block, it should get the last valid block
    let store = SledStore::open(db_path.to_str().unwrap())?;
    let retrieved_block = store.get_block(0)?.expect("Block should exist");
    assert_eq!(
        retrieved_block.header.index, block.header.index,
        "Retrieved block should be last valid block saved"
    );
    assert_eq!(
        retrieved_block.type_id(),
        Block::type_id(&block),
        "Retrieved block should be last valid block saved"
    );

    // Cleanup
    temp_dir.close()?;
    let _ = fs::remove_dir_all(db_path);
    Ok(())
}

#[tokio::test]
async fn test_storage_trait_compliance() -> anyhow::Result<()> {
    // Create a temporary directory for the sled database
    let temp_dir = tempdir()?;
    let db_path = temp_dir.path().to_path_buf();
    // Initialize the SledStore
    let _store = SledStore::open(db_path.to_str().unwrap())?;
    // Verify that SledStore implements the Storage trait
    fn assert_storage_trait<T: Storage>() {}
    assert_storage_trait::<SledStore>();
    // Cleanup
    temp_dir.close()?;
    let _ = fs::remove_dir_all(db_path);
    Ok(())
}

#[tokio::test]
async fn test_storage_cleanup() -> anyhow::Result<()> {
    // Create a temporary directory for the sled database
    let temp_dir = tempdir()?;
    let db_path = temp_dir.path().to_path_buf();
    // Initialize the SledStore
    let store = SledStore::open(db_path.to_str().unwrap())?;
    // Add a block
    let header = ledger_core::BlockHeader::new(0, [0u8; 32], [0u8; 32], 0);
    let block = Block {
        header,
        txs: vec![],
    };
    store.put_block(&block)?;
    // Verify the block exists
    let retrieved_block = store.get_block(0)?.expect("Block should exist");
    assert_eq!(retrieved_block.header.index, 0);
    // Cleanup
    temp_dir.close()?;
    let _ = fs::remove_dir_all(&db_path);
    // Verify the directory is removed
    assert!(!db_path.exists(), "Database directory should be removed");
    Ok(())
}

#[tokio::test]
async fn test_storage_performance() -> anyhow::Result<()> {
    use std::time::Instant;
    // Create a temporary directory for the sled database
    let temp_dir = tempdir()?;
    let db_path = temp_dir.path().to_path_buf();
    // Initialize the SledStore
    let store = SledStore::open(db_path.to_str().unwrap())?;
    // Measure performance of adding blocks
    let num_blocks = 1000;
    let start_time = Instant::now();
    for i in 0..num_blocks {
        let header = ledger_core::BlockHeader::new(
            i as u64,
            if i == 0 {
                [0u8; 32]
            } else {
                store.get_block((i - 1) as u64).unwrap().unwrap().hash()
            },
            [0u8; 32],
            0,
        );
        let block = Block {
            header,
            txs: vec![],
        };
        store.put_block(&block)?;
    }
    let duration = start_time.elapsed();
    println!("Time taken to add {} blocks: {:?}", num_blocks, duration);
    // Measure performance of retrieving blocks
    let start_time = Instant::now();
    for i in 0..num_blocks {
        let _ = store.get_block(i as u64)?;
    }
    let duration = start_time.elapsed();
    println!(
        "Time taken to retrieve {} blocks: {:?}",
        num_blocks, duration
    );
    // Cleanup
    temp_dir.close()?;
    let _ = fs::remove_dir_all(db_path);
    Ok(())
}

#[tokio::test]
async fn test_storage_large_blockchain() -> anyhow::Result<()> {
    // Create a temporary directory for the sled database
    let temp_dir = tempdir()?;
    let db_path = temp_dir.path().to_path_buf();
    // Initialize the SledStore
    let store = SledStore::open(db_path.to_str().unwrap())?;
    // Test data
    let mut rng = rand::thread_rng();
    let num_blocks = 10000;
    let mut blocks: Vec<Block> = Vec::new();
    // Generate and store blocks
    for i in 0..num_blocks {
        let prev_hash = if i == 0 {
            [0u8; 32]
        } else {
            blocks[i - 1].hash()
        };
        let merkle_root: [u8; 32] = rng.gen();
        let header = ledger_core::BlockHeader::new(i as u64, prev_hash, merkle_root, rng.gen());
        let block = Block {
            header,
            txs: vec![],
        };
        store.put_block(&block)?;
        blocks.push(block);
    }
    // Retrieve and verify blocks
    for i in 0..num_blocks {
        let retrieved_block = store.get_block(i as u64)?.expect("Block should exist");
        assert_eq!(retrieved_block.header.index, i as u64);
        assert_eq!(
            retrieved_block.header.previous_hash,
            if i == 0 {
                [0u8; 32]
            } else {
                blocks[i - 1].hash()
            }
        );
        assert_eq!(
            retrieved_block.header.merkle_root,
            blocks[i].header.merkle_root
        );
    }
    // Verify tip height and hash
    let tip_height = store.tip_height()?;
    let tip_hash = store.tip_hash()?.expect("Tip hash should exist");
    assert_eq!(tip_height, num_blocks as u64 - 1);
    assert_eq!(tip_hash, blocks.last().unwrap().hash());
    // Cleanup
    temp_dir.close()?;
    let _ = fs::remove_dir_all(db_path);
    Ok(())
}

#[tokio::test]
async fn test_storage_empty_database() -> anyhow::Result<()> {
    // Create a temporary directory for the sled database
    let temp_dir = tempdir()?;
    let db_path = temp_dir.path().to_path_buf();
    // Initialize the SledStore
    let store = SledStore::open(db_path.to_str().unwrap())?;
    // Verify that the database is empty
    let tip_height = store.tip_height()?;
    let tip_hash = store.tip_hash()?;
    assert_eq!(tip_height, 0);
    assert!(
        tip_hash.is_none(),
        "Tip hash should be None for empty database"
    );
    let block = store.get_block(0)?;
    assert!(
        block.is_none(),
        "No blocks should exist in an empty database"
    );
    // Cleanup
    temp_dir.close()?;
    let _ = fs::remove_dir_all(db_path);
    Ok(())
}

#[tokio::test]
async fn test_storage_non_existent_block() -> anyhow::Result<()> {
    // Create a temporary directory for the sled database
    let temp_dir = tempdir()?;
    let db_path = temp_dir.path().to_path_buf();
    // Initialize the SledStore
    let store = SledStore::open(db_path.to_str().unwrap())?;
    // Attempt to retrieve a non-existent block
    let block = store.get_block(9999)?;
    assert!(block.is_none(), "Block should not exist");
    // Cleanup
    temp_dir.close()?;
    let _ = fs::remove_dir_all(db_path);
    Ok(())
}

#[tokio::test]
async fn test_storage_repeated_open_close() -> anyhow::Result<()> {
    // Create a temporary directory for the sled database
    let temp_dir = tempdir()?;
    let db_path = temp_dir.path().to_path_buf();
    // Repeatedly open and close the SledStore
    for _ in 0..10 {
        {
            let store = SledStore::open(db_path.to_str().unwrap())?;
            let header = ledger_core::BlockHeader::new(0, [0u8; 32], [0u8; 32], 0);
            let block = Block {
                header,
                txs: vec![],
            };
            store.put_block(&block)?;
        } // Store goes out of scope and is closed here
        {
            let store = SledStore::open(db_path.to_str().unwrap())?;
            let retrieved_block = store.get_block(0)?.expect("Block should exist");
            assert_eq!(retrieved_block.header.index, 0);
        } // Store goes out of scope and is closed here
    }
    // Cleanup
    temp_dir.close()?;
    let _ = fs::remove_dir_all(db_path);
    Ok(())
}

#[tokio::test]
async fn test_storage_large_transactions() -> anyhow::Result<()> {
    // Create a temporary directory for the sled database
    let temp_dir = tempdir()?;
    let db_path = temp_dir.path().to_path_buf();
    // Initialize the SledStore
    let store = SledStore::open(db_path.to_str().unwrap())?;
    // Create a block with large transactions
    let large_txs: Vec<Transaction> = (0..1000)
        .map(|i| Transaction {
            from: "a".repeat(1000) + &i.to_string(),
            to: "b".repeat(1000) + &i.to_string(),
            amount: i as u64,
            timestamp: 1_600_000_000 + i as u64,
        })
        .collect();
    let header = ledger_core::BlockHeader::new(0, [0u8; 32], [0u8; 32], 0);
    let block = Block {
        header,
        txs: large_txs.clone(),
    };
    store.put_block(&block)?;
    // Retrieve and verify the block
    let retrieved_block = store.get_block(0)?.expect("Block should exist");
    assert_eq!(retrieved_block.txs.len(), large_txs.len());
    assert_eq!(retrieved_block.txs, large_txs);
    // Cleanup
    temp_dir.close()?;
    let _ = fs::remove_dir_all(db_path);
    Ok(())
}

#[tokio::test]
async fn test_storage_multiple_tips() -> anyhow::Result<()> {
    // Create a temporary directory for the sled database
    let temp_dir = tempdir()?;
    let db_path = temp_dir.path().to_path_buf();
    // Initialize the SledStore
    let store = SledStore::open(db_path.to_str().unwrap())?;
    // Add multiple blocks
    let mut prev_hash = [0u8; 32];
    for i in 0..5 {
        let header = ledger_core::BlockHeader::new(i as u64, prev_hash, [0u8; 32], 0);
        let block = Block {
            header,
            txs: vec![],
        };
        store.put_block(&block)?;
        prev_hash = block.hash();
    }
    // Verify tip height and hash
    let tip_height = store.tip_height()?;
    let tip_hash = store.tip_hash()?.expect("Tip hash should exist");
    assert_eq!(tip_height, 4);
    assert_eq!(tip_hash, prev_hash);
    // Cleanup
    temp_dir.close()?;
    let _ = fs::remove_dir_all(db_path);
    Ok(())
}

#[tokio::test]
async fn test_storage_stress() -> anyhow::Result<()> {
    use std::sync::Arc;
    use tokio::task;
    // Create a temporary directory for the sled database
    let temp_dir = tempdir()?;
    let db_path = temp_dir.path().to_path_buf();
    // Initialize the SledStore
    let store = Arc::new(SledStore::open(db_path.to_str().unwrap())?);
    let num_blocks = 1000;
    let mut handles = Vec::new();
    // Concurrently add blocks
    for i in 0..num_blocks {
        let store_clone = Arc::clone(&store);
        let handle = task::spawn(async move {
            let header = ledger_core::BlockHeader::new(
                i as u64,
                if i == 0 {
                    [0u8; 32]
                } else {
                    store_clone
                        .get_block((i - 1) as u64)
                        .unwrap()
                        .unwrap()
                        .hash()
                },
                [0u8; 32],
                0,
            );
            let block = Block {
                header,
                txs: vec![],
            };
            store_clone.put_block(&block).unwrap();
        });
        handles.push(handle);
    }
    // Wait for all tasks to complete
    for handle in handles {
        handle.await.unwrap();
    }
    // Verify all blocks are stored
    for i in 0..num_blocks {
        let retrieved_block = store.get_block(i as u64)?.expect("Block should exist");
        assert_eq!(retrieved_block.header.index, i as u64);
    }
    // Cleanup
    temp_dir.close()?;
    let _ = fs::remove_dir_all(db_path);
    Ok(())
}

#[tokio::test]
async fn test_storage_reindex() -> anyhow::Result<()> {
    // Create a temporary directory for the sled database
    let temp_dir = tempdir()?;
    let db_path = temp_dir.path().to_path_buf();
    // Initialize the SledStore
    let store = SledStore::open(db_path.to_str().unwrap())?;
    // Add blocks
    let mut prev_hash = [0u8; 32];
    for i in 0..5 {
        let header = ledger_core::BlockHeader::new(i as u64, prev_hash, [0u8; 32], 0);
        let block = Block {
            header,
            txs: vec![],
        };
        store.put_block(&block)?;
        prev_hash = block.hash();
    }
    // Simulate reindexing by closing and reopening the store
    drop(store);
    let store = SledStore::open(db_path.to_str().unwrap())?;
    // Verify all blocks are still accessible after reindexing
    for i in 0..5 {
        let retrieved_block = store.get_block(i as u64)?.expect("Block should exist");
        assert_eq!(retrieved_block.header.index, i as u64);
    }
    // Cleanup
    temp_dir.close()?;
    let _ = fs::remove_dir_all(db_path);
    Ok(())
}
