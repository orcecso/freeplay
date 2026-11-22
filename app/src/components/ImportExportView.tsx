import { useState } from "react";
import { api, ProcessDto } from "../api";
import { ProcessPickerModal } from "./ProcessPickerModal";

interface Props {
  onImported: (trainerId: string) => void;
}

function formatProcessTitle(processName: string): string {
  return processName
    .replace(/\.exe$/i, "")
    .replace(/[_-]+/g, " ")
    .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
    .replace(/\s+/g, " ")
    .trim();
}

function toSlug(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");
}

export function ImportExportView({ onImported }: Props) {
  const [id, setId] = useState("my-imported-game");
  const [gameTitle, setGameTitle] = useState("My Single-Player Game");
  const [processName, setProcessName] = useState("");
  const [author, setAuthor] = useState("community");
  const [pointerWidth, setPointerWidth] = useState<"32" | "64">("64");
  const [allowAutoAssembler, setAllowAutoAssembler] = useState(true);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [xml, setXml] = useState("");
  const [result, setResult] = useState<{
    kind: "success" | "error";
    text: string;
    details?: string[];
    trainerId?: string;
    importedCount?: number;
  } | null>(null);

  const handleFileUpload = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    const text = await file.text();
    setXml(text);
    const stem = file.name.replace(/\.ct$/i, "").replace(/[^a-zA-Z0-9._-]/g, "-").toLowerCase();
    if (stem) setId(stem);
  };

  const handlePickProcess = (proc: ProcessDto) => {
    setProcessName(proc.name);
    const detectedTitle = proc.window_title?.trim() || formatProcessTitle(proc.name);
    if (detectedTitle) {
      setGameTitle(detectedTitle);
      const slug = toSlug(detectedTitle);
      if (slug) setId(`${slug}-id`);
    }
  };

  const handleImportCt = async () => {
    setResult(null);
    if (!processName.trim()) {
      setResult({ kind: "error", text: "Please select a Target Process first." });
      return;
    }
    try {
      const summary = await api.importCt({
        xml,
        id,
        gameTitle,
        processName: processName.trim(),
        author,
        pointerWidth,
        allowAutoAssembler,
      });
      const details = [...summary.warnings, ...summary.skipped.map((s) => `Skipped — ${s}`)];
      setResult({
        kind: summary.imported_entries > 0 ? "success" : "error",
        text:
          summary.imported_entries > 0
            ? `Imported ${summary.imported_entries} declarative entries into "${summary.record.title}" (${summary.skipped.length} skipped).`
            : `Imported 0 declarative entries into "${summary.record.title}" — all ${summary.skipped.length} entries in this .CT file were skipped (see reasons below).`,
        details,
        trainerId: summary.record.trainer_id,
        importedCount: summary.imported_entries,
      });
      // Only auto-navigate when entries were imported cleanly with nothing skipped.
      if (summary.imported_entries > 0 && details.length === 0) {
        onImported(summary.record.trainer_id);
      }
    } catch (e) {
      setResult({ kind: "error", text: String(e) });
    }
  };

  return (
    <div>
      <div className="header-row">
        <div>
          <h1>Import Cheat Engine (.CT) Table</h1>
          <div className="muted">
            Converts declarative value & pointer-chain entries to safe JSON. Lua and Auto Assembler
            scripts are automatically skipped.
          </div>
        </div>
      </div>

      {result && (
        <div className={`banner ${result.kind}`}>
          <div className="row" style={{ justifyContent: "space-between" }}>
            <div>{result.text}</div>
            {result.trainerId && (result.importedCount ?? 0) > 0 && (
              <button className="btn" onClick={() => onImported(result.trainerId!)}>
                FreePlay
              </button>
            )}
          </div>
          {result.details && result.details.length > 0 && (
            <ul style={{ margin: "6px 0 0", paddingLeft: 18 }}>
              {result.details.map((d, i) => (
                <li key={i}>{d}</li>
              ))}
            </ul>
          )}
        </div>
      )}

      <div className="card">
        <div className="form-grid">
          <label>
            Trainer ID (filename-safe)
            <input className="input" value={id} onChange={(e) => setId(e.target.value)} />
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
            Target Process (.exe)
            <div className="row">
              <input
                className="input"
                readOnly
                placeholder="Click Pick Process..."
                value={processName}
              />
              <button
                type="button"
                className="btn btn-secondary"
                style={{ whiteSpace: "nowrap" }}
                onClick={() => setPickerOpen(true)}
              >
                Pick Process...
              </button>
            </div>
          </label>
          <label>
            Author
            <input className="input" value={author} onChange={(e) => setAuthor(e.target.value)} />
          </label>
          <label>
            Pointer Width
            <select
              className="select"
              value={pointerWidth}
              onChange={(e) => setPointerWidth(e.target.value as "32" | "64")}
            >
              <option value="64">64-bit</option>
              <option value="32">32-bit</option>
            </select>
          </label>
          <label>
            Load .CT File
            <input className="input" type="file" accept=".ct,.CT,.xml" onChange={handleFileUpload} />
          </label>
        </div>

        <label>
          .CT XML Content
          <textarea
            className="textarea"
            placeholder="Paste Cheat Engine .CT XML here or pick a file above..."
            value={xml}
            onChange={(e) => setXml(e.target.value)}
          />
        </label>

        <label
          style={{
            flexDirection: "row",
            alignItems: "center",
            gap: 8,
            marginBottom: 8,
            cursor: "pointer",
          }}
        >
          <input
            type="checkbox"
            checked={allowAutoAssembler}
            onChange={(e) => setAllowAutoAssembler(e.target.checked)}
          />
          <span>
            Allow x64 Auto Assembler code-cave scripts &amp; symbols (Lua scripts remain blocked)
          </span>
        </label>

        <div className="row" style={{ marginTop: 8 }}>
          <button
            className="btn"
            disabled={!xml.trim() || !processName.trim()}
            onClick={handleImportCt}
          >
            Import to Library
          </button>
        </div>
      </div>

      <ProcessPickerModal
        open={pickerOpen}
        onClose={() => setPickerOpen(false)}
        onSelect={handlePickProcess}
      />
    </div>
  );
}
