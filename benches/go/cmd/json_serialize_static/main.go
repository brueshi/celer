package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"time"
)

type response struct {
	Message string `json:"message"`
}

func root() response {
	return response{Message: "hello"}
}

func main() {
	iterations := flag.Int("iterations", 100000, "number of benchmark iterations")
	warmup := flag.Int("warmup", 1000, "number of warmup iterations")
	flag.Parse()

	resp := root()

	// Warmup
	for i := 0; i < *warmup; i++ {
		_, _ = json.Marshal(resp)
	}

	// Benchmark
	start := time.Now()
	for i := 0; i < *iterations; i++ {
		data, _ := json.Marshal(resp)
		_ = data
	}
	elapsed := time.Since(start)

	fmt.Printf(`{"iterations":%d,"total_ns":%d}`+"\n", *iterations, elapsed.Nanoseconds())
}
