from fastapi import FastAPI

app = FastAPI()

@app.get("/")
def root() -> dict:
    return {"message": "hello"}

@app.get("/items/{item_id}")
def get_item(item_id: int) -> dict:
    return {"item_id": item_id, "name": "test"}
