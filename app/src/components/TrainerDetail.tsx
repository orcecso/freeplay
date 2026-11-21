import { useEffect, useState } from "react";
import { api, ProcessDto, SessionStatus, TrainerEntry, TrainerManifest } from "../api";
import { ProcessPickerModal } from "./ProcessPickerModal";

interface Props {
  trainerId: string;
  session: SessionStatus;
  onSessionChange: (s: SessionStatus) => void;
  onBack: () => void;
}

export function TrainerDetail({ trainerId, session, onSessionChange, onBack }: Props) {
  const [manifest, setManifest] = useState<TrainerManifest | null>(null);
  const [values, setValues] = useState<Record<string, string>>({});
  const [frozen, setFrozen] = useState<Record<string, boolean>>({});
  const [patches, setPatches] = useState<Record<string, boolean>>({});
  const [exportedXml, setExportedXml] = useState<string | null>(null);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [message, setMessage] = useState<{ kind: "success" | "error"; text: string } | null>(null);

  const isAttachedToThis = session.attached && session.trainer_id === trainerId && session.alive;

  useEffect(() => {
    api
      .loadTrainer(trainerId)
      .then((m) => {
        setManifest(m);
        const initVals: Record<string, string> = {};
        for (const e of m.entries) {
          if (e.default_value !== undefined) initVals[e.id] = String(e.default_value);
        }
        setValues(initVals);
      })
      .catch((e) => setMessage({ kind: "error", text: String(e) }));
  }, [trainerId]);

  const handleAttach = async () => {
    setMessage(null);
    try {
      const s = await api.attachTrainer(trainerId);
      onSessionChange(s);
      setMessage({
        kind: "success",
        text: `Attached to ${s.process_name} (PID ${s.pid}).`,
      });
    } catch (e) {
      setMessage({ kind: "error", text: String(e) });
    }
  };

  const handleAttachPicked = async (proc: ProcessDto) => {
    setMessage(null);
    try {
      const s = await api.attachTrainerToPid(trainerId, proc.pid, proc.name);
      onSessionChange(s);
      setMessage({
        kind: "success",
        text: `Attached to ${s.process_name} (PID ${s.pid}).`,
      });
    } catch (e) {
      setMessage({ kind: "error", text: String(e) });
    }
  };

  const handleDetach = async () => {
    const s = await api.detachTrainer();
    onSessionChange(s);
    setFrozen({});
    setPatches({});
  };

  const handleRead = async (entry: TrainerEntry) => {
    try {
      const val = await api.readEntry(entry.id);
      setValues((prev) => ({ ...prev, [entry.id]: val }));
    } catch (e) {
      setMessage({ kind: "error", text: `${entry.label}: ${e}` });
    }
  };

  const handleWrite = async (entry: TrainerEntry) => {
    const num = Number(values[entry.id]);
    if (Number.isNaN(num)) {
      setMessage({ kind: "error", text: `${entry.label}: enter a valid number.` });
      return;
    }
    try {
      await api.writeEntry(entry.id, num);
      setMessage({ kind: "success", text: `Wrote ${num} to ${entry.label}.` });
    } catch (e) {
      setMessage({ kind: "error", text: `${entry.label}: ${e}` });
    }
  };

  const handleFreezeToggle = async (entry: TrainerEntry) => {
    const next = !frozen[entry.id];
    const parsed = values[entry.id] !== undefined ? Number(values[entry.id]) : undefined;
    try {
      const active = await api.setFreeze(
        entry.id,
        next,
        parsed !== undefined && !Number.isNaN(parsed) ? parsed : undefined
      );
      setFrozen((prev) => ({ ...prev, [entry.id]: active }));
    } catch (e) {
      setMessage({ kind: "error", text: `${entry.label}: ${e}` });
    }
  };

  const handlePatchToggle = async (entry: TrainerEntry) => {
    const next = !patches[entry.id];
    try {
      const active = await api.togglePatch(entry.id, next);
      setPatches((prev) => ({ ...prev, [entry.id]: active }));
    } catch (e) {
      setMessage({ kind: "error", text: `${entry.label}: ${e}` });
    }
  };

  const handleExportCt = async () => {
    try {
      const res = await api.exportCt(trainerId);
      setExportedXml(res.xml);
      if (res.skipped.length > 0) {
        setMessage({
          kind: "success",
          text: `Exported .CT XML (${res.skipped.length} non-static entries omitted).`,
        });
      }
    } catch (e) {
      setMessage({ kind: "error", text: String(e) });
    }
  };

  if (!manifest) {
    return <div className="muted">Loading trainer...</div>;
  }

  return (
    <div>
      <div className="header-row">
        <div>
          <div className="row" style={{ marginBottom: 6 }}>
            <button className="btn btn-secondary" onClick={onBack}>
              ← Library
            </button>
            <h1>{manifest.game.title}</h1>
            <span className="badge">v{manifest.version}</span>
          </div>
          <div className="muted">
            Targets: <code>{manifest.game.processes.join(", ")}</code> · {manifest.game.pointer_width}
            -bit · Author: {manifest.author}
          </div>
        </div>
        <div className="row">
          <button className="btn btn-secondary" onClick={handleExportCt}>
            Export .CT
          </button>
          {isAttachedToThis ? (
            <button className="btn btn-danger" onClick={handleDetach}>
              Detach
            </button>
          ) : (
            <>
              <button className="btn btn-secondary" onClick={() => setPickerOpen(true)}>
                Pick Process...
              </button>
              <button className="btn" onClick={handleAttach}>
                Attach to Game
              </button>
            </>
          )}
        </div>
      </div>

      {message && <div className={`banner ${message.kind}`}>{message.text}</div>}

      {exportedXml && (
        <div className="card" style={{ marginBottom: 16 }}>
          <div className="row" style={{ justifyContent: "space-between" }}>
            <div className="card-title">Exported Cheat Engine (.CT) XML</div>
            <button className="btn btn-secondary" onClick={() => setExportedXml(null)}>
              Close
            </button>
          </div>
          <textarea className="textarea" readOnly value={exportedXml} />
        </div>
      )}

      {manifest.entries.length === 0 ? (
        <div className="card">
          <div className="card-title">No declarative entries in this trainer</div>
          <div className="muted">
            If you imported a Cheat Engine <code>.CT</code> file, its options likely use{" "}
            <strong>Auto Assembler scripts</strong> or script-defined symbols (e.g.{" "}
            <code>[pPlayer]+10</code>), which are skipped during declarative import.
          </div>
        </div>
      ) : (
        manifest.entries.map((entry) => (
        <div className="entry-row" key={entry.id}>
          <div>
            <div className="row">
              <strong>{entry.label}</strong>
              {entry.category && <span className="badge">{entry.category}</span>}
              {entry.hotkey && <span className="badge badge-warning">{entry.hotkey}</span>}
              <span className="muted">
                {entry.kind === "value"
                  ? entry.value_type
                  : entry.kind === "auto_assembler"
                  ? "AA Script"
                  : "AoB Patch"}
              </span>
            </div>
            {entry.description && <div className="muted">{entry.description}</div>}
          </div>

          {entry.kind === "value" ? (
            <div className="row">
              <input
                className="input"
                style={{ width: 120 }}
                placeholder="Value"
                value={values[entry.id] ?? ""}
                onChange={(e) => setValues((prev) => ({ ...prev, [entry.id]: e.target.value }))}
                disabled={!isAttachedToThis}
              />
              <button
                className="btn btn-secondary"
                disabled={!isAttachedToThis}
                onClick={() => handleRead(entry)}
              >
                Read
              </button>
              <button
                className="btn"
                disabled={!isAttachedToThis}
                onClick={() => handleWrite(entry)}
              >
                Set
              </button>
              <button
                className={frozen[entry.id] ? "btn btn-danger" : "btn btn-secondary"}
                disabled={!isAttachedToThis}
                onClick={() => handleFreezeToggle(entry)}
              >
                {frozen[entry.id] ? "Unfreeze" : "Freeze"}
              </button>
            </div>
          ) : (
            <button
              className={patches[entry.id] ? "btn btn-danger" : "btn"}
              disabled={!isAttachedToThis}
              onClick={() => handlePatchToggle(entry)}
            >
              {patches[entry.id] ? "Disable Patch" : "Enable Patch"}
            </button>
          )}
        </div>
      ))
      )}

      <ProcessPickerModal
        open={pickerOpen}
        onClose={() => setPickerOpen(false)}
        onSelect={handleAttachPicked}
      />
    </div>
  );
}
