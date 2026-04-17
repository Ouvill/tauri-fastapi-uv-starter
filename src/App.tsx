import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

type BootstrapStatus = {
  phase: string;
  message: string;
};

function App() {
  const [apiPort, setApiPort] = useState<number | null>(null);
  const [backendRunning, setBackendRunning] = useState<boolean | null>(null);
  const [bootstrapStatus, setBootstrapStatus] = useState<BootstrapStatus>({
    phase: "initializing",
    message: "Preparing Python environment...",
  });
  const [apiResponse, setApiResponse] = useState("");
  const [name, setName] = useState("");
  const [loading, setLoading] = useState(false);

  // 起動時にバックエンド準備状態をポーリング
  useEffect(() => {
    let cancelled = false;

    const refresh = async () => {
      try {
        const status = await invoke<BootstrapStatus>("get_backend_bootstrap_status");
        const running = await invoke<boolean>("is_backend_running");
        const port = await invoke<number | null>("get_api_port");
        if (!cancelled) {
          setBootstrapStatus(status);
          setBackendRunning(running);
          setApiPort(port);
        }
      } catch (_e) {
        if (!cancelled) {
          setBootstrapStatus({
            phase: "failed",
            message: "Failed to query backend status.",
          });
          setBackendRunning(false);
          setApiPort(null);
        }
      }
    };

    refresh();
    const timerId = window.setInterval(refresh, 1200);
    return () => {
      cancelled = true;
      window.clearInterval(timerId);
    };
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
        {bootstrapStatus.phase === "syncing" || bootstrapStatus.phase === "starting" || bootstrapStatus.phase === "initializing" ? (
          <span style={{ color: "gray" }}>{bootstrapStatus.message}</span>
        ) : bootstrapStatus.phase === "failed" ? (
          <span style={{ color: "red" }}>{bootstrapStatus.message}</span>
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
