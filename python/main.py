from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

app = FastAPI(title="Tauri FastAPI Backend")

app.add_middleware(
    CORSMiddleware,
    # Limit origins so arbitrary websites cannot call localhost APIs.
    allow_origins=[
        "tauri://localhost",
        "http://localhost:1420",
        "http://127.0.0.1:1420",
    ],
    allow_methods=["*"],
    allow_headers=["*"],
)


@app.get("/")
def root():
    return {"status": "ok", "message": "FastAPI backend is running"}


@app.get("/hello/{name}")
def hello(name: str):
    return {"message": f"Hello, {name}! From FastAPI!"}
