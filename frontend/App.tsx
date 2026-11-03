import {
  ArrowRight,
  ArrowsLeftRight,
  ArrowUpRight,
  CheckCircle,
  CircleNotch,
  ClockCounterClockwise,
  Code,
  Copy,
  Desktop,
  Fingerprint,
  GearSix,
  Key,
  PencilSimple,
  Plus,
  HardDrives as Server,
  ShieldCheck,
  Terminal,
  Trash,
  UploadSimple,
  Warning,
  X,
} from "@phosphor-icons/react";
import { useCallback, useEffect, useRef, useState } from "react";
import { desktop, loadWorkspace, native, saveWorkspace } from "./api";
import fingerprintIcon from "./assets/tabius/fingerprint.svg";
import hostsIcon from "./assets/tabius/hosts.svg";
import keyIcon from "./assets/tabius/key.svg";
import forwardingIcon from "./assets/tabius/port-forwarding.svg";
import snippetsIcon from "./assets/tabius/snippets.svg";
import type { Editor } from "./Editors";
import { EditorModal } from "./Editors";
import type { Forward, Host, KnownHost, Workspace } from "./model";
import { defaults, emptyWorkspace, newForward, newHost } from "./model";
import { SettingsView } from "./Settings";
import { nativeFingerprints } from "./sync";
import type { Session } from "./Terminal";
import { TerminalPane } from "./Terminal";
import { Empty, Modal } from "./ui";

type Page =
  | "Hosts"
  | "Keychain"
  | "Port forwarding"
  | "Snippets"
  | "Known hosts"
  | "Settings";
const pages = [
  { name: "Hosts", icon: hostsIcon },
  { name: "Keychain", icon: keyIcon },
  { name: "Port forwarding", icon: forwardingIcon },
  { name: "Snippets", icon: snippetsIcon },
  { name: "Known hosts", icon: fingerprintIcon },
] as const;
export function App() {
  const [workspace, setWorkspace] = useState<Workspace>(emptyWorkspace);
  const [loaded, setLoaded] = useState(false);
  const [loadError, setLoadError] = useState("");
  const [page, setPage] = useState<Page>("Hosts");
  const [editor, setEditor] = useState<Editor | null>(null);
  const [sessions, setSessions] = useState<Session[]>([]);
  const [active, setActive] = useState("library");
  const [notice, setNotice] = useState("");
  const [known, setKnown] = useState<KnownHost[]>([]);
  const [historyTab, setHistoryTab] = useState(false);
  const [confirm, setConfirm] = useState<{
    message: string;
    action: () => void;
  } | null>(null);
  const [syncing, setSyncing] = useState(false);
  const workspaceRef = useRef(workspace);
  workspaceRef.current = workspace;
  const saveChain = useRef(Promise.resolve());
  useEffect(() => {
    void loadWorkspace()
      .then((value) => {
        setWorkspace({
          ...value,
          settings: { ...defaults, ...value.settings },
        });
        setLoaded(true);
      })
      .catch((err) => setLoadError(String(err)));
  }, []);
  useEffect(() => {
    if (!notice) return;
    const timer = setTimeout(() => setNotice(""), 6000);
    return () => clearTimeout(timer);
  }, [notice]);
  function update(next: Workspace) {
    setWorkspace(next);
    saveChain.current = saveChain.current
      .then(() => saveWorkspace(next))
      .catch((err) => {
        setNotice(`Could not save: ${String(err)}`);
      });
  }
  const refreshKnown = useCallback(() => {
    if (desktop)
      void native<KnownHost[]>("known_hosts")
        .then(setKnown)
        .catch((err) => setNotice(String(err)));
  }, []);
  useEffect(() => {
    if (page === "Known hosts") refreshKnown();
  }, [page, refreshKnown]);
  function connect(host: Host, forward?: Forward) {
    const session = {
      id: crypto.randomUUID(),
      host: {
        ...host,
        startup:
          workspace.snippets.find((s) => s.id === host.startup)?.command ?? "",
      },
      forward,
      fingerprints: (workspace.sync?.fingerprints ?? [])
        .filter(
          (f) =>
            f.host.toLowerCase() === host.address.toLowerCase() &&
            f.port === host.port,
        )
        .map((f) => `${f.type} SHA256:${f.digest.replace(/=+$/, "")}`),
    };
    setSessions((items) => [...items, session]);
    setActive(session.id);
  }
  function remove(
    collection: "hosts" | "keys" | "snippets" | "forwards",
    id: string,
  ) {
    setConfirm({
      message: `Delete this ${collection === "keys" ? "key" : collection === "forwards" ? "forwarding rule" : collection.slice(0, -1)}? This cannot be undone.`,
      action: () => {
        if (collection === "keys") {
          void native("delete_key", { id })
            .then(() =>
              update({
                ...workspace,
                keys: workspace.keys.filter((k) => k.id !== id),
                hosts: workspace.hosts.map((h) =>
                  h.keyId === id ? { ...h, keyId: "" } : h,
                ),
              }),
            )
            .catch((err) => setNotice(String(err)));
        } else if (collection === "hosts")
          update({
            ...workspace,
            hosts: workspace.hosts.filter((h) => h.id !== id),
            forwards: workspace.forwards.filter((f) => f.hostId !== id),
          });
        else
          update({
            ...workspace,
            [collection]: workspace[collection].filter(
              (item) => item.id !== id,
            ),
          });
      },
    });
  }
  const activeSession = sessions.find((s) => s.id === active);
  const localFingerprints = nativeFingerprints(known);
  const syncedKnown = (workspace.sync?.fingerprints ?? [])
    .filter(
      (f) =>
        !localFingerprints.some((k) => k.id === f.id && k.digest === f.digest),
    )
    .map((f) => ({
      line: f.id,
      host: f.port === 22 ? f.host : `[${f.host}]:${f.port}`,
      kind: f.type,
      fingerprint: `SHA256:${f.digest.replace(/=+$/, "")}`,
      synced: true,
    }));
  const displayedKnown = [...known, ...syncedKnown];
  const counts = [
    workspace.hosts.length,
    workspace.keys.length,
    workspace.forwards.length,
    workspace.snippets.length,
    displayedKnown.length,
  ];
  return (
    <div className="app-shell">
      <aside className="sidebar" inert={syncing}>
        <div className="brand">
          <div className="brand-mark">
            <Terminal size={23} weight="bold" />
          </div>
          <span>tabius</span>
        </div>
        <nav>
          {pages.map(({ name, icon }, i) => (
            <button
              type="button"
              key={name}
              className={`nav-item ${page === name && active === "library" ? "selected" : ""}`}
              aria-current={
                page === name && active === "library" ? "page" : undefined
              }
              onClick={() => {
                setPage(name);
                setActive("library");
              }}
            >
              <span
                className="nav-icon"
                style={{
                  maskImage: `url("${icon}")`,
                  WebkitMaskImage: `url("${icon}")`,
                }}
                aria-hidden="true"
              />
              <span>{name}</span>
              {counts[i] > 0 && <span className="nav-count">{counts[i]}</span>}
            </button>
          ))}
        </nav>
        <div className="sidebar-bottom">
          <button
            type="button"
            className={`nav-item ${page === "Settings" && active === "library" ? "selected" : ""}`}
            aria-current={
              page === "Settings" && active === "library" ? "page" : undefined
            }
            onClick={() => {
              setPage("Settings");
              setActive("library");
            }}
          >
            <GearSix size={19} />
            <span>Settings</span>
          </button>
        </div>
      </aside>
      <main className="main">
        {sessions.length > 0 && (
          <div className="tabbar">
            {sessions.map((s) => (
              <div
                key={s.id}
                className={`session-tab ${active === s.id ? "active" : ""}`}
              >
                <button type="button" onClick={() => setActive(s.id)}>
                  <Terminal size={15} />
                  {s.host.name}
                  {s.forward ? " · Tunnel" : ""}
                </button>
                <button
                  type="button"
                  aria-label={`Close ${s.host.name}`}
                  onClick={() => {
                    setSessions((items) =>
                      items.filter((item) => item.id !== s.id),
                    );
                    if (active === s.id) setActive("library");
                  }}
                >
                  <X size={13} />
                </button>
              </div>
            ))}
            <div className="spacer" />
          </div>
        )}
        {loadError ? (
          <div className="empty-state">
            <Warning size={30} />
            <h2>Workspace could not be loaded</h2>
            <p>{loadError}</p>
            <p>Your saved files have not been overwritten.</p>
          </div>
        ) : !loaded ? (
          <div className="empty-state">
            <CircleNotch className="spin" size={30} />
            <p>Opening workspace…</p>
          </div>
        ) : (
          <>
            <section className="library" hidden={active !== "library"}>
              <header className="page-header" inert={syncing}>
                <div className="heading-row">
                  <div>
                    <h1>{page}</h1>
                  </div>
                  {page === "Hosts" && (
                    <button
                      type="button"
                      className="primary"
                      onClick={() =>
                        setEditor({ type: "host", value: newHost() })
                      }
                    >
                      <Plus size={17} />
                      New host
                    </button>
                  )}
                  {page === "Keychain" && (
                    <button
                      type="button"
                      className="primary"
                      onClick={() => setEditor({ type: "key" })}
                    >
                      <Plus size={17} />
                      New key
                    </button>
                  )}
                  {page === "Port forwarding" && (
                    <button
                      type="button"
                      className="primary"
                      onClick={() =>
                        setEditor({ type: "forward", value: newForward() })
                      }
                    >
                      <Plus size={17} />
                      New forwarding
                    </button>
                  )}
                  {page === "Snippets" && (
                    <button
                      type="button"
                      className="primary"
                      onClick={() =>
                        setEditor({
                          type: "snippet",
                          value: {
                            id: crypto.randomUUID(),
                            name: "",
                            command: "",
                          },
                        })
                      }
                    >
                      <Plus size={17} />
                      New snippet
                    </button>
                  )}
                  {page === "Known hosts" && (
                    <button
                      type="button"
                      onClick={() => setEditor({ type: "known" })}
                    >
                      <UploadSimple size={17} />
                      Import known hosts
                    </button>
                  )}
                </div>
              </header>
              <div className="page-content" key={page}>
                {syncing && <div className="sync-lock" aria-hidden="true" />}
                {page === "Hosts" &&
                  (workspace.hosts.length ? (
                    <>
                      <div className="section-label">
                        Hosts <span>{workspace.hosts.length}</span>
                      </div>
                      <div className="host-grid">
                        {workspace.hosts.map((host) => (
                          <article className="host-card" key={host.id}>
                            <div className="card-top">
                              <div className="host-icon">
                                <Server size={24} />
                              </div>
                              <div className="card-actions">
                                <button
                                  type="button"
                                  aria-label={`Edit ${host.name}`}
                                  onClick={() =>
                                    setEditor({ type: "host", value: host })
                                  }
                                >
                                  <PencilSimple size={16} />
                                </button>
                                <button
                                  type="button"
                                  aria-label={`Delete ${host.name}`}
                                  onClick={() => remove("hosts", host.id)}
                                >
                                  <Trash size={16} />
                                </button>
                              </div>
                            </div>
                            <h3>{host.name}</h3>
                            <p className="mono">
                              {host.address}
                              <span className="host-port">:{host.port}</span>
                            </p>
                            <div className="card-bottom">
                              <span>
                                <Desktop size={14} />
                                {host.system}
                              </span>
                              <button
                                type="button"
                                onClick={() => connect(host)}
                              >
                                Connect
                                <ArrowUpRight size={16} />
                              </button>
                            </div>
                          </article>
                        ))}
                      </div>
                    </>
                  ) : (
                    <Empty
                      icon={<Server size={31} />}
                      title="No hosts"
                      text="Add a host to save its connection settings."
                      action={
                        <button
                          type="button"
                          className="primary"
                          onClick={() =>
                            setEditor({ type: "host", value: newHost() })
                          }
                        >
                          <Plus size={17} />
                          Add your first host
                        </button>
                      }
                    />
                  ))}
                {page === "Keychain" &&
                  (workspace.keys.length ? (
                    <>
                      <div className="section-label">
                        Keys <span>{workspace.keys.length}</span>
                      </div>
                      <div className="item-list">
                        {workspace.keys.map((key) => (
                          <article className="list-item" key={key.id}>
                            <div className="host-icon">
                              <Key size={23} />
                            </div>
                            <div className="item-body">
                              <h3>
                                {key.name}
                                <span className="badge">{key.kind}</span>
                              </h3>
                              <p className="mono">{key.fingerprint}</p>
                            </div>
                            <button
                              type="button"
                              onClick={() => {
                                void navigator.clipboard
                                  .writeText(key.publicKey)
                                  .then(() => setNotice("Public key copied."))
                                  .catch((err) => setNotice(String(err)));
                              }}
                            >
                              <Copy size={16} />
                              Public key
                            </button>
                            <button
                              type="button"
                              className="icon-button"
                              aria-label={`Delete ${key.name}`}
                              onClick={() => remove("keys", key.id)}
                            >
                              <Trash size={17} />
                            </button>
                          </article>
                        ))}
                      </div>
                    </>
                  ) : (
                    <Empty
                      icon={<Key size={31} />}
                      title="No keys"
                      text="Generate or import an SSH key."
                      action={
                        <button
                          type="button"
                          className="primary"
                          onClick={() => setEditor({ type: "key" })}
                        >
                          <Plus size={17} />
                          Create or import a key
                        </button>
                      }
                    />
                  ))}
                {page === "Port forwarding" &&
                  (workspace.forwards.length ? (
                    <div className="item-list">
                      {workspace.forwards.map((rule) => (
                        <article className="list-item" key={rule.id}>
                          <div className="host-icon">
                            <ArrowsLeftRight size={23} />
                          </div>
                          <div className="item-body">
                            <h3>
                              {rule.name}
                              <span className="badge">{rule.kind}</span>
                            </h3>
                            <p className="mono">
                              {rule.bindAddress}:{rule.bindPort}{" "}
                              {rule.kind !== "dynamic" &&
                                `→ ${rule.destination}:${rule.destinationPort}`}
                            </p>
                            <p>
                              {workspace.hosts.find((h) => h.id === rule.hostId)
                                ?.name ?? "Host removed"}
                            </p>
                          </div>
                          <button
                            type="button"
                            onClick={() => {
                              const host = workspace.hosts.find(
                                (h) => h.id === rule.hostId,
                              );
                              if (host) connect(host, rule);
                            }}
                          >
                            Open tunnel
                            <ArrowRight size={16} />
                          </button>
                          <button
                            type="button"
                            className="icon-button"
                            aria-label="Edit forwarding"
                            onClick={() =>
                              setEditor({ type: "forward", value: rule })
                            }
                          >
                            <PencilSimple size={17} />
                          </button>
                          <button
                            type="button"
                            className="icon-button"
                            aria-label="Delete forwarding"
                            onClick={() => remove("forwards", rule.id)}
                          >
                            <Trash size={17} />
                          </button>
                        </article>
                      ))}
                    </div>
                  ) : (
                    <Empty
                      icon={<ArrowsLeftRight size={31} />}
                      title="No forwarding rules"
                      text="Access a remote service locally, expose a local port through a server, or create a SOCKS proxy."
                      action={
                        <button
                          type="button"
                          className="primary"
                          onClick={() =>
                            setEditor({ type: "forward", value: newForward() })
                          }
                        >
                          <Plus size={17} />
                          Create a forwarding rule
                        </button>
                      }
                    />
                  ))}
                {page === "Snippets" && (
                  <>
                    <div className="segmented content-tabs">
                      <button
                        type="button"
                        className={!historyTab ? "chosen" : ""}
                        onClick={() => setHistoryTab(false)}
                      >
                        <Code size={16} />
                        Snippets <span>{workspace.snippets.length}</span>
                      </button>
                      <button
                        type="button"
                        className={historyTab ? "chosen" : ""}
                        onClick={() => setHistoryTab(true)}
                      >
                        <ClockCounterClockwise size={16} />
                        Shell history <span>{workspace.history.length}</span>
                      </button>
                    </div>
                    {historyTab ? (
                      <>
                        <div className="section-row">
                          <p className="muted">
                            Import your shell history to turn useful commands
                            into snippets.
                          </p>
                          <button
                            type="button"
                            onClick={() => setEditor({ type: "history" })}
                          >
                            <UploadSimple size={16} />
                            Import history
                          </button>
                        </div>
                        {workspace.history.length ? (
                          workspace.history.map((item) => (
                            <article className="history-item" key={item.id}>
                              <Code size={18} />
                              <div className="item-body">
                                <code>{item.command}</code>
                                <p>
                                  {item.host} ·{" "}
                                  {new Date(item.date).toLocaleDateString()}
                                </p>
                              </div>
                              <button
                                type="button"
                                onClick={() =>
                                  setEditor({
                                    type: "snippet",
                                    value: {
                                      id: crypto.randomUUID(),
                                      name: "",
                                      command: item.command,
                                    },
                                  })
                                }
                              >
                                <Plus size={15} />
                                Save as snippet
                              </button>
                            </article>
                          ))
                        ) : (
                          <Empty
                            icon={<ClockCounterClockwise size={31} />}
                            title="No shell history"
                            text="Import Bash or Zsh history from a file or paste it here. Select any command to make it a reusable snippet."
                            action={
                              <button
                                type="button"
                                onClick={() => setEditor({ type: "history" })}
                              >
                                Import shell history
                              </button>
                            }
                          />
                        )}
                      </>
                    ) : workspace.snippets.length ? (
                      <div className="snippet-grid">
                        {workspace.snippets.map((snippet) => (
                          <article key={snippet.id} className="snippet-card">
                            <div className="section-row">
                              <h3>
                                <Code size={18} />
                                {snippet.name}
                              </h3>
                              <button
                                type="button"
                                className="icon-button"
                                aria-label="Edit snippet"
                                onClick={() =>
                                  setEditor({ type: "snippet", value: snippet })
                                }
                              >
                                <PencilSimple size={16} />
                              </button>
                            </div>
                            <pre>{snippet.command}</pre>
                            <div className="section-row">
                              <button
                                type="button"
                                onClick={() => {
                                  void navigator.clipboard
                                    .writeText(snippet.command)
                                    .then(() => setNotice("Snippet copied."))
                                    .catch((err) => setNotice(String(err)));
                                }}
                              >
                                <Copy size={15} />
                                Copy
                              </button>
                              <button
                                type="button"
                                className="icon-button"
                                aria-label="Delete snippet"
                                onClick={() => remove("snippets", snippet.id)}
                              >
                                <Trash size={16} />
                              </button>
                            </div>
                            <label className="run-snippet">
                              Run in session
                              <select
                                defaultValue=""
                                onChange={(e) => {
                                  const id = e.target.value;
                                  if (id)
                                    void native("write_session", {
                                      id,
                                      data: `${snippet.command}\r`,
                                    })
                                      .then(() => {
                                        setActive(id);
                                        setNotice(
                                          "Snippet sent to the terminal for execution.",
                                        );
                                      })
                                      .catch((err) => setNotice(String(err)));
                                  e.target.value = "";
                                }}
                              >
                                <option value="">Choose an open session</option>
                                {sessions
                                  .filter((s) => !s.forward)
                                  .map((s) => (
                                    <option key={s.id} value={s.id}>
                                      {s.host.name}
                                    </option>
                                  ))}
                              </select>
                            </label>
                          </article>
                        ))}
                      </div>
                    ) : (
                      <Empty
                        icon={<Code size={31} />}
                        title="No snippets"
                        text="Save commands and scripts for reuse."
                        action={
                          <button
                            type="button"
                            className="primary"
                            onClick={() =>
                              setEditor({
                                type: "snippet",
                                value: {
                                  id: crypto.randomUUID(),
                                  name: "",
                                  command: "",
                                },
                              })
                            }
                          >
                            <Plus size={17} />
                            Add your first snippet
                          </button>
                        }
                      />
                    )}
                  </>
                )}
                {page === "Known hosts" &&
                  (displayedKnown.length ? (
                    <div className="item-list">
                      {displayedKnown.map((item) => (
                        <article className="list-item" key={item.line}>
                          <div className="host-icon">
                            <Fingerprint size={23} />
                          </div>
                          <div className="item-body">
                            <h3>
                              {item.host}
                              <span className="badge">{item.kind}</span>
                            </h3>
                            <p className="mono">{item.fingerprint}</p>
                          </div>
                          {item.synced ? (
                            <span className="badge">Reference fingerprint</span>
                          ) : (
                            <ShieldCheck size={20} className="accent" />
                          )}
                          <button
                            type="button"
                            className="icon-button"
                            aria-label="Remove trusted fingerprint"
                            onClick={() =>
                              setConfirm({
                                message: `Remove trust for ${item.host}? You will be asked to verify this host again.`,
                                action: () => {
                                  const ids = item.synced
                                    ? [item.line]
                                    : nativeFingerprints([item]).map(
                                        (f) => f.id,
                                      );
                                  if (workspace.sync)
                                    update({
                                      ...workspace,
                                      sync: {
                                        ...workspace.sync,
                                        fingerprints:
                                          workspace.sync.fingerprints.filter(
                                            (f) => !ids.includes(f.id),
                                          ),
                                      },
                                    });
                                  if (item.synced) return;
                                  void native("remove_known_host", {
                                    line: item.line,
                                  })
                                    .then(refreshKnown)
                                    .catch((err) => setNotice(String(err)));
                                },
                              })
                            }
                          >
                            <Trash size={17} />
                          </button>
                        </article>
                      ))}
                    </div>
                  ) : (
                    <Empty
                      icon={<Fingerprint size={31} />}
                      title="No known hosts"
                      text="Server identities appear here after you verify them on your first connection. Changed keys are never trusted automatically."
                    />
                  ))}
                {page === "Settings" && (
                  <SettingsView
                    settings={workspace.settings}
                    workspace={workspace}
                    syncing={syncing}
                    onSyncBusy={setSyncing}
                    isCurrent={() => workspaceRef.current === workspace}
                    onSyncApply={async (next) => {
                      await saveChain.current;
                      const removed = (workspace.sync?.knownBaseline ?? [])
                        .filter(
                          (previous) =>
                            !next.sync?.fingerprints.some(
                              (current) =>
                                current.id === previous.id &&
                                current.digest === previous.digest,
                            ),
                        )
                        .map(
                          (previous) =>
                            `${previous.id}|${previous.digest.replace(/=+$/, "")}`,
                        );
                      if (removed.length)
                        await native("prune_synced_fingerprints", { removed });
                      await saveWorkspace(next);
                      setWorkspace(next);
                      for (const key of workspace.keys.filter(
                        (key) =>
                          !next.keys.some((current) => current.id === key.id),
                      ))
                        await native("delete_key", { id: key.id }).catch(() =>
                          setNotice(
                            "Sync saved, but an unused private key file could not be removed.",
                          ),
                        );
                      refreshKnown();
                    }}
                    onChange={(settings) => update({ ...workspace, settings })}
                  />
                )}
              </div>
            </section>
            {sessions.map((session) => (
              <div
                key={session.id}
                className="session-container"
                hidden={activeSession?.id !== session.id}
              >
                <TerminalPane
                  session={session}
                  settings={workspace.settings}
                  onClose={() => {
                    setSessions((items) =>
                      items.filter((item) => item.id !== session.id),
                    );
                    setActive("library");
                  }}
                  onOS={(id, system) => {
                    setWorkspace((current) => {
                      const next = {
                        ...current,
                        hosts: current.hosts.map((h) =>
                          h.id === id ? { ...h, system } : h,
                        ),
                      };
                      saveChain.current = saveChain.current
                        .then(() => saveWorkspace(next))
                        .catch((err) => setNotice(String(err)));
                      return next;
                    });
                  }}
                />
              </div>
            ))}
          </>
        )}
      </main>
      {notice && (
        <div className="toast" role="status">
          <CheckCircle size={19} />
          {notice}
          <button
            type="button"
            aria-label="Dismiss notification"
            onClick={() => setNotice("")}
          >
            <X size={15} />
          </button>
        </div>
      )}
      {editor && (
        <EditorModal
          editor={editor}
          workspace={workspace}
          onClose={() => setEditor(null)}
          onSave={(next) => {
            update(next);
            setEditor(null);
            setNotice("Saved.");
          }}
          onKnown={refreshKnown}
        />
      )}
      {confirm && (
        <Modal title="Confirm deletion" onClose={() => setConfirm(null)}>
          <p>{confirm.message}</p>
          <div className="modal-actions">
            <button type="button" onClick={() => setConfirm(null)}>
              Cancel
            </button>
            <button
              type="button"
              className="danger"
              onClick={() => {
                confirm.action();
                setConfirm(null);
              }}
            >
              Delete
            </button>
          </div>
        </Modal>
      )}
    </div>
  );
}
