# Extra Standard lib implementation in Rimera

## HTTP server
```py
# Rimera has an extra set of standard libraries.
# You can envoke them by importing from rimera

from rimera import Http, BaseModel

app = Http(port=4040)


class Item(BaseModel):
    name: str
    price: float
    is_offer: bool | None = None


@app.get("/")
def read_root():
    return {"message": "Welcome to Rimera"}


@app.get("/items/{item_id}")
def read_item(item_id: int, q: str | None = None):
    return {
        "item_id": item_id,
        "query_param": q,
    }


@app.post("/items/")
def create_item(item: Item):
    return {
        "status": "Item created successfully",
        "item": item,
    }
```

### You can also send status via Rimera http

```py
from rimera import Http, BaseModel, status

app = Http(port=4040)

@app.get("/health")
def health():
    return {"status": "ok"}

@app.get("/users/{id}")
def user(id: int):
    if id == 0:
        return status(404, {"error": "User not found"})

    return {"id": id}
```

## Native Enviroment Variables

```py
from rimera import env

PORT = env.int("PORT", default=8080)
DEBUG = env.bool("DEBUG", default=False)
```

## Rimera Fetch

```py
from rimera import requests

url = 'https://httpbin.org/post'
payload = {'username': 'johndoe', 'email': 'john@example.com'}
response = requests.post(url, json=payload)
print("Status Code:", response.status_code)

```
