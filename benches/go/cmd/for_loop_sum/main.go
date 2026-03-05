package main

import (
	"flag"
	"fmt"
	"time"
)

func rangeSum(n int) int {
	total := 0
	for i := 0; i < n; i++ {
		total += i
	}
	return total
}

func main() {
	iterations := flag.Int("iterations", 100000, "number of benchmark iterations")
	warmup := flag.Int("warmup", 1000, "number of warmup iterations")
	flag.Parse()

	// Warmup
	var sink int
	for i := 0; i < *warmup; i++ {
		sink = rangeSum(1000)
	}

	// Benchmark
	start := time.Now()
	for i := 0; i < *iterations; i++ {
		sink = rangeSum(1000)
	}
	elapsed := time.Since(start)

	if sink == 0 {
		fmt.Print("")
	}

	fmt.Printf(`{"iterations":%d,"total_ns":%d}`+"\n", *iterations, elapsed.Nanoseconds())
}
