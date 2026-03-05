package main

import (
	"flag"
	"fmt"
	"time"
)

func fib(n int) int {
	a, b := 0, 1
	for i := 0; i < n; i++ {
		a, b = b, a+b
	}
	return a
}

func main() {
	iterations := flag.Int("iterations", 100000, "number of benchmark iterations")
	warmup := flag.Int("warmup", 1000, "number of warmup iterations")
	flag.Parse()

	// Warmup
	var sink int
	for i := 0; i < *warmup; i++ {
		sink = fib(30)
	}

	// Benchmark
	start := time.Now()
	for i := 0; i < *iterations; i++ {
		sink = fib(30)
	}
	elapsed := time.Since(start)

	// Prevent dead code elimination
	if sink == 0 {
		fmt.Print("")
	}

	fmt.Printf(`{"iterations":%d,"total_ns":%d}`+"\n", *iterations, elapsed.Nanoseconds())
}
