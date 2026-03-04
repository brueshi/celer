from fastapi import FastAPI

app = FastAPI()

@app.get("/")
def root() -> dict:
    return {"message": "hello", "framework": "celer"}

@app.get("/health")
def health() -> dict:
    return {"status": "ok"}

@app.get("/items/{item_id}")
def get_item(item_id: int) -> dict:
    return {"item_id": item_id, "name": "test"}

@app.get("/compute/{n}")
def compute(n: int) -> dict:
    result = 0
    i = 0
    while i < n:
        result = result + i
        i = i + 1
    return {"result": result}
