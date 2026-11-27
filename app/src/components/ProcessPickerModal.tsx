import { useEffect, useState } from "react";
import { api, ProcessDto } from "../api";

interface Props {
  open: boolean;
  onClose: () => void;
  onSelect: (proc: ProcessDto) => void;
}

export function ProcessPickerModal({ open, onClose, onSelect }: Props) {
  const [processes, setProcesses] = useState<ProcessDto[]>([]);
  const [query, setQuery] = useState("");
  const [includeSystem, setIncludeSystem] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = async (showSys: boolean) => {
    setLoading(true);
    setError(null);
    try {
      const list = await api.listRunningProcesses(showSys);
      setProcesses(list);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (open) {
      setQuery("");
      load(includeSystem);
    }
  }, [open, includeSystem]);

  useEffect(() => {
    if (!open) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [open, onClose]);

  if (!open) return null;

  const q = query.trim().toLowerCase();
  const filtered = processes.filter(
    (p) =>
      !q ||
      p.name.toLowerCase().includes(q) ||
      (p.window_title && p.window_title.toLowerCase().includes(q)) ||
      String(p.pid) === q
  );

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="row" style={{ justifyContent: "space-between", marginBottom: 12 }}>
          <div>
            <h2 style={{ margin: 0 }}>Select Running Game / App</h2>
            <div className="muted">
              Showing user apps & games, including Steam / Proton titles (system processes hidden by default).
            </div>
          </div>
          <button className="btn btn-secondary" onClick={onClose}>
            Close
          </button>
        </div>

        <div className="row" style={{ marginBottom: 10 }}>
          <input
            className="input"
            autoFocus
            placeholder="Filter by game title, .exe name, or PID..."
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          <button className="btn btn-secondary" onClick={() => load(includeSystem)}>
            Refresh
          </button>
        </div>

        <label
          style={{
            flexDirection: "row",
            alignItems: "center",
            gap: 8,
            marginBottom: 10,
            cursor: "pointer",
          }}
        >
          <input
            type="checkbox"
            checked={includeSystem}
            onChange={(e) => setIncludeSystem(e.target.checked)}
          />
          <span>Show OS system processes</span>
        </label>

        {error && <div className="banner error">{error}</div>}

        <div className="process-list">
          {loading ? (
            <div className="muted" style={{ padding: 12 }}>
              Scanning running processes...
            </div>
          ) : filtered.length === 0 ? (
            <div className="muted" style={{ padding: 12 }}>
              No matching processes found. Launch your game and click Refresh.
            </div>
          ) : (
            filtered.map((p) => (
              <button
                key={`${p.pid}-${p.name}`}
                className="process-item"
                onClick={() => {
                  onSelect(p);
                  onClose();
                }}
              >
                <div style={{ textAlign: "left" }}>
                  <div className="row">
                    <strong>{p.window_title || p.name}</strong>
                    {p.window_title && (
                      <span className="badge badge-success">
                        {p.window_title.endsWith("(Proton)")
                          ? "Proton"
                          : p.window_title.endsWith("(Steam)")
                            ? "Steam"
                            : "Window"}
                      </span>
                    )}
                  </div>
                  <div className="muted">
                    <code>{p.name}</code> · PID {p.pid}
                  </div>
                </div>
                <span className="btn btn-secondary" style={{ pointerEvents: "none" }}>
                  Select
                </span>
              </button>
            ))
          )}
        </div>
      </div>
    </div>
  );
}
