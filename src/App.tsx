import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

function App() {
  const [apiPort, setApiPort] = useState<number | null>(null);
  const [backendRunning, setBackendRunning] = useState<boolean | null>(null);
  const [apiResponse, setApiResponse] = useState("");
  const [name, setName] = useState("");
  const [loading, setLoading] = useState(false);

  // アプリ起動時に FastAPI のポート番号を取得
  useEffect(() => {
    invoke<number | null>("get_api_port")
      .then(setApiPort)
      .catch(() => setApiPort(null));

    invoke<boolean>("is_backend_running")
      .then(setBackendRunning)
      .catch(() => setBackendRunning(false));
  }, []);

  async function callFastApi() {
    if (apiPort === null || !name || backendRunning !== true) return;
    setLoading(true);
    try {
      const res = await fetch(`http://127.0.0.1:${apiPort}/hello/${encodeURIComponent(name)}`);
      if (!res.ok) {
        throw new Error(`HTTP ${res.status}`);
      }
      const data = await res.json();
      setApiResponse(data.message);
    } catch (e) {
      setApiResponse(`Error: ${e}`);
    } finally {
      setLoading(false);
    }
  }

  return (
    <main className="container">
      <h1>Tauri + FastAPI</h1>

      <p>
        Backend status:{" "}
        {backendRunning === null ? (
          <span style={{ color: "gray" }}>Checking...</span>
        ) : backendRunning ? (
          <span style={{ color: "green" }}>
            {apiPort !== null ? `Running on port ${apiPort}` : "Running"}
          </span>
        ) : (
          <span style={{ color: "red" }}>Failed to start backend</span>
        )}
      </p>

      <form
        className="row"
        onSubmit={(e) => {
          e.preventDefault();
          callFastApi();
        }}
      >
        <input
          onChange={(e) => setName(e.currentTarget.value)}
          placeholder="Enter a name..."
        />
        <button type="submit" disabled={loading || apiPort === null || backendRunning !== true}>
          {loading ? "..." : "Call FastAPI"}
        </button>
      </form>

      {apiResponse && <p>{apiResponse}</p>}
    </main>
  );
}

export default App;
