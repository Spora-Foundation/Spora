#!/usr/bin/env bash
set -Eeuo pipefail

# 1. Define snapshot directory
SNAPSHOT_ROOT="testing/integration/testdata/dags_for_json_tests"
SNAPSHOT_SETS=("goref_custom_pruning_depth" "goref-notx-5000-blocks" "goref-905-tx-265-blocks")

# 2. Clear all snapshot subdirectories
echo "Clearing old snapshot files..."
for set in "${SNAPSHOT_SETS[@]}"; do
    dir="$SNAPSHOT_ROOT/$set"
    if [ -d "$dir" ]; then
        # Clean RocksDB database files
        rm -f "$dir"/*.sst "$dir"/*.log "$dir"/CURRENT "$dir"/MANIFEST* "$dir"/OPTIONS* "$dir"/LOCK
        rm -rf "$dir"
        echo "Deleted $dir"
    fi
done

# 3. Clean other caches
echo "Cleaning caches..."
rm -rf target/debug/simpa
rm -rf target/debug/deps/simpa*
rm -rf target/debug/.fingerprint/simpa*
rm -rf target/debug/build/simpa-*

# 4. Regenerate snapshots
echo "Starting to generate new snapshots..."

# goref_custom_pruning_depth - increase block count and transaction count, add delay
cargo run --locked --bin simpa -- \
    --bps 1.0 \
    --delay 8.0 \
    --miners 1 \
    --tpb 10 \
    --target-blocks 1000 \
    --test-pruning \
    --output-dir "$SNAPSHOT_ROOT/goref_custom_pruning_depth"

# goref-notx-5000-blocks
cargo run --locked --bin simpa -- \
    --bps 1.0 \
    --delay 8.0 \
    --miners 1 \
    --tpb 0 \
    --target-blocks 5000 \
    --output-dir "$SNAPSHOT_ROOT/goref-notx-5000-blocks"

# goref-905-tx-265-blocks
cargo run --locked --bin simpa -- \
    --bps 1.0 \
    --delay 8.0 \
    --miners 1 \
    --tpb 10 \
    --target-blocks 500 \
    --output-dir "$SNAPSHOT_ROOT/goref-905-tx-265-blocks"

echo "All snapshots have been regenerated!"
