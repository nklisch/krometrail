package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net/http"
	"os"
	"time"
)

var (
	pricingURL = getEnv("PRICING_URL", "http://localhost:5002")
	catalogURL = getEnv("CATALOG_URL", "http://localhost:5001")
)

func getEnv(key, fallback string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	return fallback
}

var httpClient = &http.Client{Timeout: 15 * time.Second}

// indexedResult carries a priced item back from a goroutine with its original index.
type indexedResult struct {
	idx  int
	item PricedItem
	err  error
}

// priceItems calls the pricing service for each cart item concurrently.
// Each goroutine prices one item and sends the result through a buffered channel.
func priceItems(items []CartItem) ([]PricedItem, error) {
	results := make([]PricedItem, len(items))
	ch := make(chan indexedResult, len(items))

	for i, item := range items {
		go func(idx int, it CartItem) {
			priced, err := callPricingBatch([]CartItem{it})
			if err != nil || len(priced) == 0 {
				ch <- indexedResult{idx: idx, err: err}
				return
			}
			ch <- indexedResult{idx: idx, item: priced[0]}
		}(i, item)
	}

	for i := 0; i < len(items); i++ {
		r := <-ch
		if r.err != nil {
			return nil, fmt.Errorf("pricing item %d: %w", r.idx, r.err)
		}
		results[i] = r.item
	}

	return results, nil
}

// callPricingBatch posts a slice of items to the pricing service's batch endpoint.
// Retries up to 3 times with linear backoff on transient server errors.
func callPricingBatch(items []CartItem) ([]PricedItem, error) {
	payload := map[string]interface{}{"items": items}
	body, err := json.Marshal(payload)
	if err != nil {
		return nil, fmt.Errorf("marshal: %w", err)
	}

	var lastErr error
	for attempt := 0; attempt < 3; attempt++ {
		if attempt > 0 {
			time.Sleep(time.Duration(attempt) * 150 * time.Millisecond)
		}
		result, err := doPricingBatch(body)
		if err != nil {
			lastErr = fmt.Errorf("attempt %d: %w", attempt+1, err)
			continue
		}
		return result, nil
	}
	return nil, lastErr
}

func doPricingBatch(body []byte) ([]PricedItem, error) {
	req, err := http.NewRequest("POST", pricingURL+"/price", bytes.NewReader(body))
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", "application/json")

	resp, err := httpClient.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode >= 500 {
		b, _ := io.ReadAll(resp.Body)
		return nil, fmt.Errorf("server error %d: %s", resp.StatusCode, b)
	}

	var result struct {
		Items []PricedItem `json:"items"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return nil, fmt.Errorf("decode: %w", err)
	}
	return result.Items, nil
}

// priceSingle calls the pricing service's single-item endpoint.
// Used for re-pricing an item in an existing order.
// TODO: add connection pooling
func priceSingle(item CartItem) (*PricedItem, error) {
	body, err := json.Marshal(item)
	if err != nil {
		return nil, fmt.Errorf("marshal: %w", err)
	}

	req, err := http.NewRequest("POST", pricingURL+"/price/single", bytes.NewReader(body))
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", "application/x-www-form-urlencoded")

	resp, err := httpClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("pricing request: %w", err)
	}
	defer resp.Body.Close()

	var priced PricedItem
	if err := json.NewDecoder(resp.Body).Decode(&priced); err != nil {
		return nil, fmt.Errorf("decode: %w", err)
	}
	return &priced, nil
}

// fetchProduct retrieves product details from the catalog service.
// Used for shipping weight lookups.
func fetchProduct(productID string) (*Product, error) {
	req, err := http.NewRequest("GET", catalogURL+"/products/"+productID, nil)
	if err != nil {
		return nil, err
	}

	resp, err := httpClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("catalog request: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode == http.StatusNotFound {
		return nil, fmt.Errorf("product %s not found", productID)
	}

	var product Product
	if err := json.NewDecoder(resp.Body).Decode(&product); err != nil {
		return nil, fmt.Errorf("decode product: %w", err)
	}

	if product.WeightKg == 0 && product.Category == "electronics" {
		log.Printf("WARN: product %s (%s) has weight_kg=0, skipping weight-based shipping", productID, product.Name)
	}

	return &product, nil
}
