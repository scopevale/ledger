use std::fs;

use ledger_storage::sled_store::SledStore;
use tempfile::{tempdir, TempDir};

pub fn create_temp_dir() -> (TempDir, std::path::PathBuf) {
    // Create a temporary directory for the sled database
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().to_path_buf();
    (temp_dir, db_path)
}

pub fn remove_temp_dir(temp_dir: TempDir) {
    let db_path = temp_dir.path().to_path_buf();
    temp_dir.close().expect("Failed to delete temp dir");
    let _ = fs::remove_dir_all(&db_path);
    // Verify the directory is removed
    assert!(!db_path.exists(), "Database directory should be removed");
}

pub fn create_temp_store() -> (TempDir, SledStore) {
    // Create a temporary directory for the sled database
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().to_path_buf();
    // Initialize the SledStore
    (
        temp_dir,
        SledStore::open(db_path.to_str().unwrap()).expect("Failed to open SledStore"),
    )
}

pub fn clear_store(store: &SledStore) {
    store.clear().expect("Failed to clear the store");
}

pub fn teardown_store(temp_dir: tempfile::TempDir, store: SledStore) {
    let db_path = temp_dir.path().to_path_buf();
    clear_store(&store);
    temp_dir.close().expect("Failed to delete temp dir");
    let _ = fs::remove_dir_all(&db_path);
    // Verify the directory is removed
    assert!(!db_path.exists(), "Database directory should be removed");
    drop(store);
}
