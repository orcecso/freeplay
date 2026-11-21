import { useEffect, useState } from "react";
import { api, CatalogHit } from "../api";

interface Props {
  onInstalled: () => void;
}

export function BrowseView({ onInstalled }: Props) {
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<CatalogHit[]>([]);
  const [busyKey, setBusyKey] = useState<string | null>(null);
  const [message, setMessage] = useState<{ kind: "success" | "error"; text: string } | null>(null);

  const search = async (q: string) => {
    try {
      const res = await api.searchCatalog(q);
      setHits(res);
    } catch (e) {
      setMessage({ kind: "error", text: String(e) });
    }
  };

  useEffect(() => {
    search("");
  }, []);

  const handleRefresh = async () => {
    setMessage(null);
    try {
      const res = await api.refreshRepos();
      await search(query);
      if (res.errors.length > 0) {
        setMessage({
          kind: "error",
          text: `Refreshed ${res.refreshed} repo(s), errors: ${res.errors.join("; ")}`,
        });
      } else {
        setMessage({ kind: "success", text: `Refreshed ${res.refreshed} repository catalog(s).` });
      }
    } catch (e) {
      setMessage({ kind: "error", text: String(e) });
    }
  };

  const handleInstall = async (hit: CatalogHit) => {
    const key = `${hit.repo_id}:${hit.item.id}`;
    setBusyKey(key);
    setMessage(null);
    try {
      const rec = await api.installTrainer(hit.repo_id, hit.item.id);
      setMessage({ kind: "success", text: `Installed ${rec.title} v${rec.version} (SHA-256 verified).` });
      await search(query);
      onInstalled();
    } catch (e) {
      setMessage({ kind: "error", text: String(e) });
    } finally {
      setBusyKey(null);
    }
  };

  return (
    <div>
      <div className="header-row">
        <div>
          <h1>Browse Community Catalog</h1>
          <div className="muted">Search by game title, executable name, or Steam AppID.</div>
        </div>
        <button className="btn btn-secondary" onClick={handleRefresh}>
          Refresh Catalogs
        </button>
      </div>

      <div className="row" style={{ marginBottom: 16 }}>
        <input
          className="input"
          placeholder="Search games (e.g. skyrpg.exe, 123456, RPG)..."
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            search(e.target.value);
          }}
        />
      </div>

      {message && <div className={`banner ${message.kind}`}>{message.text}</div>}

      {hits.length === 0 ? (
        <div className="card">
          <div className="card-title">No trainers found</div>
          <div className="muted">
            Add a community repository URL under <strong>Repositories</strong> first, then refresh.
          </div>
        </div>
      ) : (
        <div className="card-grid">
          {hits.map((hit) => {
            const key = `${hit.repo_id}:${hit.item.id}`;
            const upToDate = hit.installed_version === hit.item.trainer_version;
            return (
              <div className="card" key={key}>
                <div className="row" style={{ justifyContent: "space-between" }}>
                  <div className="card-title">{hit.item.title}</div>
                  <span className="badge">v{hit.item.trainer_version}</span>
                </div>
                <div className="muted">
                  {hit.item.description || "Declarative trainer definition"}
                  <br />
                  Process: <code>{hit.item.processes.join(", ")}</code>
                  <br />
                  Repo: {hit.repo_name} · Author: {hit.item.author}
                </div>
                <div className="row" style={{ marginTop: "auto" }}>
                  <button
                    className={upToDate ? "btn btn-secondary" : "btn"}
                    disabled={busyKey === key || upToDate}
                    onClick={() => handleInstall(hit)}
                  >
                    {busyKey === key
                      ? "Installing..."
                      : upToDate
                      ? "Installed"
                      : hit.installed_version
                      ? `Update (v${hit.installed_version} → v${hit.item.trainer_version})`
                      : "Install"}
                  </button>
                  {hit.item.sha256 && <span className="badge badge-success">SHA-256</span>}
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
