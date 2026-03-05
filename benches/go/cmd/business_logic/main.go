package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"time"
)

type priceResponse struct {
	Price    int    `json:"price"`
	Currency string `json:"currency"`
}

func applyDiscount(price, threshold int) int {
	if price > threshold {
		return price * 90 / 100
	}
	return price
}

func calculatePrice(basePrice int) priceResponse {
	finalPrice := applyDiscount(basePrice, 50)
	return priceResponse{Price: finalPrice, Currency: "USD"}
}

func main() {
	iterations := flag.Int("iterations", 100000, "number of benchmark iterations")
	warmup := flag.Int("warmup", 1000, "number of warmup iterations")
	flag.Parse()

	// Warmup
	for i := 0; i < *warmup; i++ {
		_, _ = json.Marshal(calculatePrice(100))
	}

	// Benchmark
	start := time.Now()
	for i := 0; i < *iterations; i++ {
		data, _ := json.Marshal(calculatePrice(100))
		_ = data
	}
	elapsed := time.Since(start)

	fmt.Printf(`{"iterations":%d,"total_ns":%d}`+"\n", *iterations, elapsed.Nanoseconds())
}
