#!/bin/bash

# Set test data directory
TEST_DATA_DIR="testing/integration/testdata/dags_for_json_tests"

# Create temporary directory
TEMP_DIR=$(mktemp -d)
trap 'rm -rf "$TEMP_DIR"' EXIT

# Generate goref_custom_pruning_depth test data
echo "Generating goref_custom_pruning_depth test data..."
cargo run --bin simpa -- \
    --bps 1.0 \
    --delay 2.0 \
    --miners 1 \
    --tpb 0 \
    --target-blocks 100 \
    --test-pruning \
    --output-dir "$TEMP_DIR/goref_custom_pruning_depth"

# Generate goref-notx-5000-blocks test data
echo "Generating goref-notx-5000-blocks test data..."
cargo run --bin simpa -- \
    --bps 1.0 \
    --delay 2.0 \
    --miners 1 \
    --tpb 0 \
    --target-blocks 5000 \
    --output-dir "$TEMP_DIR/goref-notx-5000-blocks"

# Generate goref-905-tx-265-blocks test data
echo "Generating goref-905-tx-265-blocks test data..."
cargo run --bin simpa -- \
    --bps 1.0 \
    --delay 2.0 \
    --miners 1 \
    --tpb 5 \
    --target-blocks 265 \
    --output-dir "$TEMP_DIR/goref-905-tx-265-blocks"

# Export block data
for dir in "$TEMP_DIR"/*; do
    if [ -d "$dir" ]; then
        dirname=$(basename "$dir")
        echo "Exporting blocks from $dirname..."
        # Start tondid in the background
        cargo run --bin tondid -- --appdir "$dir" &
        TONDID_PID=$!
        
        # Wait for tondid to start
        sleep 2
        
        # Use RPC to get blocks
        cargo run --bin tondid -- --rpc get-blocks --include-blocks true --include-transactions true > "$TEST_DATA_DIR/$dirname/blocks.json"
        
        # Kill tondid
        kill $TONDID_PID
    fi
done

echo "Test data regeneration complete!" 