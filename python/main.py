from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

app = FastAPI(title="Tauri FastAPI Backend")

app.add_middleware(
    CORSMiddleware,
    # Allow Tauri app origins and local dev origins on any port.
    # CORS checks the request Origin (frontend), not backend API port.
    allow_origins=[
        "tauri://localhost",
        "http://tauri.localhost",
        "https://tauri.localhost",
    ],
    allow_origin_regex=r"^https?://(localhost|127\.0\.0\.1)(:\d+)?$",
    allow_methods=["*"],
    allow_headers=["*"],
)


@app.get("/")
def root():
    return {"status": "ok", "message": "FastAPI backend is running"}


@app.get("/hello/{name}")
def hello(name: str):
    return {"message": f"Hello, {name}! From FastAPI!"}
