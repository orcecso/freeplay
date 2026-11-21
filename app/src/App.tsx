import { useEffect, useState } from "react";
import { api, SessionStatus } from "./api";
import { LibraryView } from "./components/LibraryView";
import { BrowseView } from "./components/BrowseView";
import { ReposView } from "./components/ReposView";
import { ImportExportView } from "./components/ImportExportView";
import { TrainerDetail } from "./components/TrainerDetail";
import { ScannerView } from "./components/ScannerView";
import { HelpView, BUY_ME_A_COFFEE_URL } from "./components/HelpView";

type Tab = "library" | "scanner" | "browse" | "repos" | "import" | "help";

function getDemoInitialState(): { tab: Tab; trainerId: string | null } {
  if (typeof window === "undefined") return { tab: "library", trainerId: null };
  const params = new URLSearchParams(window.location.search);
  if (params.get("demo") !== "1") return { tab: "library", trainerId: null };
  const t = params.get("tab");
  if (t === "trainer") return { tab: "library", trainerId: "warhammer-survivors" };
  if (t === "scanner" || t === "browse" || t === "repos" || t === "import" || t === "help") {
    return { tab: t, trainerId: null };
  }
  return { tab: "library", trainerId: null };
}

export default function App() {
  const init = getDemoInitialState();
  const [tab, setTab] = useState<Tab>(init.tab);
  const [selectedTrainerId, setSelectedTrainerId] = useState<string | null>(init.trainerId);
  const [session, setSession] = useState<SessionStatus>({ attached: false, alive: false });
  const [updateCount, setUpdateCount] = useState(0);

  const refreshStatus = async () => {
    try {
      const s = await api.sessionStatus();
      setSession(s);
    } catch {
      // Running outside Tauri shell (e.g. vite dev preview)
    }
  };

  const refreshUpdates = async () => {
    try {
      const updates = await api.checkUpdates();
      setUpdateCount(updates.length);
    } catch {
      // ignore outside Tauri
    }
  };

  useEffect(() => {
    refreshStatus();
    refreshUpdates();
    const timer = setInterval(refreshStatus, 2500);
    return () => clearInterval(timer);
  }, []);

  return (
    <div className="layout">
      <aside className="sidebar">
        <div className="brand">
          <img src="/logo.svg" alt="FreePlay logo" className="brand-logo" />
          <span>FreePlay</span>
        </div>

        <button
          className={`nav-btn ${tab === "library" && !selectedTrainerId ? "active" : ""}`}
          onClick={() => {
            setSelectedTrainerId(null);
            setTab("library");
          }}
        >
          <span>Library</span>
          {updateCount > 0 && <span className="badge badge-warning">{updateCount}</span>}
        </button>

        <button
          className={`nav-btn ${tab === "scanner" && !selectedTrainerId ? "active" : ""}`}
          onClick={() => {
            setSelectedTrainerId(null);
            setTab("scanner");
          }}
        >
          <span>Value Scanner</span>
        </button>

        <button
          className={`nav-btn ${tab === "browse" && !selectedTrainerId ? "active" : ""}`}
          onClick={() => {
            setSelectedTrainerId(null);
            setTab("browse");
          }}
        >
          <span>Browse Repos</span>
        </button>

        <button
          className={`nav-btn ${tab === "repos" && !selectedTrainerId ? "active" : ""}`}
          onClick={() => {
            setSelectedTrainerId(null);
            setTab("repos");
          }}
        >
          <span>Repositories</span>
        </button>

        <button
          className={`nav-btn ${tab === "import" && !selectedTrainerId ? "active" : ""}`}
          onClick={() => {
            setSelectedTrainerId(null);
            setTab("import");
          }}
        >
          <span>Import / Export .CT</span>
        </button>

        <button
          className={`nav-btn ${tab === "help" && !selectedTrainerId ? "active" : ""}`}
          onClick={() => {
            setSelectedTrainerId(null);
            setTab("help");
          }}
        >
          <span>Guide &amp; FAQ</span>
        </button>

        <div className="sidebar-footer">
          <button
            className="sidebar-coffee-btn"
            onClick={() => api.openExternalUrl(BUY_ME_A_COFFEE_URL)}
            title="Support FreePlay on Buy Me a Coffee"
          >
            <span className="coffee-icon" aria-hidden="true">☕</span>
            <span>Buy Me a Coffee</span>
          </button>

          <div className="session-pill">
            <strong>{session.attached ? "Attached" : "Not Attached"}</strong>
            {session.attached ? (
              <div>
                {session.process_name} (PID {session.pid}){" "}
                <span className={`badge ${session.alive ? "badge-success" : "badge-warning"}`}>
                  {session.alive ? "Live" : "Exited"}
                </span>
              </div>
            ) : (
              <div>Open a trainer from Library to attach.</div>
            )}
          </div>
        </div>
      </aside>

      <main className="main">
        {selectedTrainerId ? (
          <TrainerDetail
            trainerId={selectedTrainerId}
            session={session}
            onSessionChange={setSession}
            onBack={() => setSelectedTrainerId(null)}
          />
        ) : tab === "library" ? (
          <LibraryView
            onOpenTrainer={(id) => setSelectedTrainerId(id)}
            onUpdatesChanged={setUpdateCount}
          />
        ) : tab === "scanner" ? (
          <ScannerView onOpenTrainer={(id) => setSelectedTrainerId(id)} />
        ) : tab === "browse" ? (
          <BrowseView onInstalled={() => refreshUpdates()} />
        ) : tab === "repos" ? (
          <ReposView onChanged={() => refreshUpdates()} />
        ) : tab === "help" ? (
          <HelpView />
        ) : (
          <ImportExportView
            onImported={(id) => {
              setSelectedTrainerId(id);
            }}
          />
        )}
      </main>
    </div>
  );
}
