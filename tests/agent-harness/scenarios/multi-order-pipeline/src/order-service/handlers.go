package main

import (
	"encoding/json"
	"log"
	"math"
	"net/http"
	"strings"
)

type Handler struct{}

func NewHandler() *Handler { return &Handler{} }

func (h *Handler) health(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]string{"status": "ok"})
}

func (h *Handler) handleOrder(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}

	var req struct {
		Items []CartItem `json:"items"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, "invalid request body", http.StatusBadRequest)
		return
	}
	if len(req.Items) == 0 {
		http.Error(w, "items required", http.StatusBadRequest)
		return
	}

	// Price all cart items concurrently via the pricing service
	pricedItems, err := priceItems(req.Items)
	if err != nil {
		log.Printf("priceItems error: %v", err)
		http.Error(w, "pricing failed", http.StatusInternalServerError)
		return
	}

	var lineItems []LineItem
	var subtotal float64

	for i, cartItem := range req.Items {
		priced := pricedItems[i]

		// discount is a dollar amount off the base price.
		unitPrice := applyDiscount(priced.BasePrice, priced.Discount)
		lineTotal := math.Round(unitPrice*float64(cartItem.Quantity)*100) / 100

		lineItems = append(lineItems, LineItem{
			ProductID: cartItem.ProductID,
			Quantity:  cartItem.Quantity,
			UnitPrice: unitPrice,
			LineTotal: lineTotal,
		})
		subtotal += lineTotal
	}
	subtotal = math.Round(subtotal*100) / 100

	// Compute shipping from catalog product weights
	shippingCost, err := computeOrderShipping(req.Items)
	if err != nil {
		log.Printf("shipping error: %v", err)
		// non-fatal — continue with $0 shipping
	}

	tax := math.Round(subtotal*0.08*100) / 100
	total := math.Round((subtotal+shippingCost+tax)*100) / 100

	order := Order{
		OrderID:      nextOrderID(),
		Status:       "confirmed",
		Items:        lineItems,
		Subtotal:     subtotal,
		ShippingCost: shippingCost,
		Tax:          tax,
		Total:        total,
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(order)
}

func (h *Handler) handleOrderByID(w http.ResponseWriter, r *http.Request) {
	path := strings.TrimPrefix(r.URL.Path, "/orders/")
	parts := strings.SplitN(path, "/", 2)

	if len(parts) == 2 && parts[1] == "reprice" {
		h.repriceItem(w, r)
		return
	}

	// GET /orders/:id — stub status endpoint
	orderID := parts[0]
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]string{
		"orderId": orderID,
		"status":  "confirmed",
	})
}

func (h *Handler) repriceItem(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}

	var item CartItem
	if err := json.NewDecoder(r.Body).Decode(&item); err != nil {
		http.Error(w, "invalid request body", http.StatusBadRequest)
		return
	}

	// Use the single-item pricing endpoint for re-pricing
	priced, err := priceSingle(item)
	if err != nil {
		log.Printf("priceSingle error: %v", err)
		http.Error(w, "repricing failed", http.StatusInternalServerError)
		return
	}

	unitPrice := applyDiscount(priced.BasePrice, priced.Discount)

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]interface{}{
		"productId":    item.ProductID,
		"quantity":     item.Quantity,
		"unitPrice":    unitPrice,
		"currentPrice": priced.FinalPrice,
	})
}

// applyDiscount subtracts a discount from the base price.
// discount is a dollar amount off the base price.
func applyDiscount(basePrice, discount float64) float64 {
	result := basePrice - discount
	if result < 0 {
		result = 0
	}
	return math.Round(result*100) / 100
}

// computeOrderShipping sums shipping costs for all items in the order.
func computeOrderShipping(items []CartItem) (float64, error) {
	var total float64
	for _, item := range items {
		product, err := fetchProduct(item.ProductID)
		if err != nil {
			log.Printf("fetchProduct %s: %v", item.ProductID, err)
			continue
		}
		total += calculateShipping(product.WeightKg, "standard")
	}
	return math.Round(total*100) / 100, nil
}
