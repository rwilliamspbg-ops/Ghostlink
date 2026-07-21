#!/bin/bash
# ================================================
# Ghost-Link Throughput Benchmark - Polished Version
# ================================================

echo "========================================"
echo "   Ghost-Link Throughput Benchmark"
echo "   Multiple Runs + CSV Export"
echo "========================================"

# Configuration
ITERATIONS=3                    # Change this for more/less runs
OUTPUT_CSV="throughput_results_$(date +%Y%m%d_%H%M%S).csv"

# Kill any running instances
echo "Cleaning up old processes..."
taskkill /F /IM ghost-link.exe 2>/dev/null || true
sleep 1

echo "Building release version..."
cargo build --release -p ghost-link

# CSV Header
echo "Transport,Token_Count,Micro_Batch,Iteration,Throughput_tok_per_sec,Wall_Time_ms,Avg_Latency_ms" > "$OUTPUT_CSV"

echo -e "\nStarting benchmarks ($ITERATIONS iterations each)...\n"

run_test() {
    local transport=$1
    local tokens=$2
    local micro_batch=$3
    local label="$transport-${tokens}tok"

    echo "→ Running $label ($ITERATIONS iterations)"

    local total=0
    local count=0

    for i in $(seq 1 $ITERATIONS); do
        echo "   Run $i/$ITERATIONS..."
        
        output=$(cargo run -p ghost-link --release --quiet -- flow local remote 24 64 $tokens $micro_batch $transport 2>&1)
        
        # Extract throughput
        tps=$(echo "$output" | grep -oE 'Throughput: [0-9]+\.[0-9]+' | awk '{print $2}')
        
        if [[ -n "$tps" ]]; then
            echo "$transport,$tokens,$micro_batch,$i,$tps" >> "$OUTPUT_CSV"
            total=$(awk "BEGIN {print $total + $tps}")
            ((count++))
            printf "     Throughput: %.0f tokens/sec\n" "$tps"
        else
            echo "     Failed to parse throughput"
        fi
    done

    if [[ $count -gt 0 ]]; then
        avg=$(awk "BEGIN {print $total / $count}")
        printf "   Average: \033[1;32m%.0f tokens/sec\033[0m\n\n" "$avg"
    fi
}

# Run all test cases
run_test "inmem" 64 8
run_test "inmem" 256 32
run_test "inmem" 1024 128

run_test "tcp" 64 8
run_test "tcp" 256 32
run_test "tcp" 1024 128

echo "========================================"
echo "Benchmark completed!"
echo "Results saved to: $OUTPUT_CSV"
echo "========================================"

# Show summary
echo -e "\nSummary (Averages):"
awk -F, 'NR>1 {sum[$1$2] += $5; count[$1$2]++} 
         END {for (k in sum) printf "%-12s %6.0f tokens/sec\n", k, sum[k]/count[k]}' "$OUTPUT_CSV"