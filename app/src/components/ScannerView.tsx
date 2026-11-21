import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  api,
  CapturedEntryInput,
  ProcessDto,
  ScanHitDto,
  ScannerExportResult,
  ScanValueTypeName,
} from "../api";
import { ProcessPickerModal } from "./ProcessPickerModal";

interface ScanProgress {
  phase: "first" | "next";
  current: number;
  total: number;
  regions_done: number;
  regions_total: number;
  hits_found: number;
  percent: number;
}

function formatMb(bytes: number): string {
  return (bytes / (1024 * 1024)).toFixed(1);
}

interface Props {
  onOpenTrainer: (trainerId: string) => void;
}

function downloadText(filename: string, text: string) {
  const blob = new Blob([text], { type: "text/plain;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}

export function ScannerView({ onOpenTrainer }: Props) {
  const [pickerOpen, setPickerOpen] = useState(false);
  const [attachedProc, setAttachedProc] = useState<ProcessDto | null>(null);

  // Scan controls
  const [valueType, setValueType] = useState<ScanValueTypeName>("all");
  const [scanMode, setScanMode] = useState<
    "exact" | "increased" | "decreased" | "changed" | "unchanged"
  >("exact");
  const [scanValue, setScanValue] = useState("");
  const [hasFirstScan, setHasFirstScan] = useState(false);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<ScanProgress | null>(null);
  const [totalHits, setTotalHits] = useState(0);

  useEffect(() => {
    const params =
      typeof window !== "undefined" ? new URLSearchParams(window.location.search) : null;
    if (params?.get("demo") === "1") {
      setAttachedProc({
        pid: 23044,
        name: "WarhammerSurvivors.exe",
        window_title: "Warhammer Survivors",
        is_system: false,
      });
      setGameTitle("Warhammer Survivors");
      setTrainerId("warhammer-survivors");
      setScanValue("100");
      setHasFirstScan(true);
      setTotalHits(3);
      setProgress({
        phase: "first",
        current: 486 * 1024 * 1024,
        total: 486 * 1024 * 1024,
        regions_done: 612,
        regions_total: 612,
        hits_found: 3,
        percent: 100,
      });
      setPreview([
        {
          address: "0x7FF6A14028C4",
          value_type: "f32",
          value: "100.000",
          static_ref: "GameAssembly.dll+0x4B28C4",
        },
        {
          address: "0x1E4B90C11084",
          value_type: "i32",
          value: "100",
        },
        {
          address: "0x1E4B90C12400",
          value_type: "f64",
          value: "100.0000",
        },
      ]);
      setCaptured([
        {
          label: "Current Coins",
          category: "Player",
          address: "0x7FF6A14028C4",
          value_type: "f32",
          default_value: 99999,
          freeze: false,
        },
      ]);
    }
    let unlisten: (() => void) | undefined;
    listen<ScanProgress>("scanner-progress", (ev) => {
      setProgress(ev.payload);
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch(() => {
        // Ignore outside Tauri shell
      });
    return () => {
      if (unlisten) unlisten();
    };
  }, []);
  const [preview, setPreview] = useState<ScanHitDto[]>([]);
  const [editValues, setEditValues] = useState<Record<string, string>>({});

  // Captured entries & trainer metadata for export / sharing
  const [captured, setCaptured] = useState<CapturedEntryInput[]>([]);
  const [trainerId, setTrainerId] = useState("my-game-trainer");
  const [gameTitle, setGameTitle] = useState("My Single-Player Game");
  const [author, setAuthor] = useState("community");
  const [version, setVersion] = useState("1.0.0");
  const [steamAppId, setSteamAppId] = useState("");
  const [description, setDescription] = useState("");
  const [exportResult, setExportResult] = useState<ScannerExportResult | null>(null);

  const [message, setMessage] = useState<{ kind: "success" | "error"; text: string } | null>(null);

  const handlePickProcess = async (proc: ProcessDto) => {
    setMessage(null);
    try {
      await api.scannerAttach(proc.pid, proc.name);
      setAttachedProc(proc);
      setHasFirstScan(false);
      setTotalHits(0);
      setPreview([]);
      setCaptured([]);
      setExportResult(null);
      if (proc.window_title) {
        setGameTitle(proc.window_title);
        const slug = proc.window_title
          .toLowerCase()
          .replace(/[^a-z0-9]+/g, "-")
          .replace(/^-|-$/g, "");
        if (slug) setTrainerId(slug);
      }
      setMessage({
        kind: "success",
        text: `Scanner attached to ${proc.name} (PID ${proc.pid}). Enter a value to First Scan.`,
      });
    } catch (e) {
      setMessage({ kind: "error", text: String(e) });
    }
  };

  const handleFirstScan = async () => {
    const num = Number(scanValue);
    if (!scanValue.trim() || Number.isNaN(num)) {
      setMessage({ kind: "error", text: "Enter a valid number to scan for." });
      return;
    }
    setBusy(true);
    setMessage(null);
    try {
      const res = await api.scannerFirstScan(valueType, num);
      setTotalHits(res.total_hits);
      setPreview(res.preview);
      setHasFirstScan(true);
      setMessage({
        kind: "success",
        text: `First Scan found ${res.total_hits.toLocaleString()} address(es). Change the value in-game, then click Next Scan.`,
      });
    } catch (e) {
      setMessage({ kind: "error", text: String(e) });
    } finally {
      setBusy(false);
    }
  };

  const handleNextScan = async () => {
    let num: number | undefined = undefined;
    if (scanMode === "exact") {
      num = Number(scanValue);
      if (!scanValue.trim() || Number.isNaN(num)) {
        setMessage({ kind: "error", text: "Enter the new exact value." });
        return;
      }
    }
    setBusy(true);
    setMessage(null);
    try {
      const res = await api.scannerNextScan(scanMode, valueType, num);
      setTotalHits(res.total_hits);
      setPreview(res.preview);
    } catch (e) {
      setMessage({ kind: "error", text: String(e) });
    } finally {
      setBusy(false);
    }
  };

  const handleRefresh = async () => {
    try {
      const res = await api.scannerRefreshPreview();
      setTotalHits(res.total_hits);
      setPreview(res.preview);
    } catch (e) {
      setMessage({ kind: "error", text: String(e) });
    }
  };

  const handleTestWrite = async (hit: ScanHitDto) => {
    const key = `${hit.address}:${hit.value_type}`;
    const raw = editValues[key] ?? hit.value;
    const num = Number(raw);
    if (Number.isNaN(num)) {
      setMessage({ kind: "error", text: "Enter a valid number to write." });
      return;
    }
    try {
      await api.scannerWriteAddress(hit.address, hit.value_type, num);
      setMessage({
        kind: "success",
        text: `Wrote ${num} (${hit.value_type}) to ${hit.address}.`,
      });
      await handleRefresh();
    } catch (e) {
      setMessage({ kind: "error", text: String(e) });
    }
  };

  const handleCapture = (hit: ScanHitDto) => {
    if (captured.some((c) => c.address === hit.address && c.value_type === hit.value_type)) return;
    const key = `${hit.address}:${hit.value_type}`;
    const defaultNum = Number(editValues[key] ?? hit.value);
    setCaptured((prev) => [
      ...prev,
      {
        label: `Value ${prev.length + 1}`,
        category: "Player",
        address: hit.address,
        value_type: hit.value_type,
        default_value: Number.isNaN(defaultNum) ? undefined : defaultNum,
        freeze: false,
      },
    ]);
  };

  const handleExport = async (saveToLibrary: boolean) => {
    if (!attachedProc) return;
    if (captured.length === 0) {
      setMessage({ kind: "error", text: "Capture at least one address before exporting." });
      return;
    }
    setMessage(null);
    try {
      const res = await api.scannerExportTrainer({
        trainerId,
        gameTitle,
        processName: attachedProc.name,
        author,
        version,
        steamAppId: steamAppId.trim() ? Number(steamAppId) : undefined,
        description: description.trim() || undefined,
        entries: captured,
        saveToLibrary,
      });
      setExportResult(res);
      setMessage({
        kind: "success",
        text: saveToLibrary
          ? `Saved "${gameTitle}" to your Library and generated .CT + .trainer.json (SHA-256 ready for repos).`
          : `Generated .CT + .trainer.json + repo index.json snippet.`,
      });
    } catch (e) {
      setMessage({ kind: "error", text: String(e) });
    }
  };

  return (
    <div>
      <div className="header-row">
        <div>
          <h1>Live Value Scanner</h1>
          <div className="muted">
            Find values in a running single-player game, test them live, and export as{" "}
            <code>.CT</code> or <code>.trainer.json</code> to share in community repos.
          </div>
        </div>
        <button className="btn" onClick={() => setPickerOpen(true)}>
          {attachedProc ? `Change Process (${attachedProc.name})` : "Pick Game Process..."}
        </button>
      </div>

      {message && <div className={`banner ${message.kind}`}>{message.text}</div>}

      <div className="card" style={{ marginBottom: 16 }}>
        <div className="form-grid" style={{ gridTemplateColumns: "1fr 140px 160px" }}>
          <label>
            Value
            <input
              className="input"
              placeholder="e.g. 100"
              value={scanValue}
              disabled={!attachedProc || (hasFirstScan && scanMode !== "exact")}
              onChange={(e) => setScanValue(e.target.value)}
            />
          </label>
          <label>
            Value Type
            <select
              className="select"
              value={valueType}
              disabled={hasFirstScan}
              onChange={(e) => setValueType(e.target.value as ScanValueTypeName)}
            >
              <option value="all">All (4 Bytes + Float + Double)</option>
              <option value="i32">4 Bytes (i32)</option>
              <option value="f32">Float (f32)</option>
              <option value="f64">Double (f64)</option>
              <option value="i64">8 Bytes (i64)</option>
              <option value="i16">2 Bytes (i16)</option>
              <option value="u8">Byte (u8)</option>
            </select>
          </label>
          <label>
            Next Scan Mode
            <select
              className="select"
              value={scanMode}
              disabled={!hasFirstScan}
              onChange={(e) => setScanMode(e.target.value as typeof scanMode)}
            >
              <option value="exact">Exact Value</option>
              <option value="increased">Increased</option>
              <option value="decreased">Decreased</option>
              <option value="changed">Changed</option>
              <option value="unchanged">Unchanged</option>
            </select>
          </label>
        </div>

        <div className="row">
          <button
            className="btn"
            disabled={!attachedProc || busy}
            onClick={handleFirstScan}
          >
            {busy ? "Scanning..." : hasFirstScan ? "New Scan" : "First Scan"}
          </button>
          <button
            className="btn btn-secondary"
            disabled={!attachedProc || !hasFirstScan || busy}
            onClick={handleNextScan}
          >
            Next Scan
          </button>
          <button
            className="btn btn-secondary"
            disabled={!attachedProc || !hasFirstScan || busy}
            onClick={handleRefresh}
          >
            Refresh Values
          </button>
          {hasFirstScan && (
            <span className="muted" style={{ marginLeft: "auto" }}>
              Found: <strong>{totalHits.toLocaleString()}</strong> (showing top {preview.length})
            </span>
          )}
        </div>

        {(busy || progress) && progress && (
          <div className="progress-wrap">
            <div className="row" style={{ justifyContent: "space-between" }}>
              <span className="muted">
                {progress.phase === "first"
                  ? `Scanned ${progress.regions_done.toLocaleString()} / ${progress.regions_total.toLocaleString()} memory regions (${formatMb(progress.current)} / ${formatMb(progress.total)} MB)`
                  : `Checked ${progress.current.toLocaleString()} / ${progress.total.toLocaleString()} addresses`}
              </span>
              <span className="muted">
                <strong>{progress.percent.toFixed(0)}%</strong> ·{" "}
                {progress.hits_found.toLocaleString()} hits
              </span>
            </div>
            <div className="progress-track">
              <div
                className="progress-fill"
                style={{ width: `${Math.min(100, Math.max(0, progress.percent))}%` }}
              />
            </div>
          </div>
        )}
      </div>

      {preview.length > 0 && (
        <div className="card" style={{ marginBottom: 16 }}>
          <h2>Scan Results</h2>
          <div style={{ maxHeight: 240, overflowY: "auto" }}>
            {preview.map((hit) => {
              const key = `${hit.address}:${hit.value_type}`;
              return (
                <div className="entry-row" key={key}>
                  <div>
                    <div className="row">
                      <code>{hit.address}</code>
                      <span className="badge">{hit.value_type}</span>
                      {hit.static_ref && (
                        <span className="badge badge-success">Static: {hit.static_ref}</span>
                      )}
                    </div>
                    <div className="muted">
                      Current: <strong>{hit.value}</strong>
                      {hit.previous !== undefined && ` (prev: ${hit.previous})`}
                    </div>
                  </div>
                  <div className="row">
                    <input
                      className="input"
                      style={{ width: 100 }}
                      placeholder={hit.value}
                      value={editValues[key] ?? ""}
                      onChange={(e) =>
                        setEditValues((prev) => ({ ...prev, [key]: e.target.value }))
                      }
                    />
                    <button className="btn btn-secondary" onClick={() => handleTestWrite(hit)}>
                      Test Set
                    </button>
                    <button className="btn" onClick={() => handleCapture(hit)}>
                      + Capture
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      )}

      {captured.length > 0 && (
        <div className="card" style={{ marginBottom: 16 }}>
          <h2>Captured Entries ({captured.length}) &amp; Export Details</h2>
          {captured.map((item, idx) => (
            <div className="entry-row" key={item.address}>
              <div className="row" style={{ flex: 1 }}>
                <input
                  className="input"
                  style={{ width: 180 }}
                  value={item.label}
                  placeholder="Label (e.g. Gold)"
                  onChange={(e) =>
                    setCaptured((prev) =>
                      prev.map((c, i) => (i === idx ? { ...c, label: e.target.value } : c))
                    )
                  }
                />
                <input
                  className="input"
                  style={{ width: 120 }}
                  value={item.category ?? ""}
                  placeholder="Category"
                  onChange={(e) =>
                    setCaptured((prev) =>
                      prev.map((c, i) => (i === idx ? { ...c, category: e.target.value } : c))
                    )
                  }
                />
                <code>{item.address}</code>
                <span className="badge">{item.value_type}</span>
              </div>
              <button
                className="btn btn-danger"
                onClick={() => setCaptured((prev) => prev.filter((_, i) => i !== idx))}
              >
                Remove
              </button>
            </div>
          ))}

          <div className="form-grid" style={{ marginTop: 12 }}>
            <label>
              Trainer ID (filename-safe)
              <input
                className="input"
                value={trainerId}
                onChange={(e) => setTrainerId(e.target.value)}
              />
            </label>
            <label>
              Game Title
              <input
                className="input"
                value={gameTitle}
                onChange={(e) => setGameTitle(e.target.value)}
              />
            </label>
            <label>
              Author
              <input className="input" value={author} onChange={(e) => setAuthor(e.target.value)} />
            </label>
            <label>
              Version
              <input
                className="input"
                value={version}
                onChange={(e) => setVersion(e.target.value)}
              />
            </label>
            <label>
              Steam AppID (optional)
              <input
                className="input"
                placeholder="e.g. 123456"
                value={steamAppId}
                onChange={(e) => setSteamAppId(e.target.value)}
              />
            </label>
            <label>
              Description (optional)
              <input
                className="input"
                placeholder="Gold, HP, XP values"
                value={description}
                onChange={(e) => setDescription(e.target.value)}
              />
            </label>
          </div>

          <div className="row">
            <button className="btn" onClick={() => handleExport(true)}>
              Save to Library &amp; Generate Exports
            </button>
            <button className="btn btn-secondary" onClick={() => handleExport(false)}>
              Export Only (.CT + Repo JSON)
            </button>
          </div>
        </div>
      )}

      {exportResult && (
        <div className="card">
          <div className="row" style={{ justifyContent: "space-between" }}>
            <div className="card-title">Ready to Share (.CT &amp; Community Repository)</div>
            <div className="row">
              <button
                className="btn btn-secondary"
                onClick={() => downloadText(`${exportResult.trainer_id}.CT`, exportResult.ct_xml)}
              >
                Download .CT
              </button>
              <button
                className="btn btn-secondary"
                onClick={() =>
                  downloadText(
                    `${exportResult.trainer_id}.trainer.json`,
                    exportResult.trainer_json
                  )
                }
              >
                Download .trainer.json
              </button>
              <button className="btn" onClick={() => onOpenTrainer(exportResult.trainer_id)}>
                Open in Library
              </button>
            </div>
          </div>

          <div className="muted">
            SHA-256: <code>{exportResult.sha256}</code>
          </div>

          <label>
            Community Repo <code>index.json</code> Entry (paste into your GitHub repo&apos;s{" "}
            <code>trainers</code> array)
            <textarea className="textarea" readOnly value={exportResult.repo_index_entry} />
          </label>

          <label>
            Cheat Engine <code>.CT</code> XML
            <textarea className="textarea" readOnly value={exportResult.ct_xml} />
          </label>
        </div>
      )}

      <ProcessPickerModal
        open={pickerOpen}
        onClose={() => setPickerOpen(false)}
        onSelect={handlePickProcess}
      />
    </div>
  );
}
