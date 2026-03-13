import React, { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useStore } from "../store.js";

type Step = "shipping" | "payment" | "confirm";

export function Checkout() {
	const [step, setStep] = useState<Step>("shipping");
	const [error, setError] = useState<string | null>(null);
	const [orderId, setOrderId] = useState<string | null>(null);
	const setShippingAddress = useStore((s) => s.setShippingAddress);
	const submitOrder = useStore((s) => s.submitOrder);
	const navigate = useNavigate();

	const [shipping, setShipping] = useState({ name: "", address: "", city: "", zip: "" });
	const [payment, setPayment] = useState({ cardNumber: "" });

	const handleShippingNext = () => {
		if (!shipping.address || !shipping.city || !shipping.zip) {
			setError("All shipping fields are required");
			return;
		}
		setError(null);
		setShippingAddress(shipping);
		setStep("payment");
	};

	const handleSubmitOrder = async () => {
		try {
			setError(null);
			const result = await submitOrder();
			setOrderId(result.orderId);
			setStep("confirm");
		} catch (e) {
			setError(e instanceof Error ? e.message : "Order failed");
		}
	};

	if (step === "confirm") {
		return (
			<div data-testid="checkout-confirm">
				<h1>Order Confirmed!</h1>
				<p data-testid="order-id">Order ID: {orderId}</p>
				<button type="button" data-testid="continue-shopping" onClick={() => navigate("/")}>
					Continue Shopping
				</button>
			</div>
		);
	}

	return (
		<div data-testid="checkout-page">
			<h1>Checkout — {step === "shipping" ? "Shipping" : "Payment"}</h1>
			{error && <div data-testid="checkout-error" style={{ color: "red" }}>{error}</div>}

			{step === "shipping" && (
				<div data-testid="shipping-form">
					<input data-testid="shipping-name" placeholder="Full Name" value={shipping.name} onChange={(e) => setShipping((s) => ({ ...s, name: e.target.value }))} />
					<input data-testid="shipping-address" placeholder="Address" value={shipping.address} onChange={(e) => setShipping((s) => ({ ...s, address: e.target.value }))} />
					<input data-testid="shipping-city" placeholder="City" value={shipping.city} onChange={(e) => setShipping((s) => ({ ...s, city: e.target.value }))} />
					<input data-testid="shipping-zip" placeholder="ZIP" value={shipping.zip} onChange={(e) => setShipping((s) => ({ ...s, zip: e.target.value }))} />
					<button type="button" data-testid="next-step" onClick={handleShippingNext}>
						Next
					</button>
				</div>
			)}

			{step === "payment" && (
				<div data-testid="payment-form">
					<input data-testid="card-number" placeholder="Card Number" value={payment.cardNumber} onChange={(e) => setPayment({ cardNumber: e.target.value })} />
					<button type="button" data-testid="submit-order" onClick={handleSubmitOrder}>
						Place Order
					</button>
				</div>
			)}
		</div>
	);
}
