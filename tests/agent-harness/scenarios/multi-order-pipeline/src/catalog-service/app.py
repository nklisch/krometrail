"""
Product Catalog Service — Flask HTTP API.
Serves product data, inventory status, and category listings.
"""

import os
from flask import Flask, jsonify, request
from data import PRODUCTS, PRODUCTS_LIST, CATEGORIES, SUPPLIERS
from inventory import get_available_stock, stock_summary

app = Flask(__name__)

PER_PAGE = 10


def _filter_products(category: str | None) -> list:
    if category:
        return [p for p in PRODUCTS_LIST if p.category == category]
    return list(PRODUCTS_LIST)


@app.route("/health")
def health():
    return jsonify({"status": "ok", "products": len(PRODUCTS)})


@app.route("/products")
def list_products():
    category = request.args.get("category") or None
    page = max(1, request.args.get("page", 1, type=int))

    filtered = _filter_products(category)
    total = len(filtered)

    start = (page - 1) * PER_PAGE
    end = start + PER_PAGE
    page_products = filtered[start:end]
    has_more = end < total

    next_page = f"/products?page={page + 1}" if has_more else None

    return jsonify({
        "products": [p.to_dict() for p in page_products],
        "page": page,
        "total": total,
        "per_page": PER_PAGE,
        "next_page": next_page,
    })


@app.route("/products/<product_id>")
def get_product(product_id: str):
    product = PRODUCTS.get(product_id)
    if not product:
        return jsonify({"error": "product not found"}), 404

    quantity = request.args.get("quantity", 1, type=int)
    data = product.to_dict(quantity=quantity)
    data["available_stock"] = get_available_stock(product_id)
    return jsonify(data)


@app.route("/categories")
def list_categories():
    return jsonify([
        {"id": c.id, "name": c.name, "description": c.description}
        for c in CATEGORIES
    ])


@app.route("/suppliers")
def list_suppliers():
    return jsonify([
        {"id": s.id, "name": s.name, "lead_time_days": s.lead_time_days}
        for s in SUPPLIERS
    ])


@app.route("/inventory")
def inventory():
    return jsonify(stock_summary())


if __name__ == "__main__":
    port = int(os.environ.get("PORT", 5001))
    app.run(host="0.0.0.0", port=port, debug=False)
