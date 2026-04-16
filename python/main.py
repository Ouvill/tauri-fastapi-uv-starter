from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

app = FastAPI(title="Tauri FastAPI Backend")

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],  # tauri://localhost and http://localhost:*
    allow_methods=["*"],
    allow_headers=["*"],
)


@app.get("/")
def root():
    return {"status": "ok", "message": "FastAPI backend is running"}


@app.get("/hello/{name}")
def hello(name: str):
    return {"message": f"Hello, {name}! From FastAPI!"}
