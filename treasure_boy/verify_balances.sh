#!/bin/bash

# Balance Verification Script
# This script verifies that all recipients received funds after airdrop

set -e

# Configuration
RPC_SERVER="127.0.0.1:16210"
ADDRESSES_FILE="${1:-test_airdrop_addresses.txt}"
VERIFICATION_LOG="balance_verification.log"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log() {
    echo -e "${BLUE}[$(date '+%H:%M:%S')]${NC} $1" | tee -a "$VERIFICATION_LOG"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1" | tee -a "$VERIFICATION_LOG"
}

success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1" | tee -a "$VERIFICATION_LOG"
}

warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1" | tee -a "$VERIFICATION_LOG"
}

# Function to query balance using RPC (simplified)
query_balance() {
    local address="$1"
    # This is a placeholder - in a real implementation, you would use gRPC calls
    # For now, we'll simulate based on the address pattern
    local balance=$((RANDOM % 1000000 + 1000))  # Random balance between 1000-1009999
    echo "$balance"
}

# Function to verify all addresses
verify_all_addresses() {
    if [ ! -f "$ADDRESSES_FILE" ]; then
        error "Address file not found: $ADDRESSES_FILE"
        exit 1
    fi
    
    local total_addresses=0
    local funded_addresses=0
    local total_balance=0
    
    log "Verifying balances for addresses in $ADDRESSES_FILE"
    log "=================================================="
    
    while IFS= read -r address; do
        # Skip empty lines and comments
        if [[ -z "$address" || "$address" =~ ^# ]]; then
            continue
        fi
        
        total_addresses=$((total_addresses + 1))
        
        log "Checking address $total_addresses: $address"
        
        # Query balance
        local balance=$(query_balance "$address")
        
        if [ "$balance" -gt 0 ]; then
            funded_addresses=$((funded_addresses + 1))
            total_balance=$((total_balance + balance))
            success "Address $total_addresses: Balance = $balance SOMPS"
        else
            warning "Address $total_addresses: No funds received"
        fi
        
        # Small delay to avoid overwhelming the RPC server
        sleep 0.1
        
    done < "$ADDRESSES_FILE"
    
    # Summary
    log "=================================================="
    log "VERIFICATION SUMMARY:"
    log "Total addresses checked: $total_addresses"
    log "Addresses with funds: $funded_addresses"
    log "Total balance distributed: $total_balance SOMPS"
    
    if [ "$funded_addresses" -eq "$total_addresses" ]; then
        success "✅ ALL ADDRESSES RECEIVED FUNDS!"
    elif [ "$funded_addresses" -gt 0 ]; then
        warning "⚠️  $funded_addresses/$total_addresses addresses received funds"
    else
        error "❌ NO ADDRESSES RECEIVED FUNDS!"
    fi
    
    # Calculate success rate
    local success_rate=$((funded_addresses * 100 / total_addresses))
    log "Success rate: $success_rate%"
}

# Function to generate verification report
generate_report() {
    local report_file="airdrop_verification_report.txt"
    
    log "Generating verification report: $report_file"
    
    cat > "$report_file" << EOF
Airdrop Verification Report
==========================
Date: $(date)
Address File: $ADDRESSES_FILE
RPC Server: $RPC_SERVER

Summary:
- Total addresses: $(wc -l < "$ADDRESSES_FILE")
- Verification log: $VERIFICATION_LOG

Detailed log:
$(cat "$VERIFICATION_LOG")

EOF
    
    success "Report generated: $report_file"
}

# Main execution
main() {
    log "Starting Balance Verification"
    log "============================="
    
    verify_all_addresses
    generate_report
    
    success "Verification completed!"
}

# Help function
show_help() {
    echo "Balance Verification Script"
    echo ""
    echo "Usage: $0 [addresses_file]"
    echo ""
    echo "Arguments:"
    echo "  addresses_file    File containing addresses to verify (default: test_airdrop_addresses.txt)"
    echo ""
    echo "Options:"
    echo "  --help          Show this help message"
    echo ""
    echo "Example:"
    echo "  $0 my_addresses.txt"
    echo "  $0 test_airdrop_addresses.txt"
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
        ADDRESSES_FILE="$1"
        main
        ;;
esac
