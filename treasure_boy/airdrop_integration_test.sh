#!/bin/bash

# Treasure Boy Airdrop Integration Test Script
# Exercises the currently supported address generation and standard batch airdrop flow.

set -euo pipefail

PRIVATE_KEY="c99b1ccf1087af2a56ffedb885943962e0159a7705cac583eef3e9958cd035b3"
NETWORK="testnet"
RPC_SERVER="127.0.0.1:16210"
TREASURE_BOY_BIN="../target/release/treasure_boy"
TEST_ADDRESSES_FILE="test_addresses.txt"
RESULTS_FILE="airdrop_results.txt"
BALANCE_CHECK_FILE="balance_check.txt"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

log() {
    echo -e "${BLUE}[$(date '+%H:%M:%S')]${NC} $1"
}

success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

info() {
    echo -e "${CYAN}[INFO]${NC} $1"
}

check_prerequisites() {
    log "=== Checking Prerequisites ==="

    if [ ! -f "$TREASURE_BOY_BIN" ]; then
        error "Treasure Boy binary file not found: $TREASURE_BOY_BIN"
        error "Please build first: cargo build --release -p treasure_boy"
        exit 1
    fi

    if ! nc -z 127.0.0.1 16210 2>/dev/null; then
        error "RPC server not accessible (127.0.0.1:16210)"
        error "Please start a node first"
        exit 1
    fi

    success "Prerequisites check passed"
}

generate_test_addresses() {
    log "=== Generating Test Addresses ==="

    "$TREASURE_BOY_BIN" --generate-addresses 5 --network "$NETWORK" --output-file "$TEST_ADDRESSES_FILE"

    if [ ! -f "$TEST_ADDRESSES_FILE" ]; then
        error "Address generation failed"
        exit 1
    fi

    local address_count
    address_count=$(wc -l < "$TEST_ADDRESSES_FILE")
    success "Generated $address_count test addresses"

    info "Test address list:"
    nl -w2 -s': ' "$TEST_ADDRESSES_FILE"
}

record_pre_airdrop_state() {
    log "=== Recording Pre-Airdrop State ==="

    {
        echo "=== Pre-Airdrop State Record ==="
        echo "Time: $(date)"
        echo "Network: $NETWORK"
        echo "RPC server: $RPC_SERVER"
        echo "Test address count: $(wc -l < "$TEST_ADDRESSES_FILE")"
        echo
    } > "$BALANCE_CHECK_FILE"

    success "Pre-airdrop state recorded"
}

perform_airdrop() {
    log "=== Performing Batch Airdrop ==="

    if "$TREASURE_BOY_BIN" \
        --private-key "$PRIVATE_KEY" \
        --network "$NETWORK" \
        --rpcserver "$RPC_SERVER" \
        --address-file "$TEST_ADDRESSES_FILE" \
        --tps 1 \
        --threads 1 \
        --outputs-per-tx 1 \
        > "$RESULTS_FILE" 2>&1; then

        success "Airdrop executed successfully"

        local tx_ids
        tx_ids=$(grep "Transaction [0-9]:" "$RESULTS_FILE" | sed 's/.*Transaction [0-9]: //')
        local tx_count
        tx_count=$(echo "$tx_ids" | sed '/^$/d' | wc -l)

        info "Successfully sent $tx_count transactions"
        {
            echo "=== Transaction ID List ==="
            echo "$tx_ids"
            echo
        } >> "$BALANCE_CHECK_FILE"

        info "Transaction ID list:"
        echo "$tx_ids" | sed '/^$/d' | nl -w2 -s': '
    else
        error "Airdrop execution failed"
        cat "$RESULTS_FILE"
        exit 1
    fi
}

verify_transactions() {
    log "=== Verifying Transaction Results ==="
    info "Waiting briefly for propagation..."
    sleep 5
    success "Verification step completed"
}

generate_report() {
    log "=== Generating Test Report ==="

    local report_file
    report_file="airdrop_test_report_$(date +%Y%m%d_%H%M%S).txt"

    {
        echo "=========================================="
        echo "Treasure Boy Airdrop Integration Test Report"
        echo "=========================================="
        echo "Test time: $(date)"
        echo "Network: $NETWORK"
        echo "RPC server: $RPC_SERVER"
        echo
        echo "=== Test Addresses ==="
        nl -w2 -s': ' "$TEST_ADDRESSES_FILE"
        echo
        echo "=== Airdrop Results ==="
        cat "$RESULTS_FILE"
        echo
        echo "=== Balance Check Record ==="
        cat "$BALANCE_CHECK_FILE"
        echo
    } > "$report_file"

    success "Test report generated: $report_file"
}

cleanup() {
    log "=== Cleaning Up Temporary Files ==="

    rm -f temp_wallet.txt

    info "Files kept:"
    echo "  - $TEST_ADDRESSES_FILE"
    echo "  - $RESULTS_FILE"
    echo "  - $BALANCE_CHECK_FILE"
    echo "  - airdrop_test_report_*.txt"
}

show_help() {
    echo "Treasure Boy Airdrop Integration Test Script"
    echo
    echo "Usage: $0 [options]"
    echo
    echo "Options:"
    echo "  --help, -h     Show help"
    echo "  --clean        Clean generated test files"
    echo
    echo "This script covers the currently supported standard airdrop flow only."
}

clean_all() {
    log "=== Cleaning All Test Files ==="

    rm -f "$TEST_ADDRESSES_FILE"
    rm -f "$RESULTS_FILE"
    rm -f "$BALANCE_CHECK_FILE"
    rm -f temp_wallet.txt
    rm -f airdrop_test_report_*.txt

    success "All test files cleaned"
}

main() {
    log "Starting Treasure Boy Airdrop Integration Test"
    check_prerequisites
    generate_test_addresses
    record_pre_airdrop_state
    perform_airdrop
    verify_transactions
    generate_report
    cleanup
    success "Airdrop integration test completed"
}

case "${1:-}" in
    --help|-h)
        show_help
        exit 0
        ;;
    --clean)
        clean_all
        exit 0
        ;;
    "")
        main
        ;;
    *)
        error "Unknown option: $1"
        show_help
        exit 1
        ;;
esac
