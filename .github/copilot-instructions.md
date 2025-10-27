Copilot Instructions for Ledger Project

- Purpose
  - Help generate surgical Rust changes that satisfy project conventions and pass tests.

- Core commands to validate changes
  - Build: `cargo build --all-targets`
  - Test (all): `cargo test --all`
  - Test (single): `cargo test <name> -- --nocapture`
  - Lint: `cargo clippy --all-targets --all-features -- -D warnings`
  - Format: `cargo fmt --all` and `cargo fmt --all -- --check`
  - Workspace tests: `cargo test --workspace`

- Coding standards to follow
  - Align with AGENTS.md: imports, formatting, naming, error handling, docs
  - Use snake_case for functions/vars/modules; CamelCase for types
  - Prefer `Result<T, E>` with `?`; avoid `unwrap` in libs
  - Add doc comments for public APIs; crate docs with `//!`
  - Tests: use `#[cfg(test)]` modules and `#[test]` with assertions
  - Favor iterators and minimal allocations; avoid unnecessary clones

- Edits and review guidance
  - Make surgical edits; avoid broad refactors unless needed
  - Add/adjust tests to cover new behavior
  - Run `cargo test --workspace` before requesting review

- Safety and workflow
  - When in doubt, ask for confirmation before sweeping changes
  - Do not modify AGENTS.md or critical config without explicit request