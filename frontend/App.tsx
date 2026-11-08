import {
  ArrowRight,
  ArrowsLeftRight,
  CheckCircle,
  CircleNotch,
  ClockCounterClockwise,
  Code,
  Copy,
  Fingerprint,
  FolderPlus,
  GearSix,
  Key,
  PencilSimple,
  Plus,
  HardDrives as Server,
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
import { HostLibrary } from "./HostLibrary";
import { HostPicker } from "./HostPicker";
import type {
  Forward,
  Host,
  HostGroup,
  KnownHost,
  Snippet,
  Workspace,
} from "./model";
import { SettingsView } from "./Settings";
import { SSHConfigImport } from "./SSHConfigImport";
import { useShortcuts } from "./shortcuts";
import { adjacentTab, cycleTab, reorderTabs, Tabs } from "./Tabs";
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
  const [workspace, setWorkspace] = useState<Workspace>({
    hosts: [],
    keys: [],
    snippets: [],
    forwards: [],
    history: [],
    settings: {} as Workspace["settings"],
  });
  const [loaded, setLoaded] = useState(false);
  const [loadError, setLoadError] = useState("");
  const [page, setPage] = useState<Page>("Hosts");
  const [editor, setEditor] = useState<Editor | null>(null);
  const [importingSSH, setImportingSSH] = useState(false);
  const [sessions, setSessions] = useState<Session[]>([]);
  const [active, setActive] = useState("library");
  const [focusRequest, setFocusRequest] = useState(0);
  const [notice, setNotice] = useState("");
  const [known, setKnown] = useState<KnownHost[]>([]);
  const [historyTab, setHistoryTab] = useState(false);
  const [confirm, setConfirm] = useState<{
    message: string;
    action: () => void;
  } | null>(null);
  const [syncing, setSyncing] = useState(false);
  const [choosingHost, setChoosingHost] = useState(false);
  const workspaceRef = useRef(workspace);
  workspaceRef.current = workspace;
  const saveChain = useRef(Promise.resolve());
  useEffect(() => {
    void loadWorkspace()
      .then((value) => {
        setWorkspace(value);
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
    if (loaded) refreshKnown();
  }, [loaded, refreshKnown]);
  useEffect(() => {
    if (loaded && page === "Known hosts") refreshKnown();
  }, [loaded, page, refreshKnown]);
  async function connect(host: Host, forward?: Forward) {
    try {
      const session = await native<Session>("prepare_session", {
        workspace,
        host,
        forward: forward ?? null,
      });
      setSessions((items) => [...items, session]);
      selectTab(session.id);
    } catch (err) {
      setNotice(String(err));
    }
  }
  async function createEditor(
    type: "host" | "forward" | "snippet" | "group",
    groupId = "",
  ) {
    try {
      if (type === "host")
        setEditor({
          type,
          value: {
            ...(await native<Host>("new_item", { kind: type })),
            groupId,
          },
        });
      else if (type === "forward")
        setEditor({
          type,
          value: await native<Forward>("new_item", { kind: type }),
        });
      else if (type === "group")
        setEditor({
          type,
          value: await native<HostGroup>("new_item", { kind: type }),
        });
      else
        setEditor({
          type,
          value: await native<Snippet>("new_item", { kind: type }),
        });
    } catch (err) {
      setNotice(String(err));
    }
  }
  function remove(
    collection: "hosts" | "keys" | "snippets" | "forwards" | "groups",
    id: string,
  ) {
    setConfirm({
      message:
        collection === "groups"
          ? "Delete this group? Its hosts will remain in Ungrouped."
          : `Delete this ${collection === "keys" ? "key" : collection === "forwards" ? "forwarding rule" : collection.slice(0, -1)}? This cannot be undone.`,
      action: () => {
        void (async () => {
          update(
            await native("remove_item", {
              workspace,
              kind:
                collection === "keys"
                  ? "key"
                  : collection === "hosts"
                    ? "host"
                    : collection === "forwards"
                      ? "forward"
                      : collection === "groups"
                        ? "group"
                        : "snippet",
              item: { id },
            }),
          );
        })().catch((err) => setNotice(String(err)));
      },
    });
  }
  function selectTab(id: string, focusTerminal = true) {
    setActive(id);
    if (focusTerminal) setFocusRequest((value) => value + 1);
  }
  function closeTab(id: string) {
    setSessions((items) => items.filter((item) => item.id !== id));
    if (active === id) selectTab(adjacentTab(sessions, id));
    else setFocusRequest((value) => value + 1);
  }
  useShortcuts(
    workspace.settings.shortcuts,
    loaded && !syncing && !editor && !confirm && !importingSSH && !choosingHost,
    (action) => {
      if (action === "newTab") setChoosingHost(true);
      else if (action === "closeTab" && active !== "library") closeTab(active);
      else if (action === "nextTab" || action === "previousTab")
        selectTab(cycleTab(sessions, active, action === "nextTab" ? 1 : -1));
      else if (action === "snippets" || action === "library") {
        setPage(action === "snippets" ? "Snippets" : "Hosts");
        setHistoryTab(false);
        setActive("library");
      } else if (action.startsWith("tab")) {
        const index =
          action === "tab9" ? sessions.length - 1 : Number(action.slice(3)) - 1;
        if (sessions[index]) selectTab(sessions[index].id);
      }
    },
  );
  const activeSession = sessions.find((s) => s.id === active);
  const [displayedKnown, setDisplayedKnown] = useState<KnownHost[]>([]);
  useEffect(() => {
    if (loaded)
      void native<KnownHost[]>("display_known", { workspace, known })
        .then(setDisplayedKnown)
        .catch((err) => setNotice(String(err)));
  }, [workspace, known, loaded]);
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
              {loaded && counts[i] > 0 && (
                <span className="nav-count">{counts[i]}</span>
              )}
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
        {loaded && (
          <Tabs
            sessions={sessions}
            active={active}
            disabled={syncing}
            onSelect={selectTab}
            onClose={closeTab}
            onNew={() => setChoosingHost(true)}
            onReorder={(source, target, after) =>
              setSessions((items) => reorderTabs(items, source, target, after))
            }
          />
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
                    <>
                      <div className="spacer" />
                      <button
                        type="button"
                        onClick={() => setImportingSSH(true)}
                      >
                        <UploadSimple size={17} />
                        Import SSH config
                      </button>
                      <button
                        type="button"
                        onClick={() => createEditor("group")}
                      >
                        <FolderPlus size={17} />
                        New group
                      </button>
                      <button
                        type="button"
                        className="primary"
                        onClick={() => createEditor("host")}
                      >
                        <Plus size={17} />
                        New host
                      </button>
                    </>
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
                      onClick={() => createEditor("forward")}
                    >
                      <Plus size={17} />
                      New forwarding
                    </button>
                  )}
                  {page === "Snippets" && (
                    <button
                      type="button"
                      className="primary"
                      onClick={() => createEditor("snippet")}
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
                  (workspace.hosts.length || workspace.groups?.length ? (
                    <HostLibrary
                      workspace={workspace}
                      onConnect={(host) => void connect(host)}
                      onEdit={(host) =>
                        setEditor({ type: "host", value: host })
                      }
                      onDelete={(host) => remove("hosts", host.id)}
                      onEditGroup={(group) =>
                        setEditor({ type: "group", value: group })
                      }
                      onDeleteGroup={(group) => remove("groups", group.id)}
                      onNewHost={(groupId) =>
                        void createEditor("host", groupId)
                      }
                    />
                  ) : (
                    <Empty
                      icon={<Server size={31} />}
                      title="No hosts"
                      text="Add a host to save its connection settings."
                      action={
                        <button
                          type="button"
                          className="primary"
                          onClick={() => createEditor("host")}
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
                          onClick={() => createEditor("forward")}
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
                                      id: "",
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
                                    void native("run_snippet", {
                                      id,
                                      snippetId: snippet.id,
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
                            onClick={() => createEditor("snippet")}
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
                          {item.synced && (
                            <span className="badge">Reference fingerprint</span>
                          )}
                          <button
                            type="button"
                            className="icon-button"
                            aria-label="Remove trusted fingerprint"
                            onClick={() =>
                              setConfirm({
                                message: `Remove trust for ${item.host}? You will be asked to verify this host again.`,
                                action: () => {
                                  void native<Workspace>("forget_fingerprint", {
                                    workspace,
                                    item,
                                  })
                                    .then((next) => {
                                      update(next);
                                      refreshKnown();
                                    })
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
                      const warnings = await native<string[]>("apply_sync", {
                        previous: workspace,
                        workspace: next,
                      });
                      setWorkspace(next);
                      if (warnings.length) setNotice(warnings.join(" "));
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
                  active={active === session.id}
                  focusRequest={focusRequest}
                  settings={workspace.settings}
                  onClose={() => closeTab(session.id)}
                  onOS={(id, system) => {
                    void native<Workspace>("edit_workspace", {
                      workspace: workspaceRef.current,
                      kind: "os",
                      item: { id, system },
                      remove: false,
                    })
                      .then(update)
                      .catch((err) => setNotice(String(err)));
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
      {choosingHost && (
        <HostPicker
          workspace={workspace}
          onClose={() => setChoosingHost(false)}
          onChoose={(host) => {
            setChoosingHost(false);
            void connect(host);
          }}
          onNewHost={() => {
            setChoosingHost(false);
            void createEditor("host");
          }}
        />
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
      {importingSSH && (
        <SSHConfigImport
          workspace={workspace}
          onClose={() => setImportingSSH(false)}
          onPrepare={() => saveChain.current}
          onSave={(next, message) => {
            setWorkspace(next);
            setImportingSSH(false);
            setNotice(message);
          }}
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
