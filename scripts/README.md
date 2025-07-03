# Utility Scripts

This directory contains utility scripts for maintaining and monitoring the rust-docs-mcp-server database.

## Scripts

- `populate_missing.sh` - Populates specific missing crates from proxy-config.json
- `populate_individual.sh` - Populates crates individually with timeout handling
- `populate_monitor.sh` - Monitors the batch population process
- `backfill_underdocumented.sh` - Re-populates crates with few documents
- `status.sh` - Shows current database status and statistics

## Usage

Most scripts are meant for one-time or maintenance use. For regular operations, use the Rust binaries:

```bash
# Populate a single crate
cargo run --bin populate_db -- --crate-name tokio --max-pages 50

# Populate all crates from proxy-config.json
cargo run --bin populate_all

# List all crates in database
cargo run --bin populate_db -- --list
```