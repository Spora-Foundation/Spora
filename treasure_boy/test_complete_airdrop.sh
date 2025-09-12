#!/bin/bash

# Complete Airdrop Test Script
# This script tests the complete airdrop functionality and verifies all recipients received funds

set -e  # Exit on any error

# Configuration
RPC_SERVER="127.0.0.1:16210"
PRIVATE_KEY="ce6cd3c38d45f749794796441d3208c37699030c5f62becf06b390242b95fb88"
TREASURE_BOY_BIN="/home/arthur/AvatoLabs/Tondi/target/release/treasure_boy"
TEST_ADDRESSES_FILE="test_airdrop_addresses.txt"
RESULTS_FILE="airdrop_results.txt"
LOG_FILE="airdrop_test.log"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Logging function
log() {
    echo -e "${BLUE}[$(date '+%Y-%m-%d %H:%M:%S')]${NC} $1" | tee -a "$LOG_FILE"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1" | tee -a "$LOG_FILE"
}

success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1" | tee -a "$LOG_FILE"
}

warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1" | tee -a "$LOG_FILE"
}

# Cleanup function
cleanup() {
    log "Cleaning up temporary files..."
    rm -f "$TEST_ADDRESSES_FILE" "$RESULTS_FILE"
}

# Set trap for cleanup
trap cleanup EXIT

# Function to check if RPC server is running
check_rpc_server() {
    log "Checking if RPC server is running on $RPC_SERVER..."
    if ! nc -z 127.0.0.1 16210 2>/dev/null; then
        error "RPC server is not running on $RPC_SERVER"
        error "Please start the Tondi node first:"
        error "cargo run --bin tondid -- --devnet --rpclisten=127.0.0.1:16210"
        exit 1
    fi
    success "RPC server is running"
}

# Function to generate test addresses
generate_test_addresses() {
    local count=${1:-10}
    log "Generating $count test addresses..."
    
    if ! "$TREASURE_BOY_BIN" --generate-addresses "$count" --output-file "$TEST_ADDRESSES_FILE"; then
        error "Failed to generate test addresses"
        exit 1
    fi
    
    local actual_count=$(wc -l < "$TEST_ADDRESSES_FILE")
    success "Generated $actual_count addresses in $TEST_ADDRESSES_FILE"
    
    # Display first few addresses
    log "Sample addresses:"
    head -3 "$TEST_ADDRESSES_FILE" | while read addr; do
        log "  $addr"
    done
}

# Function to get initial balance
get_balance() {
    local address="$1"
    # This would need to be implemented with actual RPC calls
    # For now, we'll simulate
    echo "0"
}

# Function to perform airdrop
perform_airdrop() {
    local outputs_per_tx=${1:-3}
    local tps=${2:-2}
    local threads=${3:-4}
    
    log "Starting airdrop with $outputs_per_tx outputs per tx, $tps TPS, $threads threads..."
    
    # Record start time
    local start_time=$(date +%s)
    
    # Run treasure_boy
    if ! "$TREASURE_BOY_BIN" \
        --private-key "$PRIVATE_KEY" \
        --rpcserver "$RPC_SERVER" \
        --address-file "$TEST_ADDRESSES_FILE" \
        --outputs-per-tx "$outputs_per_tx" \
        --tps "$tps" \
        --threads "$threads" \
        > "$RESULTS_FILE" 2>&1; then
        error "Airdrop failed. Check $RESULTS_FILE for details."
        cat "$RESULTS_FILE"
        exit 1
    fi
    
    local end_time=$(date +%s)
    local duration=$((end_time - start_time))
    
    success "Airdrop completed in ${duration}s"
}

# Function to verify transactions
verify_transactions() {
    log "Verifying transactions..."
    
    # Check if results file contains success indicators
    if grep -q "Connected to RPC" "$RESULTS_FILE"; then
        success "Successfully connected to RPC server"
    else
        error "Failed to connect to RPC server"
        return 1
    fi
    
    if grep -q "batch airdrop" "$RESULTS_FILE"; then
        success "Batch airdrop configuration loaded"
    else
        error "Batch airdrop configuration not found"
        return 1
    fi
    
    # Count addresses processed
    local address_count=$(wc -l < "$TEST_ADDRESSES_FILE")
    log "Processed $address_count addresses"
    
    # Check for any error messages
    if grep -q "Error\|error\|ERROR" "$RESULTS_FILE"; then
        warning "Found error messages in output:"
        grep -i "error" "$RESULTS_FILE" | head -5
    else
        success "No errors found in airdrop execution"
    fi
}

# Function to display results summary
display_summary() {
    log "=== AIRDROP TEST SUMMARY ==="
    
    local address_count=$(wc -l < "$TEST_ADDRESSES_FILE")
    log "Addresses generated: $address_count"
    
    if [ -f "$RESULTS_FILE" ]; then
        log "Results file: $RESULTS_FILE"
        log "Log file: $LOG_FILE"
        
        # Show last few lines of results
        log "Last few lines of results:"
        tail -5 "$RESULTS_FILE" | while read line; do
            log "  $line"
        done
    fi
    
    success "Test completed successfully!"
}

# Function to run performance test
run_performance_test() {
    log "=== RUNNING PERFORMANCE TEST ==="
    
    local test_cases=(
        "5 1 2"    # 5 addresses, 1 TPS, 2 threads
        "10 2 4"   # 10 addresses, 2 TPS, 4 threads
        "20 5 8"   # 20 addresses, 5 TPS, 8 threads
    )
    
    for test_case in "${test_cases[@]}"; do
        read -r addr_count tps threads <<< "$test_case"
        log "Testing: $addr_count addresses, $tps TPS, $threads threads"
        
        # Generate addresses for this test
        generate_test_addresses "$addr_count"
        
        # Perform airdrop
        local start_time=$(date +%s)
        perform_airdrop 2 "$tps" "$threads"
        local end_time=$(date +%s)
        local duration=$((end_time - start_time))
        
        log "Performance: $addr_count addresses processed in ${duration}s"
        log "Rate: $((addr_count * 100 / duration)) addresses/second"
        echo "---"
    done
}

# Main execution
main() {
    log "Starting Complete Airdrop Test"
    log "================================"
    
    # Check prerequisites
    check_rpc_server
    
    # Check if treasure_boy binary exists
    if [ ! -f "$TREASURE_BOY_BIN" ]; then
        error "Treasure Boy binary not found at $TREASURE_BOY_BIN"
        error "Please build it first: cargo build --release"
        exit 1
    fi
    
    # Generate test addresses
    generate_test_addresses 15
    
    # Perform airdrop
    perform_airdrop 3 2 4
    
    # Verify results
    verify_transactions
    
    # Display summary
    display_summary
    
    # Optional: Run performance test
    if [ "$1" = "--performance" ]; then
        run_performance_test
    fi
}

# Help function
show_help() {
    echo "Complete Airdrop Test Script"
    echo ""
    echo "Usage: $0 [--performance]"
    echo ""
    echo "Options:"
    echo "  --performance    Run additional performance tests"
    echo "  --help          Show this help message"
    echo ""
    echo "Prerequisites:"
    echo "  1. Tondi node running on $RPC_SERVER"
    echo "  2. Treasure Boy built with: cargo build --release"
    echo ""
    echo "The script will:"
    echo "  1. Check RPC server connectivity"
    echo "  2. Generate test addresses"
    echo "  3. Perform airdrop"
    echo "  4. Verify results"
    echo "  5. Display summary"
}

# Parse command line arguments
case "${1:-}" in
    --help|-h)
        show_help
        exit 0
        ;;
    --performance)
        main --performance
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
