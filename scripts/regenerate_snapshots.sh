#!/bin/bash
set -e

# 1. 定义快照目录
SNAPSHOT_ROOT="testing/integration/testdata/dags_for_json_tests"
SNAPSHOT_SETS=("goref_custom_pruning_depth" "goref-notx-5000-blocks" "goref-905-tx-265-blocks")

# 2. 清空所有快照子目录
echo "清空旧快照文件..."
for set in "${SNAPSHOT_SETS[@]}"; do
    dir="$SNAPSHOT_ROOT/$set"
    if [ -d "$dir" ]; then
        # 清理 RocksDB 数据库文件
        rm -f "$dir"/*.sst "$dir"/*.log "$dir"/CURRENT "$dir"/MANIFEST* "$dir"/OPTIONS* "$dir"/LOCK
        rm -rf "$dir"
        echo "已删除 $dir"
    fi
done

# 3. 清理其他缓存
echo "清理缓存..."
rm -rf target/debug/simpa
rm -rf target/debug/deps/simpa*
rm -rf target/debug/.fingerprint/simpa*
rm -rf target/debug/build/simpa-*

# 4. 重新生成快照
echo "开始生成新快照..."

# goref_custom_pruning_depth - 增加区块数和交易数，添加延迟
cargo run --bin simpa -- \
    --bps 1.0 \
    --delay 8.0 \
    --miners 1 \
    --tpb 10 \
    --target-blocks 1000 \
    --test-pruning \
    --output-dir "$SNAPSHOT_ROOT/goref_custom_pruning_depth"

# goref-notx-5000-blocks
cargo run --bin simpa -- \
    --bps 1.0 \
    --delay 8.0 \
    --miners 1 \
    --tpb 0 \
    --target-blocks 5000 \
    --output-dir "$SNAPSHOT_ROOT/goref-notx-5000-blocks"

# goref-905-tx-265-blocks
cargo run --bin simpa -- \
    --bps 1.0 \
    --delay 8.0 \
    --miners 1 \
    --tpb 10 \
    --target-blocks 500 \
    --output-dir "$SNAPSHOT_ROOT/goref-905-tx-265-blocks"

echo "全部快照已重新生成！" 