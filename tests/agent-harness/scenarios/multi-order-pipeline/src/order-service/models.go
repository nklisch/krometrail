package main

import (
	"fmt"
	"sync"
)

// CartItem represents a product and quantity in a customer's cart.
type CartItem struct {
	ProductID string `json:"productId"`
	Quantity  int    `json:"quantity"`
}

// PricedItem is what the pricing service returns for a cart item.
type PricedItem struct {
	ProductID  string  `json:"productId"`
	Quantity   int     `json:"quantity"`
	BasePrice  float64 `json:"basePrice"`
	Discount   float64 `json:"discount"`
	FinalPrice float64 `json:"finalPrice"` // pre-computed by pricing service
	Tax        float64 `json:"tax"`
}

// Product is a product record from the catalog service.
type Product struct {
	ID       string  `json:"id"`
	Name     string  `json:"name"`
	Category string  `json:"category"`
	WeightKg float64 `json:"weight_kg"`
	Stock    int     `json:"stock"`
}

// LineItem is a line in a confirmed order.
type LineItem struct {
	ProductID string  `json:"productId"`
	Quantity  int     `json:"quantity"`
	UnitPrice float64 `json:"unitPrice"`
	LineTotal float64 `json:"lineTotal"`
}

// Order is the response to a successful order creation.
type Order struct {
	OrderID      string     `json:"orderId"`
	Status       string     `json:"status"`
	Items        []LineItem `json:"items"`
	Subtotal     float64    `json:"subtotal"`
	ShippingCost float64    `json:"shippingCost"`
	Tax          float64    `json:"tax"`
	Total        float64    `json:"total"`
}

// orderCounter generates sequential order IDs.
// Protected by a mutex to be safe under concurrent requests.
var (
	orderMu      sync.Mutex
	orderCounter int
)

func nextOrderID() string {
	orderMu.Lock()
	defer orderMu.Unlock()
	orderCounter++
	return fmt.Sprintf("ORD-%06d", orderCounter)
}
