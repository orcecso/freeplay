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

function formatProcessTitle(processName: string): string {
  return processName
    .replace(/\.exe$/i, "")
    .replace(/[_-]+/g, " ")
    .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
    .replace(/\s+/g, " ")
    .trim();
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
  const [valueType, setValueType] = useState<ScanValueTypeName>("i32");
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
  const [autoCombine, setAutoCombine] = useState(true);
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
      const detectedTitle = proc.window_title?.trim() || formatProcessTitle(proc.name);
      if (detectedTitle) {
        setGameTitle(detectedTitle);
        const slug = detectedTitle
          .toLowerCase()
          .replace(/[^a-z0-9]+/g, "-")
          .replace(/^-|-$/g, "");
        if (slug) setTrainerId(`${slug}-id`);
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

  const handleReset = () => {
    setHasFirstScan(false);
    setScanMode("exact");
    setScanValue("");
    setTotalHits(0);
    setPreview([]);
    setEditValues({});
    setProgress(null);
    setMessage(null);
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

  const isHitCaptured = (hit: ScanHitDto) =>
    captured.some(
      (c) =>
        c.value_type === hit.value_type &&
        (c.address === hit.address || (c.extra_addresses ?? []).includes(hit.address))
    );

  const handleCapture = (hit: ScanHitDto) => {
    if (isHitCaptured(hit)) return;
    const key = `${hit.address}:${hit.value_type}`;
    const defaultNum = Number(editValues[key] ?? hit.value);
    const parsedDefault = Number.isNaN(defaultNum) ? undefined : defaultNum;

    setCaptured((prev) => {
      if (autoCombine && parsedDefault !== undefined) {
        const matchIdx = prev.findIndex(
          (c) => c.value_type === hit.value_type && c.default_value === parsedDefault
        );
        if (matchIdx !== -1) {
          return prev.map((c, i) =>
            i === matchIdx
              ? { ...c, extra_addresses: [...(c.extra_addresses ?? []), hit.address] }
              : c
          );
        }
      }
      return [
        ...prev,
        {
          label: `Value ${prev.length + 1}`,
          category: "Player",
          address: hit.address,
          extra_addresses: [],
          value_type: hit.value_type,
          default_value: parsedDefault,
          freeze: false,
        },
      ];
    });
  };

  const handleCaptureAllCombined = () => {
    const toAdd = visiblePreview;
    if (toAdd.length === 0) return;

    setCaptured((prev) => {
      const next = [...prev];
      const byType = new Map<string, ScanHitDto[]>();
      for (const hit of toAdd) {
        const list = byType.get(hit.value_type) ?? [];
        list.push(hit);
        byType.set(hit.value_type, list);
      }

      for (const [, group] of byType) {
        const first = group[0];
        const key = `${first.address}:${first.value_type}`;
        const defaultNum = Number(editValues[key] ?? first.value);
        const parsedDefault = Number.isNaN(defaultNum) ? undefined : defaultNum;
        const addrs = group.map((h) => h.address);

        if (autoCombine && parsedDefault !== undefined) {
          const matchIdx = next.findIndex(
            (c) => c.value_type === first.value_type && c.default_value === parsedDefault
          );
          if (matchIdx !== -1) {
            const existing = next[matchIdx];
            const existingSet = new Set([existing.address, ...(existing.extra_addresses ?? [])]);
            const fresh = addrs.filter((a) => !existingSet.has(a));
            next[matchIdx] = {
              ...existing,
              extra_addresses: [...(existing.extra_addresses ?? []), ...fresh],
            };
            continue;
          }
        }

        next.push({
          label: `Value ${next.length + 1}`,
          category: "Player",
          address: addrs[0],
          extra_addresses: addrs.slice(1),
          value_type: first.value_type,
          default_value: parsedDefault,
          freeze: false,
        });
      }
      return next;
    });
  };

  const handleCombineCapturedByType = () => {
    setCaptured((prev) => {
      const merged: CapturedEntryInput[] = [];
      for (const item of prev) {
        const idx = merged.findIndex((m) => m.value_type === item.value_type);
        if (idx === -1) {
          merged.push({ ...item, extra_addresses: [...(item.extra_addresses ?? [])] });
        } else {
          const target = merged[idx];
          const seen = new Set([target.address, ...(target.extra_addresses ?? [])]);
          const extras = [item.address, ...(item.extra_addresses ?? [])].filter((a) => !seen.has(a));
          merged[idx] = {
            ...target,
            extra_addresses: [...(target.extra_addresses ?? []), ...extras],
          };
        }
      }
      return merged;
    });
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

  const visiblePreview = preview.filter((hit) => !isHitCaptured(hit));
  const hasCombinableCaptured =
    new Set(captured.map((c) => c.value_type)).size < captured.length;

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
              <option value="i32">4 Bytes (i32)</option>
              <option value="f32">Float (f32)</option>
              <option value="f64">Double (f64)</option>
              <option value="i64">8 Bytes (i64)</option>
              <option value="i16">2 Bytes (i16)</option>
              <option value="u8">Byte (u8)</option>
              <option value="all">All (4 Bytes + Float + Double)</option>
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
          <button
            className="btn btn-secondary"
            disabled={busy || (!hasFirstScan && !scanValue && !progress)}
            onClick={handleReset}
          >
            Reset
          </button>
          {hasFirstScan && (
            <span className="muted" style={{ marginLeft: "auto" }}>
              Found: <strong>{totalHits.toLocaleString()}</strong> (showing top {preview.length})
            </span>
          )}
        </div>

        <div className="muted" style={{ marginTop: 8, fontSize: 12 }}>
          <strong>Tip:</strong> Start with <code>4 Bytes (i32)</code>. If Next Scan drops to 0 hits,
          click <strong>Reset</strong> and try <code>All (4 Bytes + Float + Double)</code> — many
          games store money, health, or stats as Float or Double.
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

      {visiblePreview.length > 0 && (
        <div className="card" style={{ marginBottom: 16 }}>
          <div className="row" style={{ justifyContent: "space-between", marginBottom: 8 }}>
            <h2 style={{ margin: 0 }}>Scan Results</h2>
            <div className="row" style={{ gap: 12 }}>
              <label
                className="row"
                style={{ gap: 6, fontSize: 12, cursor: "pointer", margin: 0 }}
                title="Automatically merge newly captured addresses that share the same type and value into one entry"
              >
                <input
                  type="checkbox"
                  checked={autoCombine}
                  onChange={(e) => setAutoCombine(e.target.checked)}
                />
                <span className="muted">Auto-combine matching</span>
              </label>
              {visiblePreview.length >= 2 && visiblePreview.length <= 25 && (
                <button className="btn btn-secondary" onClick={handleCaptureAllCombined}>
                  + Capture All ({visiblePreview.length}) as 1 Entry
                </button>
              )}
            </div>
          </div>
          <div style={{ maxHeight: 240, overflowY: "auto" }}>
            {visiblePreview.map((hit) => {
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
          <div className="row" style={{ justifyContent: "space-between", marginBottom: 4 }}>
            <h2 style={{ margin: 0 }}>Captured Entries ({captured.length}) &amp; Export Details</h2>
            {hasCombinableCaptured && (
              <button
                className="btn btn-secondary"
                onClick={handleCombineCapturedByType}
                title="Merge captured entries with the same value type into a single combined entry"
              >
                Combine Same Type
              </button>
            )}
          </div>
          <div className="muted" style={{ marginBottom: 4 }}>
            <strong>Tip:</strong> Click any variable name below to rename it (e.g.{" "}
            <code>Current Coins</code>, <code>Health</code>) before saving to your Library. You can{" "}
            <strong>+ Capture</strong> multiple values — even across new scans — and save them all
            together in one trainer.
          </div>
          {captured.map((item, idx) => (
            <div className="entry-row" key={`${item.address}:${item.value_type}`}>
              <div className="row" style={{ flex: 1 }}>
                <input
                  className="input"
                  style={{ width: 210 }}
                  value={item.label}
                  title="Click here to edit the variable name"
                  placeholder="Click to edit variable name (e.g. Gold)"
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
                {item.extra_addresses && item.extra_addresses.length > 0 && (
                  <span
                    className="badge badge-success"
                    title={[item.address, ...item.extra_addresses].join(", ")}
                  >
                    {1 + item.extra_addresses.length} linked
                  </span>
                )}
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
