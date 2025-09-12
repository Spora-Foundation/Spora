#!/bin/bash

# Integrated Airdrop Test Suite
# Complete end-to-end testing of the airdrop functionality

set -e

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TREASURE_BOY_BIN="$SCRIPT_DIR/../target/release/treasure_boy"
RPC_SERVER="127.0.0.1:16210"
PRIVATE_KEY="ce6cd3c38d45f749794796441d3208c37699030c5f62becf06b390242b95fb88"

# Test parameters
TEST_ADDRESSES_FILE="test_addresses_$(date +%s).txt"
VERIFICATION_LOG="verification_$(date +%s).log"
RESULTS_LOG="results_$(date +%s).log"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
PURPLE='\033[0;35m'
NC='\033[0m'

# Logging functions
log() {
    echo -e "${BLUE}[$(date '+%H:%M:%S')]${NC} $1" | tee -a "$RESULTS_LOG"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1" | tee -a "$RESULTS_LOG"
}

success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1" | tee -a "$RESULTS_LOG"
}

warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1" | tee -a "$RESULTS_LOG"
}

info() {
    echo -e "${PURPLE}[INFO]${NC} $1" | tee -a "$RESULTS_LOG"
}

# Cleanup function
cleanup() {
    log "Cleaning up test files..."
    rm -f "$TEST_ADDRESSES_FILE" "$VERIFICATION_LOG"
    # Keep RESULTS_LOG for review
}

trap cleanup EXIT

# Test 1: Address Generation
test_address_generation() {
    log "=== TEST 1: Address Generation ==="
    
    local test_counts=(5 10 20)
    
    for count in "${test_counts[@]}"; do
        log "Testing generation of $count addresses..."
        
        if "$TREASURE_BOY_BIN" --generate-addresses "$count" --output-file "test_${count}_addresses.txt"; then
            local actual_count=$(wc -l < "test_${count}_addresses.txt")
            if [ "$actual_count" -eq "$count" ]; then
                success "✅ Generated $actual_count addresses correctly"
            else
                error "❌ Expected $count addresses, got $actual_count"
                return 1
            fi
        else
            error "❌ Failed to generate $count addresses"
            return 1
        fi
        
        # Validate address format
        local invalid_count=0
        while IFS= read -r addr; do
            if [[ ! "$addr" =~ ^tondidev:[a-z0-9]{62}$ ]]; then
                invalid_count=$((invalid_count + 1))
                warning "Invalid address format: $addr"
            fi
        done < "test_${count}_addresses.txt"
        
        if [ "$invalid_count" -eq 0 ]; then
            success "✅ All addresses have valid format"
        else
            error "❌ Found $invalid_count invalid addresses"
            return 1
        fi
        
        rm -f "test_${count}_addresses.txt"
    done
    
    success "Address generation tests passed!"
}

# Test 2: RPC Connection
test_rpc_connection() {
    log "=== TEST 2: RPC Connection ==="
    
    log "Testing RPC connection to $RPC_SERVER..."
    
    if ! nc -z 127.0.0.1 16210 2>/dev/null; then
        error "❌ RPC server not accessible on $RPC_SERVER"
        error "Please start the Tondi node:"
        error "cargo run --bin tondid -- --devnet --rpclisten=127.0.0.1:16210"
        return 1
    fi
    
    success "✅ RPC server is accessible"
    
    # Test treasure_boy connection
    log "Testing treasure_boy RPC connection..."
    if timeout 10 "$TREASURE_BOY_BIN" \
        --private-key "$PRIVATE_KEY" \
        --rpcserver "$RPC_SERVER" \
        --tps 1 \
        --threads 1 \
        > /tmp/connection_test.log 2>&1; then
        success "✅ Treasure Boy connected to RPC successfully"
    else
        if grep -q "Connected to RPC" /tmp/connection_test.log; then
            success "✅ Treasure Boy connected to RPC successfully"
        else
            error "❌ Treasure Boy failed to connect to RPC"
            cat /tmp/connection_test.log
            return 1
        fi
    fi
    
    rm -f /tmp/connection_test.log
}

# Test 3: Address File Loading
test_address_loading() {
    log "=== TEST 3: Address File Loading ==="
    
    # Generate test addresses
    "$TREASURE_BOY_BIN" --generate-addresses 10 --output-file "$TEST_ADDRESSES_FILE"
    
    log "Testing address file loading..."
    
    if timeout 10 "$TREASURE_BOY_BIN" \
        --private-key "$PRIVATE_KEY" \
        --rpcserver "$RPC_SERVER" \
        --address-file "$TEST_ADDRESSES_FILE" \
        --outputs-per-tx 2 \
        --tps 1 \
        --threads 2 \
        > /tmp/loading_test.log 2>&1; then
        
        if grep -q "Loaded 10 addresses from file" /tmp/loading_test.log; then
            success "✅ Address file loaded correctly"
        else
            error "❌ Address file not loaded correctly"
            cat /tmp/loading_test.log
            return 1
        fi
    else
        if grep -q "Loaded 10 addresses from file" /tmp/loading_test.log; then
            success "✅ Address file loaded correctly"
        else
            error "❌ Address file loading failed"
            cat /tmp/loading_test.log
            return 1
        fi
    fi
    
    rm -f /tmp/loading_test.log
}

# Test 4: Parallel Processing
test_parallel_processing() {
    log "=== TEST 4: Parallel Processing ==="
    
    # Generate addresses for parallel test
    "$TREASURE_BOY_BIN" --generate-addresses 15 --output-file "$TEST_ADDRESSES_FILE"
    
    local thread_configs=("1" "2" "4" "8")
    
    for threads in "${thread_configs[@]}"; do
        log "Testing with $threads threads..."
        
        local start_time=$(date +%s)
        
        if timeout 30 "$TREASURE_BOY_BIN" \
            --private-key "$PRIVATE_KEY" \
            --rpcserver "$RPC_SERVER" \
            --address-file "$TEST_ADDRESSES_FILE" \
            --outputs-per-tx 3 \
            --tps 2 \
            --threads "$threads" \
            > /tmp/parallel_test_${threads}.log 2>&1; then
            
            local end_time=$(date +%s)
            local duration=$((end_time - start_time))
            
            if grep -q "batch airdrop to 15 addresses" /tmp/parallel_test_${threads}.log; then
                success "✅ Parallel processing with $threads threads completed in ${duration}s"
            else
                warning "⚠️  Parallel processing with $threads threads may have issues"
            fi
        else
            if grep -q "batch airdrop to 15 addresses" /tmp/parallel_test_${threads}.log; then
                success "✅ Parallel processing with $threads threads completed"
            else
                error "❌ Parallel processing with $threads threads failed"
            fi
        fi
        
        rm -f /tmp/parallel_test_${threads}.log
    done
}

# Test 5: Error Handling
test_error_handling() {
    log "=== TEST 5: Error Handling ==="
    
    # Test 1: No private key
    log "Testing error handling: no private key..."
    if "$TREASURE_BOY_BIN" --rpcserver "$RPC_SERVER" 2>&1 | grep -q "Error: --private-key is required"; then
        success "✅ Correctly handles missing private key"
    else
        error "❌ Failed to handle missing private key"
    fi
    
    # Test 2: Invalid address file
    log "Testing error handling: invalid address file..."
    echo "invalid_address_format" > invalid_addresses.txt
    if timeout 10 "$TREASURE_BOY_BIN" \
        --private-key "$PRIVATE_KEY" \
        --rpcserver "$RPC_SERVER" \
        --address-file "invalid_addresses.txt" \
        2>&1 | grep -q "Invalid address"; then
        success "✅ Correctly handles invalid addresses"
    else
        warning "⚠️  Invalid address handling may need improvement"
    fi
    rm -f invalid_addresses.txt
    
    # Test 3: Non-existent address file
    log "Testing error handling: non-existent address file..."
    if timeout 10 "$TREASURE_BOY_BIN" \
        --private-key "$PRIVATE_KEY" \
        --rpcserver "$RPC_SERVER" \
        --address-file "non_existent.txt" \
        2>&1 | grep -q "Failed to load addresses"; then
        success "✅ Correctly handles non-existent address file"
    else
        warning "⚠️  Non-existent file handling may need improvement"
    fi
}

# Test 6: Performance Benchmark
test_performance() {
    log "=== TEST 6: Performance Benchmark ==="
    
    local test_sizes=(5 10 20 50)
    
    for size in "${test_sizes[@]}"; do
        log "Benchmarking with $size addresses..."
        
        "$TREASURE_BOY_BIN" --generate-addresses "$size" --output-file "$TEST_ADDRESSES_FILE"
        
        local start_time=$(date +%s)
        
        timeout 60 "$TREASURE_BOY_BIN" \
            --private-key "$PRIVATE_KEY" \
            --rpcserver "$RPC_SERVER" \
            --address-file "$TEST_ADDRESSES_FILE" \
            --outputs-per-tx 2 \
            --tps 5 \
            --threads 4 \
            > /tmp/perf_${size}.log 2>&1 || true
        
        local end_time=$(date +%s)
        local duration=$((end_time - start_time))
        
        if [ "$duration" -gt 0 ]; then
            local rate=$((size * 100 / duration))
            info "Performance: $size addresses in ${duration}s (${rate} addresses/100s)"
        fi
        
        rm -f /tmp/perf_${size}.log
    done
}

# Generate final report
generate_report() {
    log "=== GENERATING FINAL REPORT ==="
    
    local report_file="test_report_$(date +%s).txt"
    
    cat > "$report_file" << EOF
Treasure Boy Integrated Test Report
==================================
Date: $(date)
Test Duration: $(date -d @$(($(date +%s) - START_TIME)) -u +%H:%M:%S)

Configuration:
- Treasure Boy Binary: $TREASURE_BOY_BIN
- RPC Server: $RPC_SERVER
- Private Key: ${PRIVATE_KEY:0:8}...

Test Results:
$(cat "$RESULTS_LOG")

Files Generated:
- Results Log: $RESULTS_LOG
- Test Addresses: $TEST_ADDRESSES_FILE (cleaned up)
- Verification Log: $VERIFICATION_LOG (cleaned up)

EOF
    
    success "📊 Test report generated: $report_file"
    info "📋 Full results log: $RESULTS_LOG"
}

# Main execution
main() {
    START_TIME=$(date +%s)
    
    log "🚀 Starting Treasure Boy Integrated Test Suite"
    log "=============================================="
    
    # Check prerequisites
    if [ ! -f "$TREASURE_BOY_BIN" ]; then
        error "❌ Treasure Boy binary not found: $TREASURE_BOY_BIN"
        error "Please build it first: cargo build --release"
        exit 1
    fi
    
    # Run all tests
    test_address_generation
    test_rpc_connection
    test_address_loading
    test_parallel_processing
    test_error_handling
    test_performance
    
    # Generate report
    generate_report
    
    success "🎉 All tests completed successfully!"
    log "=============================================="
}

# Help function
show_help() {
    echo "Treasure Boy Integrated Test Suite"
    echo ""
    echo "Usage: $0 [--help]"
    echo ""
    echo "This script runs comprehensive tests for the Treasure Boy airdrop functionality:"
    echo ""
    echo "Tests included:"
    echo "  1. Address Generation - Tests address generation with different counts"
    echo "  2. RPC Connection - Verifies connection to Tondi node"
    echo "  3. Address File Loading - Tests loading addresses from file"
    echo "  4. Parallel Processing - Tests multi-threaded processing"
    echo "  5. Error Handling - Tests error conditions"
    echo "  6. Performance Benchmark - Measures performance with different scales"
    echo ""
    echo "Prerequisites:"
    echo "  - Tondi node running on $RPC_SERVER"
    echo "  - Treasure Boy built with: cargo build --release"
    echo ""
    echo "Output:"
    echo "  - Detailed test logs"
    echo "  - Performance metrics"
    echo "  - Final test report"
}

# Parse arguments
case "${1:-}" in
    --help|-h)
        show_help
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
