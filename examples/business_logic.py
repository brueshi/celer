def apply_discount(price: int, threshold: int) -> int:
    if price > threshold:
        return price * 90 // 100
    return price

def calculate_price(base_price: int) -> dict:
    final_price = apply_discount(base_price, 50)
    return {"price": final_price, "currency": "USD"}
