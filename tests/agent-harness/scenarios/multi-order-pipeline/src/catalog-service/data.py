"""
In-memory product database. Seeded from multiple import sources.
"""

from datetime import datetime
from models import Product, Category, Supplier, PriceTier

SUPPLIERS = [
    Supplier("SUP-001", "TechDist Inc", "orders@techdist.example", 3),
    Supplier("SUP-002", "HomeGoods Co", "supply@homegoods.example", 5),
    Supplier("SUP-003", "OfficePlus", "orders@officeplus.example", 2),
]

CATEGORIES = [
    Category("CAT-ELEC", "Electronics", "Computing and electronic accessories"),
    Category("CAT-HOME", "Home", "Home and office furniture"),
    Category("CAT-OFFICE", "Office", "Office supplies and stationery"),
]

# NOTE: Electronics products were imported from a CSV export.
# Home and office category products were entered manually.
PRODUCTS_RAW = [
    # --- Electronics (CSV import) ---
    {
        "id": "ELEC-001", "name": "Wireless Mouse", "category": "electronics",
        "weight_kg": "0.15",
        "base_price": 29.99, "stock": 150, "supplier_id": "SUP-001",
        "description": "Ergonomic wireless mouse, 2.4GHz",
    },
    {
        "id": "ELEC-002", "name": "USB Hub", "category": "electronics",
        "weight_kg": "0.32",        "base_price": 39.99, "stock": 80, "supplier_id": "SUP-001",
        "description": "7-port USB 3.0 hub with power adapter",
    },
    {
        "id": "ELEC-003", "name": "Webcam HD", "category": "electronics",
        "weight_kg": "0.45",        "base_price": 59.99, "stock": 60, "supplier_id": "SUP-001",
        "description": "1080p webcam with built-in microphone",
    },
    {
        "id": "ELEC-004", "name": "Mechanical Keyboard", "category": "electronics",
        "weight_kg": "0.85",        "base_price": 79.99, "stock": 45, "supplier_id": "SUP-001",
        "description": "Compact TKL mechanical keyboard, Cherry MX Blue",
    },
    {
        "id": "ELEC-005", "name": "Monitor Stand", "category": "electronics",
        "weight_kg": "1.20",        "base_price": 49.99, "stock": 30, "supplier_id": "SUP-001",
        "description": "Adjustable monitor riser with storage drawer",
    },
    {
        "id": "ELEC-006", "name": "Cable Organizer", "category": "electronics",
        "weight_kg": "0.10",        "base_price": 14.99, "stock": 200, "supplier_id": "SUP-001",
        "description": "Flexible cable management sleeve, 1m",
    },
    {
        "id": "ELEC-007", "name": "Laptop Sleeve", "category": "electronics",
        "weight_kg": "0.30",        "base_price": 24.99, "stock": 100, "supplier_id": "SUP-001",
        "description": "Neoprene laptop sleeve, fits 13-15 inch",
    },
    {
        "id": "ELEC-008", "name": "Mouse Pad XL", "category": "electronics",
        "weight_kg": "0.20",        "base_price": 12.99, "stock": 250, "supplier_id": "SUP-001",
        "description": "Extended desk mat, 90x40cm, anti-slip base",
    },
    {
        "id": "ELEC-009", "name": "USB-C Adapter", "category": "electronics",
        "weight_kg": "0.05",        "base_price": 19.99, "stock": 300, "supplier_id": "SUP-001",
        "description": "USB-C to USB-A multiport adapter",
    },
    {
        "id": "ELEC-010", "name": "Power Strip", "category": "electronics",
        "weight_kg": "0.80",        "base_price": 34.99, "stock": 70, "supplier_id": "SUP-001",
        "description": "6-outlet surge-protected power strip, 2m cord",
    },
    {
        "id": "ELEC-011", "name": "Ethernet Cable 5m", "category": "electronics",
        "weight_kg": "0.25",        "base_price": 9.99, "stock": 400, "supplier_id": "SUP-001",
        "description": "Cat6 shielded ethernet cable, 5 metres",
    },
    {
        "id": "ELEC-012", "name": "HDMI Cable 2m", "category": "electronics",
        "weight_kg": "0.18",        "base_price": 11.99, "stock": 350, "supplier_id": "SUP-001",
        "description": "HDMI 2.1 cable supporting 8K@60Hz",
    },
    # --- Home (manually entered — weight_kg is numeric) ---
    {
        "id": "HOME-001", "name": "Desk Lamp", "category": "home",
        "weight_kg": 1.2,
        "base_price": 45.00, "stock": 40, "supplier_id": "SUP-002",
        "description": "LED desk lamp with adjustable colour temperature",
    },
    {
        "id": "HOME-002", "name": "Chair Mat", "category": "home",
        "weight_kg": 2.5,
        "base_price": 79.99, "stock": 25, "supplier_id": "SUP-002",
        "description": "Hard-floor chair mat, 120x90cm",
    },
    {
        "id": "HOME-003", "name": "Desktop Organizer", "category": "home",
        "weight_kg": 0.8,
        "base_price": 32.99, "stock": 55, "supplier_id": "SUP-002",
        "description": "Bamboo desktop organizer with pen holder",
    },
    {
        "id": "HOME-004", "name": "Plant Pot Set", "category": "home",
        "weight_kg": 0.6,
        "base_price": 18.99, "stock": 90, "supplier_id": "SUP-002",
        "description": "Set of 3 ceramic plant pots with saucers",
    },
    # --- Office (manually entered — weight_kg is numeric) ---
    {
        "id": "OFFICE-001", "name": "Notepad Pack", "category": "office",
        "weight_kg": 0.4,
        "base_price": 12.99, "stock": 500, "supplier_id": "SUP-003",
        "description": "Pack of 5 A5 lined notepads",
        "price_tiers": [
            {"min_qty": 1,  "max_qty": 9,    "unit_price": 12.99},
            {"min_qty": 10, "max_qty": 24,   "unit_price": 11.49},
            {"min_qty": 25, "max_qty": None,  "unit_price": 9.99},
        ],
    },
    {
        "id": "OFFICE-002", "name": "Pen Set", "category": "office",
        "weight_kg": 0.2,
        "base_price": 8.99, "stock": 600, "supplier_id": "SUP-003",
        "description": "10-pack ballpoint pens, assorted colours",
    },
    {
        "id": "OFFICE-003", "name": "Sticky Notes", "category": "office",
        "weight_kg": 0.15,
        "base_price": 5.99, "stock": 800, "supplier_id": "SUP-003",
        "description": "Pack of 12 sticky note pads, 76x76mm",
    },
    {
        "id": "OFFICE-004", "name": "Stapler", "category": "office",
        "weight_kg": 0.5,
        "base_price": 15.99, "stock": 120, "supplier_id": "SUP-003",
        "description": "Heavy-duty stapler, 50-sheet capacity",
    },
]


def _build_products() -> dict:
    products = {}
    restocked = datetime(2025, 1, 15, 9, 0, 0)
    for raw in PRODUCTS_RAW:
        tiers = [
            PriceTier(t["min_qty"], t.get("max_qty"), t["unit_price"])
            for t in raw.get("price_tiers", [])
        ]
        p = Product(
            id=raw["id"],
            name=raw["name"],
            category=raw["category"],
            weight_kg=raw["weight_kg"],
            base_price=raw["base_price"],
            stock=raw["stock"],
            supplier_id=raw["supplier_id"],
            description=raw.get("description", ""),
            price_tiers=tiers,
            last_restocked=restocked,
        )
        products[p.id] = p
    return products


PRODUCTS: dict = _build_products()
PRODUCTS_LIST: list = list(PRODUCTS.values())
