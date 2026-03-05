package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"time"
)

type computeResponse struct {
	Result int `json:"result"`
	Input  int `json:"input"`
}

func compute(n int) computeResponse {
	result := 0
	for i := 0; i < n; i++ {
		result += i
	}
	return computeResponse{Result: result, Input: n}
}

func main() {
	iterations := flag.Int("iterations", 100000, "number of benchmark iterations")
	warmup := flag.Int("warmup", 1000, "number of warmup iterations")
	flag.Parse()

	// Warmup
	for i := 0; i < *warmup; i++ {
		_, _ = json.Marshal(compute(100))
	}

	// Benchmark
	start := time.Now()
	for i := 0; i < *iterations; i++ {
		data, _ := json.Marshal(compute(100))
		_ = data
	}
	elapsed := time.Since(start)

	fmt.Printf(`{"iterations":%d,"total_ns":%d}`+"\n", *iterations, elapsed.Nanoseconds())
}
