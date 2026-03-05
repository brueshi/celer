package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"time"
)

type itemResponse struct {
	ItemID  int    `json:"item_id"`
	Name    string `json:"name"`
	InStock bool   `json:"in_stock"`
}

func getItem(itemID int) itemResponse {
	return itemResponse{ItemID: itemID, Name: "widget", InStock: true}
}

func main() {
	iterations := flag.Int("iterations", 100000, "number of benchmark iterations")
	warmup := flag.Int("warmup", 1000, "number of warmup iterations")
	flag.Parse()

	// Warmup
	for i := 0; i < *warmup; i++ {
		_, _ = json.Marshal(getItem(42))
	}

	// Benchmark
	start := time.Now()
	for i := 0; i < *iterations; i++ {
		data, _ := json.Marshal(getItem(42))
		_ = data
	}
	elapsed := time.Since(start)

	fmt.Printf(`{"iterations":%d,"total_ns":%d}`+"\n", *iterations, elapsed.Nanoseconds())
}
