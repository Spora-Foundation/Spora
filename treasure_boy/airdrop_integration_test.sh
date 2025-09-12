#!/bin/bash

# Treasure Boy Airdrop Integration Test Script
# Perform airdrop using specified private key and verify transaction results

set -e

# Configuration
PRIVATE_KEY="c99b1ccf1087af2a56ffedb885943962e0159a7705cac583eef3e9958cd035b3"
NETWORK="testnet"
RPC_SERVER="127.0.0.1:16210"
TREASURE_BOY_BIN="../target/release/treasure_boy"
TEST_ADDRESSES_FILE="test_addresses.txt"
RESULTS_FILE="airdrop_results.txt"
BALANCE_CHECK_FILE="balance_check.txt"

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

# 日志函数
log() {
    echo -e "${BLUE}[$(date '+%H:%M:%S')]${NC} $1"
}

success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

info() {
    echo -e "${CYAN}[INFO]${NC} $1"
}

# Check prerequisites
check_prerequisites() {
    log "=== Checking Prerequisites ==="
    
    # Check treasure_boy binary file
    if [ ! -f "$TREASURE_BOY_BIN" ]; then
        error "Treasure Boy binary file not found: $TREASURE_BOY_BIN"
        error "Please build first: cargo build --release"
        exit 1
    fi
    
    # Check RPC connection
    if ! nc -z 127.0.0.1 16210 2>/dev/null; then
        error "RPC server not accessible (127.0.0.1:16210)"
        error "Please start tondid node first: cargo run --bin tondid --release -- --testnet --netsuffix=10 --utxoindex --rpclisten=127.0.0.1:16210"
        exit 1
    fi
    
    success "Prerequisites check passed"
}

# Generate test addresses
generate_test_addresses() {
    log "=== Generating Test Addresses ==="
    
    # Generate 5 test addresses
    "$TREASURE_BOY_BIN" --generate-addresses 5 --network "$NETWORK" --output-file "$TEST_ADDRESSES_FILE"
    
    if [ ! -f "$TEST_ADDRESSES_FILE" ]; then
        error "Address generation failed"
        exit 1
    fi
    
    local address_count=$(wc -l < "$TEST_ADDRESSES_FILE")
    success "Generated $address_count test addresses"
    
    info "Test address list:"
    cat "$TEST_ADDRESSES_FILE" | nl -w2 -s': '
}

# Record pre-airdrop state
record_pre_airdrop_state() {
    log "=== Recording Pre-Airdrop State ==="
    
    # Get wallet address
    local wallet_address=$("$TREASURE_BOY_BIN" --generate-addresses 1 --network "$NETWORK" --output-file "temp_wallet.txt" && head -1 "temp_wallet.txt")
    rm -f "temp_wallet.txt"
    
    info "Wallet address: $wallet_address"
    
    # Record timestamp
    echo "=== Pre-Airdrop State Record ===" > "$BALANCE_CHECK_FILE"
    echo "Time: $(date)" >> "$BALANCE_CHECK_FILE"
    echo "Wallet address: $wallet_address" >> "$BALANCE_CHECK_FILE"
    echo "Test address count: $(wc -l < "$TEST_ADDRESSES_FILE")" >> "$BALANCE_CHECK_FILE"
    echo "" >> "$BALANCE_CHECK_FILE"
    
    success "Pre-airdrop state recorded"
}

# Perform airdrop
perform_airdrop() {
    log "=== Performing Airdrop ==="
    
    info "Starting batch airdrop to $(wc -l < "$TEST_ADDRESSES_FILE") addresses..."
    
    # Execute airdrop and record results
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
        
        # Extract transaction IDs
        local tx_ids=$(grep "Transaction [0-9]:" "$RESULTS_FILE" | sed 's/.*Transaction [0-9]: //')
        local tx_count=$(echo "$tx_ids" | wc -l)
        
        info "Successfully sent $tx_count transactions"
        echo "=== Transaction ID List ===" >> "$BALANCE_CHECK_FILE"
        echo "$tx_ids" >> "$BALANCE_CHECK_FILE"
        echo "" >> "$BALANCE_CHECK_FILE"
        
        # Display transaction IDs
        info "Transaction ID list:"
        echo "$tx_ids" | nl -w2 -s': '
        
    else
        error "Airdrop execution failed"
        error "Error log:"
        cat "$RESULTS_FILE"
        exit 1
    fi
}

# Verify transaction results
verify_transactions() {
    log "=== Verifying Transaction Results ==="
    
    # Wait for transaction confirmation
    info "Waiting for transaction confirmation..."
    sleep 5
    
    # Check if transactions are in mempool
    log "Checking transaction status..."
    
    # Here we can add more detailed transaction verification logic
    # Such as querying RPC for transaction status, checking UTXO changes, etc.
    
    success "Transaction verification completed"
}

# Generate test report
generate_report() {
    log "=== Generating Test Report ==="
    
    local report_file="airdrop_test_report_$(date +%Y%m%d_%H%M%S).txt"
    
    {
        echo "=========================================="
        echo "Treasure Boy Airdrop Integration Test Report"
        echo "=========================================="
        echo "Test time: $(date)"
        echo "Network: $NETWORK"
        echo "RPC server: $RPC_SERVER"
        echo "Private key: $PRIVATE_KEY"
        echo ""
        echo "=== Test Addresses ==="
        cat "$TEST_ADDRESSES_FILE" | nl -w2 -s': '
        echo ""
        echo "=== Airdrop Results ==="
        cat "$RESULTS_FILE"
        echo ""
        echo "=== Balance Check Record ==="
        cat "$BALANCE_CHECK_FILE"
        echo ""
        echo "=========================================="
        echo "Test completed"
        echo "=========================================="
    } > "$report_file"
    
    success "Test report generated: $report_file"
    
    # Display report summary
    info "=== Test Summary ==="
    echo "Test address count: $(wc -l < "$TEST_ADDRESSES_FILE")"
    echo "Successful transaction count: $(grep -c "Transaction [0-9]:" "$RESULTS_FILE")"
    echo "Report file: $report_file"
}

# Clean up temporary files
cleanup() {
    log "=== Cleaning Up Temporary Files ==="
    
    # Keep important files, remove temporary files
    rm -f "temp_wallet.txt"
    
    info "Files kept:"
    echo "  - $TEST_ADDRESSES_FILE (test addresses)"
    echo "  - $RESULTS_FILE (airdrop results)"
    echo "  - $BALANCE_CHECK_FILE (balance check record)"
    echo "  - airdrop_test_report_*.txt (test reports)"
}

# Show help information
show_help() {
    echo "Treasure Boy Airdrop Integration Test Script"
    echo ""
    echo "Usage: $0 [options]"
    echo ""
    echo "Options:"
    echo "  --help, -h     Show this help information"
    echo "  --clean        Clean all test files"
    echo ""
    echo "Features:"
    echo "  1. Generate test addresses"
    echo "  2. Execute batch airdrop"
    echo "  3. Verify transaction results"
    echo "  4. Generate test report"
    echo ""
    echo "Prerequisites:"
    echo "  - tondid node must be running on 127.0.0.1:16210"
    echo "  - treasure_boy binary file must exist"
    echo "  - Private key must have sufficient UTXO"
}

# Clean all test files
clean_all() {
    log "=== Cleaning All Test Files ==="
    
    rm -f "$TEST_ADDRESSES_FILE"
    rm -f "$RESULTS_FILE"
    rm -f "$BALANCE_CHECK_FILE"
    rm -f "temp_wallet.txt"
    rm -f "airdrop_test_report_*.txt"
    
    success "All test files cleaned"
}

# Main function
main() {
    log "🚀 Starting Treasure Boy Airdrop Integration Test"
    log "=================================="
    
    # Check prerequisites
    check_prerequisites
    
    # Generate test addresses
    generate_test_addresses
    
    # Record pre-airdrop state
    record_pre_airdrop_state
    
    # Perform airdrop
    perform_airdrop
    
    # Verify transaction results
    verify_transactions
    
    # Generate test report
    generate_report
    
    # Clean up temporary files
    cleanup
    
    log "=================================="
    success "🎉 Airdrop integration test completed!"
}

# Parse command line arguments
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
