import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

function App() {
  const [apiPort, setApiPort] = useState<number | null>(null);
  const [apiResponse, setApiResponse] = useState("");
  const [name, setName] = useState("");
  const [loading, setLoading] = useState(false);

  // アプリ起動時に FastAPI のポート番号を取得
  useEffect(() => {
    invoke<number>("get_api_port").then(setApiPort);
  }, []);

  async function callFastApi() {
    if (!apiPort || !name) return;
    setLoading(true);
    try {
      const res = await fetch(`http://127.0.0.1:${apiPort}/hello/${encodeURIComponent(name)}`);
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
        {apiPort ? (
          <span style={{ color: "green" }}>Running on port {apiPort}</span>
        ) : (
          <span style={{ color: "gray" }}>Loading...</span>
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
        <button type="submit" disabled={loading || !apiPort}>
          {loading ? "..." : "Call FastAPI"}
        </button>
      </form>

      {apiResponse && <p>{apiResponse}</p>}
    </main>
  );
}

export default App;
