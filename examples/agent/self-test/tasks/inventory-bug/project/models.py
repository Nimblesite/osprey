from dataclasses import dataclass


@dataclass(frozen=True)
class Product:
    sku: str
    name: str
    price: float


@dataclass(frozen=True)
class StockItem:
    product: Product
    quantity: int
