# Testing Strategy for Blockchain Ledger Project

## Current Test Coverage Analysis

The `ledger-core/src/lib.rs:194-297` already has good basic unit tests covering:
- PoW leading zero bit counting
- Block mining functionality
- Merkle root calculation
- Genesis block creation
- Block hashing

## Suggested Test Additions

### 1. Core Library Tests (`ledger-core`)

**Add to existing test module:**
- **Transaction validation**: Invalid amounts, empty strings, timestamp bounds
- **BlockHeader edge cases**: Overflow conditions, boundary values for nonce/index
- **Merkle tree edge cases**: Single transaction, odd number of transactions, large transaction sets
- **PoW robustness**: Different target difficulties, nonce overflow handling
- **Serialization/deserialization**: Round-trip tests for all structs

**New integration test file** (`tests/chain_integration.rs`):
```rust
// Chain operations with mock storage
// Genesis creation and retrieval
// Block sequence validation
// Tip tracking accuracy
```

### 2. Storage Layer Tests (`ledger-storage`)

**Unit tests** (`src/sled_store.rs`):
- Database open/close operations
- Block storage and retrieval consistency
- Tip height/hash synchronization
- Error handling for corrupted data
- Concurrent access patterns

**Integration tests** (`tests/storage_integration.rs`):
- Large blockchain storage/retrieval
- Database persistence across restarts
- Storage trait compliance verification

### 3. Node API Tests (`ledger-node`)

**Unit tests** (`src/main.rs` - extract handlers to lib.rs):
- HTTP endpoint responses
- Transaction payload validation
- Chain state API accuracy
- Error response formatting

**Integration tests** (`tests/api_integration.rs`):
- Full HTTP server lifecycle
- End-to-end transaction submission
- API error scenarios
- Concurrent request handling

### 4. CLI Tests (`ledger-cli`)

**Integration tests** (`tests/cli_integration.rs`):
- Command parsing validation
- HTTP client error handling
- Output formatting verification
- Network connectivity scenarios

### 5. Cross-Crate Integration Tests

**Workspace-level tests** (`tests/` in root):
- Full system integration (CLI → Node → Storage → Core)
- Multi-node scenarios (if applicable)
- Performance regression tests
- Data consistency across restarts

### 6. Property-Based Testing

Using `proptest` crate:
- Transaction generation with random valid inputs
- Block mining with varying difficulties
- Merkle tree properties with arbitrary transaction sets
- Storage invariants under random operations

### 7. Performance/Benchmark Tests

Expand `benches/pow.rs`:
- Different block sizes and transaction counts
- Storage operation benchmarks
- API endpoint performance
- Memory usage profiling

## Testing Infrastructure Recommendations

1. **Add test utilities crate**: `ledger-test-utils` for shared test data generation
2. **Mock implementations**: In-memory storage for faster unit tests
3. **Test containers**: For integration testing with real databases
4. **CI/CD integration**: Automated testing across different Rust versions

## Priority Implementation Order

1. **High Priority**: Storage layer tests (data integrity critical)
2. **Medium Priority**: API integration tests (user-facing functionality)
3. **Low Priority**: Property-based and performance tests (quality improvements)

## Test Commands

```bash
# Run all tests
cargo test

# Run tests for specific crate
cargo test -p ledger-core
cargo test -p ledger-storage
cargo test -p ledger-node
cargo test -p ledger-cli

# Run integration tests
cargo test --test integration

# Run benchmarks
cargo bench

# Run with coverage (requires cargo-llvm-cov)
cargo llvm-cov --html
```

## Notes

The project shows good architectural separation with a solid foundation. Focus on storage tests first since data integrity is critical for a blockchain system, then build out API and integration tests to ensure the system works reliably end-to-end.