import { useEffect, useState } from "react";
import { api, RepoSource } from "../api";

interface Props {
  onChanged: () => void;
}

export function ReposView({ onChanged }: Props) {
  const [repos, setRepos] = useState<RepoSource[]>([]);
  const [url, setUrl] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ kind: "success" | "error"; text: string } | null>(null);

  const load = async () => {
    try {
      setRepos(await api.listRepos());
    } catch (e) {
      setMessage({ kind: "error", text: String(e) });
    }
  };

  useEffect(() => {
    load();
  }, []);

  const handleAdd = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!url.trim()) return;
    setBusy(true);
    setMessage(null);
    try {
      const idx = await api.addRepo(url.trim());
      setUrl("");
      setMessage({
        kind: "success",
        text: `Added "${idx.repo_name}" (${idx.trainers.length} trainer(s) available).`,
      });
      await load();
      onChanged();
    } catch (err) {
      setMessage({ kind: "error", text: String(err) });
    } finally {
      setBusy(false);
    }
  };

  const handleRemove = async (repoId: string) => {
    setMessage(null);
    try {
      await api.removeRepo(repoId);
      await load();
      onChanged();
    } catch (err) {
      setMessage({ kind: "error", text: String(err) });
    }
  };

  return (
    <div>
      <div className="header-row">
        <div>
          <h1>Community Repositories</h1>
          <div className="muted">
            Add GitHub raw URLs or local <code>file:///</code> paths pointing to an{" "}
            <code>index.json</code> catalog.
          </div>
        </div>
      </div>

      <form className="row" style={{ marginBottom: 18 }} onSubmit={handleAdd}>
        <input
          className="input"
          placeholder="https://raw.githubusercontent.com/<user>/<repo>/main/index.json"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
        />
        <button className="btn" type="submit" disabled={busy || !url.trim()}>
          {busy ? "Adding..." : "Add Repo"}
        </button>
      </form>

      {message && <div className={`banner ${message.kind}`}>{message.text}</div>}

      {repos.length === 0 ? (
        <div className="card">
          <div className="card-title">No repositories configured</div>
          <div className="muted">
            FreePlay ships with zero built-in trainers. Paste a community <code>index.json</code>{" "}
            URL above—or test locally with <code>examples/sample-repo/index.json</code>.
          </div>
        </div>
      ) : (
        repos.map((r) => (
          <div className="entry-row" key={r.repo_id}>
            <div>
              <div className="card-title">{r.repo_name}</div>
              <div className="muted">
                ID: <code>{r.repo_id}</code> · {r.index_url}
              </div>
            </div>
            <button className="btn btn-danger" onClick={() => handleRemove(r.repo_id)}>
              Remove
            </button>
          </div>
        ))
      )}
    </div>
  );
}
