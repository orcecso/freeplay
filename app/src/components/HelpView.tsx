import { useState } from "react";
import { api } from "../api";

export const BUY_ME_A_COFFEE_URL = "https://buymeacoffee.com/orcecso";

interface FaqItem {
  question: string;
  answer: string;
}

const FAQ_ITEMS: FaqItem[] = [
  {
    question: "Is FreePlay safe to use? Does it work on multiplayer games?",
    answer:
      "FreePlay is strictly built for offline, single-player games. It uses standard Windows user-mode APIs only and actively refuses to attach if known anti-cheat services or modules (such as Easy Anti-Cheat, BattlEye, or Vanguard) are detected. Never use trainers in online or multiplayer games.",
  },
  {
    question: "Why did FreePlay refuse to attach to my game?",
    answer:
      "There are two common reasons: (1) an anti-cheat service or protected module was detected, which triggers FreePlay's safety guard, or (2) the game is running with elevated (Administrator) privileges while FreePlay is not. Make sure the game is an offline single-player title and try matching privilege levels if needed.",
  },
  {
    question: "How do I install trainers from a community repository?",
    answer:
      "Open the Repositories tab and add a repository's index.json URL (both https:// and local file:/// URLs are supported). Then switch to Browse Repos, search for your game, and click Install. Installed trainers appear in your Library.",
  },
  {
    question: "Can I use my existing Cheat Engine (.CT) tables?",
    answer:
      "Yes! Open the Import / Export .CT tab to convert Cheat Engine XML (.CT) tables into FreePlay trainer manifests, or export your FreePlay trainers back to .CT format.",
  },
  {
    question: "How does the Value Scanner work?",
    answer:
      "Open Value Scanner, pick a running single-player game process, and run a First Scan for a value you see in-game (like health, gold, or ammo). Change the value in-game, then run Next Scan (Exact, Increased, Decreased, Changed, or Unchanged) until you narrow down the address. You can edit values directly or capture and export your findings as a reusable trainer.",
  },
  {
    question: "Why did Next Scan return 0 hits? Which Value Type should I use?",
    answer:
      "4 Bytes (i32) is the default because it is fast and matches most classic games. However, many modern indie games (Unity, Godot, GameMaker, etc.) store currency, health, or timers as Float (f32) or Double (f64) — sometimes with hidden decimals. If your current Value Type drops to 0 hits on Next Scan, click Reset, switch Value Type to All (4 Bytes + Float + Double), and scan again.",
  },
  {
    question: "How can I support FreePlay development?",
    answer:
      "FreePlay is free and open-source. If it saves you time or makes your single-player gaming more fun, you can support ongoing development by buying a coffee at buymeacoffee.com/orcecso, sharing the project, or contributing community trainers!",
  },
];

export function HelpView() {
  const [openIndex, setOpenIndex] = useState<number | null>(0);
  const [copied, setCopied] = useState(false);

  const handleOpenCoffee = () => {
    api.openExternalUrl(BUY_ME_A_COFFEE_URL);
  };

  const handleCopyLink = async () => {
    try {
      await navigator.clipboard.writeText(BUY_ME_A_COFFEE_URL);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Fallback if clipboard API is unavailable
    }
  };

  return (
    <div className="help-view">
      <div className="page-header">
        <div>
          <h1>Guide &amp; FAQ</h1>
          <p className="muted">Quick-start README, safety policy, and frequently asked questions.</p>
        </div>
      </div>

      {/* Highlighted Buy Me a Coffee Support Card */}
      <section className="coffee-hero">
        <div className="coffee-hero-content">
          <div className="coffee-badge">Support the Project</div>
          <h2>Enjoying FreePlay? Buy Me a Coffee!</h2>
          <p>
            FreePlay is completely free and open-source — no paywalls, no subscriptions. If you love
            using it, consider fueling future updates, new features, and maintenance with a coffee.
          </p>
          <div className="coffee-actions">
            <button className="btn-coffee" onClick={handleOpenCoffee}>
              <span aria-hidden="true">☕</span> Buy Me a Coffee
            </button>
            <button className="btn-secondary" onClick={handleCopyLink}>
              {copied ? "Link Copied!" : "Copy Support Link"}
            </button>
          </div>
          <div className="coffee-url">{BUY_ME_A_COFFEE_URL}</div>
        </div>
      </section>

      {/* README / Quick Start */}
      <section className="card help-section">
        <h2>Quick-Start README</h2>
        <p className="muted">
          FreePlay is an open-source, single-player game trainer platform built around a community
          repository model.
        </p>

        <div className="guide-grid">
          <div className="guide-step">
            <span className="step-num">1</span>
            <div>
              <strong>Library</strong>
              <p className="muted">
                View your installed trainers, attach to a running single-player game, and toggle
                cheats or freeze values live.
              </p>
            </div>
          </div>

          <div className="guide-step">
            <span className="step-num">2</span>
            <div>
              <strong>Value Scanner</strong>
              <p className="muted">
                Scan memory for health, currency, or stats on the fly, modify them directly, and
                export your own trainer manifest.
              </p>
            </div>
          </div>

          <div className="guide-step">
            <span className="step-num">3</span>
            <div>
              <strong>Repositories &amp; Browse</strong>
              <p className="muted">
                Add community repository <code>index.json</code> feeds in Repositories, then browse
                and install SHA-256 verified trainers.
              </p>
            </div>
          </div>

          <div className="guide-step">
            <span className="step-num">4</span>
            <div>
              <strong>Import / Export .CT</strong>
              <p className="muted">
                Bring in existing Cheat Engine <code>.CT</code> tables or export your FreePlay
                trainers to share with others.
              </p>
            </div>
          </div>
        </div>

        <div className="policy-box">
          <strong>Scope &amp; Safety Policy</strong>
          <ul>
            <li>
              Strictly for <strong>offline, single-player games</strong> only.
            </li>
            <li>
              <strong>No anti-cheat or DRM bypasses:</strong> uses standard Windows user-mode APIs
              and automatically refuses to attach when anti-cheat software is detected.
            </li>
            <li>
              <strong>No multiplayer or DLC unlocking:</strong> do not use or publish trainers for
              online games, paid DLC, or real-money currencies.
            </li>
          </ul>
        </div>
      </section>

      {/* FAQ Accordion */}
      <section className="card help-section">
        <h2>Frequently Asked Questions</h2>
        <div className="faq-list">
          {FAQ_ITEMS.map((item, idx) => {
            const isOpen = openIndex === idx;
            return (
              <div key={item.question} className={`faq-item ${isOpen ? "open" : ""}`}>
                <button
                  className="faq-question"
                  onClick={() => setOpenIndex(isOpen ? null : idx)}
                  aria-expanded={isOpen}
                >
                  <span>{item.question}</span>
                  <span className="faq-chevron">{isOpen ? "−" : "+"}</span>
                </button>
                {isOpen && <div className="faq-answer">{item.answer}</div>}
              </div>
            );
          })}
        </div>
      </section>
    </div>
  );
}
