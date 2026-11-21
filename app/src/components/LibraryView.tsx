import { useEffect, useState } from "react";
import { api, AvailableUpdate, InstalledRecord } from "../api";

interface Props {
  onOpenTrainer: (trainerId: string) => void;
  onUpdatesChanged: (count: number) => void;
}

export function LibraryView({ onOpenTrainer, onUpdatesChanged }: Props) {
  const [installed, setInstalled] = useState<InstalledRecord[]>([]);
  const [updates, setUpdates] = useState<Record<string, AvailableUpdate>>({});
  const [error, setError] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);

  const load = async () => {
    try {
      setError(null);
      const [items, updList] = await Promise.all([api.listInstalled(), api.checkUpdates()]);
      setInstalled(items);
      const map: Record<string, AvailableUpdate> = {};
      for (const u of updList) map[u.trainer_id] = u;
      setUpdates(map);
      onUpdatesChanged(updList.length);
    } catch (e) {
      setError(String(e));
    }
  };

  useEffect(() => {
    load();
  }, []);

  const handleUpgrade = async (u: AvailableUpdate) => {
    setBusyId(u.trainer_id);
    try {
      await api.installTrainer(u.repo_id, u.trainer_id);
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusyId(null);
    }
  };

  const handleUninstall = async (trainerId: string) => {
    setBusyId(trainerId);
    try {
      await api.uninstallTrainer(trainerId);
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusyId(null);
    }
  };

  return (
    <div>
      <div className="header-row">
        <div>
          <h1>Installed Trainers</h1>
          <div className="muted">Your local library of declarative single-player trainers.</div>
        </div>
        <button className="btn btn-secondary" onClick={load}>
          Refresh
        </button>
      </div>

      {error && <div className="banner error">{error}</div>}

      {installed.length === 0 ? (
        <div className="card">
          <div className="card-title">Library is empty</div>
          <div className="muted">
            Add a community repository in <strong>Repositories</strong>, browse trainers in{" "}
            <strong>Browse Repos</strong>, or import a Cheat Engine <code>.CT</code> table.
          </div>
        </div>
      ) : (
        <div className="card-grid">
          {installed.map((rec) => {
            const upd = updates[rec.trainer_id];
            return (
              <div className="card" key={rec.trainer_id}>
                <div className="row" style={{ justifyContent: "space-between" }}>
                  <div className="card-title">{rec.title}</div>
                  <span className="badge">v{rec.version}</span>
                </div>
                <div className="muted">
                  Processes: <code>{rec.processes.join(", ")}</code>
                  <br />
                  Author: {rec.author}
                  {rec.source_repo_id ? ` · Repo: ${rec.source_repo_id}` : " · Local import"}
                </div>
                <div className="row" style={{ marginTop: "auto" }}>
                  <button className="btn" onClick={() => onOpenTrainer(rec.trainer_id)}>
                    Open
                  </button>
                  {upd && (
                    <button
                      className="btn btn-secondary"
                      disabled={busyId === rec.trainer_id}
                      onClick={() => handleUpgrade(upd)}
                    >
                      Update → v{upd.available_version}
                    </button>
                  )}
                  <button
                    className="btn btn-danger"
                    style={{ marginLeft: "auto" }}
                    disabled={busyId === rec.trainer_id}
                    onClick={() => handleUninstall(rec.trainer_id)}
                  >
                    Remove
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
