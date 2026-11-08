import {
  ClockCounterClockwise,
  Code,
  Palette,
  Play,
  Plus,
  X,
} from "@phosphor-icons/react";
import { useState } from "react";
import { native } from "./api";
import type { Workspace } from "./model";

export function SessionTools({
  workspace,
  sessionId,
  onTheme,
  onNewSnippet,
  onImportHistory,
  onClose,
}: {
  workspace: Workspace;
  sessionId: string;
  onTheme: (theme: string) => void;
  onNewSnippet: () => void;
  onImportHistory: () => void;
  onClose: () => void;
}) {
  const [tab, setTab] = useState("Snippets");
  const [filter, setFilter] = useState("");
  const [error, setError] = useState("");
  async function run(command: string) {
    try {
      await native("write_session", { id: sessionId, data: `${command}\r` });
      setError("");
    } catch (e) {
      setError(String(e));
    }
  }
  return (
    <aside className="session-tools" aria-label="Session tools">
      <header>
        <strong>Session tools</strong>
        <button
          type="button"
          className="icon-button"
          aria-label="Close session tools"
          onClick={onClose}
        >
          <X size={16} />
        </button>
      </header>
      <nav aria-label="Session tools tabs">
        {[
          { name: "Snippets", icon: Code },
          { name: "History", icon: ClockCounterClockwise },
          { name: "Themes", icon: Palette },
        ].map(({ name, icon: Icon }) => (
          <button
            type="button"
            key={name}
            className={tab === name ? "chosen" : ""}
            aria-pressed={tab === name}
            onClick={() => setTab(name)}
          >
            <Icon size={17} />
            {name}
          </button>
        ))}
      </nav>
      <div className="tools-content">
        {tab === "Themes" ? (
          <>
            <p className="muted">Terminal appearance</p>
            <div className="theme-options">
              {["Graphite", "Midnight", "Paper"].map((theme) => (
                <button
                  type="button"
                  key={theme}
                  className={`theme-option ${theme.toLowerCase()} ${workspace.settings.theme === theme ? "chosen" : ""}`}
                  aria-pressed={workspace.settings.theme === theme}
                  onClick={() => onTheme(theme)}
                >
                  <div>
                    <span />
                    <span />
                    <span />
                  </div>
                  {theme}
                </button>
              ))}
            </div>
          </>
        ) : (
          <>
            <input
              aria-label={`Search ${tab.toLowerCase()}`}
              placeholder={`Search ${tab.toLowerCase()}…`}
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
            />
            <button
              type="button"
              className="tools-add"
              onClick={tab === "Snippets" ? onNewSnippet : onImportHistory}
            >
              <Plus size={16} />
              {tab === "Snippets" ? "New snippet" : "Import shell history"}
            </button>
            {(tab === "Snippets" ? workspace.snippets : workspace.history)
              .filter((item) =>
                `${"name" in item ? item.name : ""} ${item.command}`
                  .toLowerCase()
                  .includes(filter.toLowerCase()),
              )
              .map((item) => (
                <article className="tool-command" key={item.id}>
                  <strong>{"name" in item ? item.name : item.host}</strong>
                  <pre>{item.command}</pre>
                  <button type="button" onClick={() => void run(item.command)}>
                    <Play size={14} />
                    Run in terminal
                  </button>
                </article>
              ))}
            {(tab === "Snippets" ? workspace.snippets : workspace.history)
              .length === 0 && (
              <div className="tools-empty">
                <Code size={26} />
                <h3>
                  {tab === "Snippets"
                    ? "Keep useful commands close"
                    : "Your imported commands"}
                </h3>
                <p>
                  {tab === "Snippets"
                    ? "Save a snippet to run it in this session."
                    : "Import Bash or Zsh history from your Library."}
                </p>
              </div>
            )}
          </>
        )}
        {error && <p role="alert">{error}</p>}
      </div>
      <footer>Commands run in the active session.</footer>
    </aside>
  );
}
