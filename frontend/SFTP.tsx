import {
  ArrowClockwise,
  ArrowDown,
  ArrowLeft,
  ArrowUp,
  File,
  Folder,
  HardDrives,
  Laptop,
  Plug,
  Plus,
  X,
} from "@phosphor-icons/react";
import { Channel } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { native } from "./api";
import { HostKeyPrompt, type HostKeyRequest } from "./HostKeyPrompt";
import type { Host, Workspace } from "./model";
import { Modal } from "./ui";

export interface FileEntry {
  name: string;
  directory: boolean;
  size: number;
  modified: number;
}
interface Listing {
  path: string;
  entries: FileEntry[];
}
export function joinPath(path: string, name: string) {
  return `${path.replace(/[\\/]+$/, "")}/${name}`;
}
export function parentPath(path: string) {
  if (/^[A-Za-z]:[\\/]*$/.test(path)) return `${path.slice(0, 2)}/`;
  const parent =
    path.replace(/[\\/]+$/, "").replace(/[\\/][^\\/]*$/, "") || "/";
  return /^[A-Za-z]:$/.test(parent) ? `${parent}/` : parent;
}
export function validName(name: string) {
  return (
    name.trim().length > 0 &&
    name !== "." &&
    name !== ".." &&
    !/[\\/]/.test(name) &&
    !Array.from(name).some((character) => character.charCodeAt(0) < 32)
  );
}
export function fileSize(size: number) {
  if (size < 1024) return `${size} B`;
  if (size < 1048576) return `${(size / 1024).toFixed(1)} KB`;
  return `${(size / 1048576).toFixed(1)} MB`;
}
export function SFTP({
  workspace,
  onTrustChanged,
}: {
  workspace: Workspace;
  onTrustChanged: () => void;
}) {
  const [local, setLocal] = useState<Listing>({ path: "", entries: [] });
  const [remote, setRemote] = useState<Listing>({ path: ".", entries: [] });
  const [host, setHost] = useState<Host>();
  const [connection, setConnection] = useState<string>();
  const connectionRef = useRef<string | undefined>(undefined);
  const [request, setRequest] = useState<HostKeyRequest>();
  const [busy, setBusy] = useState(false);
  const [connecting, setConnecting] = useState(false);
  const [error, setError] = useState("");
  const [status, setStatus] = useState("");
  const [selection, setSelection] = useState<{
    side: "local" | "remote";
    entry: FileEntry;
  }>();
  const [hidden, setHidden] = useState(false);
  const [filter, setFilter] = useState("");
  const [edit, setEdit] = useState<{
    side: "local" | "remote";
    action: "mkdir" | "rename" | "delete";
  }>();
  const [name, setName] = useState("");
  async function load(
    side: "local" | "remote",
    path: string,
    id = connectionRef.current,
  ) {
    const listing = await native<Listing>("sftp_list", {
      id: side === "remote" ? id : null,
      path,
    });
    (side === "local" ? setLocal : setRemote)(listing);
    setSelection(undefined);
  }
  useEffect(() => {
    void native<Listing>("sftp_list", { id: null, path: "" })
      .then(setLocal)
      .catch((e) => setError(String(e)));
    return () => {
      if (connectionRef.current)
        void native("sftp_disconnect", { id: connectionRef.current }).catch(
          () => {},
        );
    };
  }, []);
  async function perform(work: () => Promise<void>) {
    setBusy(true);
    setError("");
    setStatus("");
    try {
      await work();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  function disconnect() {
    const id = connectionRef.current;
    connectionRef.current = undefined;
    setConnection(undefined);
    setRequest(undefined);
    setHost(undefined);
    setSelection(undefined);
    if (id)
      void native("sftp_disconnect", { id }).catch((e) => setError(String(e)));
  }
  async function connect(next: Host) {
    const id = crypto.randomUUID();
    connectionRef.current = id;
    setConnection(id);
    setHost(next);
    setConnecting(true);
    await perform(async () => {
      try {
        const channel = new Channel<HostKeyRequest>();
        channel.onmessage = (message) => {
          if (connectionRef.current === id) setRequest(message);
        };
        await native("sftp_connect", {
          id,
          host: next,
          settings: workspace.settings,
          onKey: channel,
        });
        if (connectionRef.current !== id) {
          await native("sftp_disconnect", { id });
          return;
        }
        await load("remote", ".", id);
        onTrustChanged();
      } catch (e) {
        if (connectionRef.current === id) disconnect();
        throw e;
      }
    });
    setConnecting(false);
  }
  async function transfer() {
    if (!selection || !connection) return;
    const { entry, side } = selection;
    await perform(async () => {
      await native("sftp_transfer", {
        id: connection,
        local: joinPath(local.path, entry.name),
        remote: joinPath(remote.path, entry.name),
        upload: side === "local",
      });
      await load(
        side === "local" ? "remote" : "local",
        side === "local" ? remote.path : local.path,
      );
      setStatus(`Transferred ${entry.name}`);
    });
  }
  async function modify() {
    if (!edit) return;
    const { side, action } = edit;
    const listing = side === "local" ? local : remote;
    const entry = selection?.side === side ? selection.entry : undefined;
    await perform(async () => {
      await native("sftp_modify", {
        id: side === "remote" ? connection : null,
        action: action === "delete" && entry?.directory ? "rmdir" : action,
        path: joinPath(
          listing.path,
          action === "mkdir" ? name : (entry?.name ?? ""),
        ),
        destination: joinPath(listing.path, name),
      });
      setEdit(undefined);
      await load(side, listing.path);
    });
  }
  return (
    <section className="sftp-workspace" aria-label="SFTP file browser">
      <header className="sftp-heading">
        <div>
          <h1>File transfer</h1>
          <p>Local files and remote servers, side by side.</p>
        </div>
        <label className="sftp-hidden">
          <input
            type="checkbox"
            checked={hidden}
            onChange={(e) => setHidden(e.target.checked)}
          />{" "}
          Show hidden files
        </label>
        <input
          aria-label="Filter files"
          placeholder="Filter files…"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />
      </header>
      <div className="file-panes">
        {(["local", "remote"] as const).map((side) => {
          const listing = side === "local" ? local : remote;
          const connected = side === "local" || (connection && !connecting);
          const selected =
            selection?.side === side ? selection.entry : undefined;
          const entries = listing.entries
            .filter(
              (entry) =>
                (hidden || !entry.name.startsWith(".")) &&
                entry.name.toLowerCase().includes(filter.toLowerCase()),
            )
            .sort(
              (a, b) =>
                Number(b.directory) - Number(a.directory) ||
                a.name.localeCompare(b.name),
            );
          return (
            <section
              className="file-pane"
              key={side}
              aria-label={side === "local" ? "Local files" : "Remote files"}
            >
              <header className="file-pane-heading">
                {side === "local" ? (
                  <Laptop size={20} />
                ) : (
                  <HardDrives size={20} />
                )}
                <div>
                  <strong>
                    {side === "local"
                      ? "This computer"
                      : (host?.name ?? "Remote server")}
                  </strong>
                  <span>
                    {side === "local"
                      ? "Local filesystem"
                      : host
                        ? `${host.username ? `${host.username}@` : ""}${host.address}`
                        : "SFTP over SSH"}
                  </span>
                </div>
                {connection && side === "remote" && (
                  <button
                    type="button"
                    className="icon-button"
                    aria-label="Disconnect SFTP"
                    onClick={disconnect}
                  >
                    <X size={18} />
                  </button>
                )}
              </header>
              {connected ? (
                <>
                  <form
                    className="file-path"
                    onSubmit={(e) => {
                      e.preventDefault();
                      const data = new FormData(e.currentTarget);
                      void perform(() => load(side, String(data.get("path"))));
                    }}
                  >
                    <button
                      type="button"
                      className="icon-button"
                      aria-label={`Parent ${side} directory`}
                      disabled={busy}
                      onClick={() =>
                        void perform(() => load(side, parentPath(listing.path)))
                      }
                    >
                      <ArrowLeft size={17} />
                    </button>
                    <input
                      key={listing.path}
                      name="path"
                      aria-label={`${side} path`}
                      defaultValue={listing.path}
                      disabled={busy}
                    />
                    <button
                      type="button"
                      className="icon-button"
                      aria-label={`Refresh ${side}`}
                      disabled={busy}
                      onClick={() =>
                        void perform(() => load(side, listing.path))
                      }
                    >
                      <ArrowClockwise size={17} />
                    </button>
                  </form>
                  <div className="file-actions">
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() => {
                        setName("");
                        setEdit({ side, action: "mkdir" });
                      }}
                    >
                      <Plus size={15} />
                      Folder
                    </button>
                    <button
                      type="button"
                      disabled={busy || !selected}
                      onClick={() => {
                        setName(selected?.name ?? "");
                        setEdit({ side, action: "rename" });
                      }}
                    >
                      Rename
                    </button>
                    <button
                      type="button"
                      disabled={busy || !selected}
                      onClick={() => setEdit({ side, action: "delete" })}
                    >
                      Delete
                    </button>
                    <button
                      type="button"
                      disabled={
                        busy ||
                        !connection ||
                        connecting ||
                        !selected ||
                        selected.directory
                      }
                      onClick={() => void transfer()}
                    >
                      {side === "local" ? (
                        <ArrowUp size={15} />
                      ) : (
                        <ArrowDown size={15} />
                      )}
                      {side === "local" ? "Upload" : "Download"}
                    </button>
                  </div>
                  <div className="file-table-wrap">
                    <table className="file-table">
                      <thead>
                        <tr>
                          <th>Name</th>
                          <th>Size</th>
                          <th>Modified</th>
                        </tr>
                      </thead>
                      <tbody>
                        {entries.map((entry) => (
                          <tr
                            key={entry.name}
                            className={
                              selected?.name === entry.name ? "selected" : ""
                            }
                          >
                            <td>
                              <button
                                type="button"
                                disabled={busy}
                                onClick={() => setSelection({ side, entry })}
                                onDoubleClick={() => {
                                  if (entry.directory)
                                    void perform(() =>
                                      load(
                                        side,
                                        joinPath(listing.path, entry.name),
                                      ),
                                    );
                                }}
                                onKeyDown={(e) => {
                                  if (e.key === "Enter" && entry.directory)
                                    void perform(() =>
                                      load(
                                        side,
                                        joinPath(listing.path, entry.name),
                                      ),
                                    );
                                }}
                              >
                                {entry.directory ? (
                                  <Folder size={18} weight="duotone" />
                                ) : (
                                  <File size={18} />
                                )}
                                <span>{entry.name}</span>
                              </button>
                            </td>
                            <td>
                              {entry.directory ? "—" : fileSize(entry.size)}
                            </td>
                            <td>
                              {entry.modified
                                ? new Date(
                                    entry.modified * 1000,
                                  ).toLocaleDateString()
                                : "—"}
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                    {entries.length === 0 && (
                      <p className="file-empty">
                        {filter ? "No matching files" : "This folder is empty"}
                      </p>
                    )}
                  </div>
                  <footer className="file-footer">
                    {entries.length} items{" "}
                    <span>
                      {selected?.name ?? "Double-click a folder to open"}
                    </span>
                  </footer>
                </>
              ) : (
                <div className="remote-picker">
                  <div className="remote-symbol">
                    <Plug size={32} />
                  </div>
                  <h2>{connecting ? "Connecting…" : "Connect to a host"}</h2>
                  <p>
                    {connecting
                      ? "Establishing a secure SFTP connection."
                      : "Choose a saved host to browse and transfer files."}
                  </p>
                  {!connecting && (
                    <>
                      <p className="hint">
                        Use an SSH key or agent for authentication.
                      </p>
                      {workspace.hosts.length ? (
                        <div className="sftp-hosts">
                          {workspace.hosts.map((item) => (
                            <button
                              type="button"
                              key={item.id}
                              onClick={() => void connect(item)}
                              disabled={busy}
                            >
                              <HardDrives size={19} />
                              <span>
                                <strong>{item.name}</strong>
                                <small>{item.address}</small>
                              </span>
                              <Plug size={17} />
                            </button>
                          ))}
                        </div>
                      ) : (
                        <p>Add a host in Library to get started.</p>
                      )}
                    </>
                  )}
                </div>
              )}
            </section>
          );
        })}
      </div>
      {(busy || status) && (
        <footer className="transfer-status" role="status" aria-live="polite">
          {busy ? "Working…" : status}
        </footer>
      )}
      {error && (
        <div className="sftp-error" role="alert">
          {error}
          <button
            type="button"
            className="icon-button"
            aria-label="Dismiss file error"
            onClick={() => setError("")}
          >
            <X size={15} />
          </button>
        </div>
      )}
      {request && connection && (
        <HostKeyPrompt
          command="sftp_answer"
          request={request}
          sessionId={connection}
          references={[]}
          onAnswered={() => {
            setRequest(undefined);
            onTrustChanged();
          }}
          onAbort={disconnect}
        />
      )}
      {edit && (
        <Modal
          title={
            edit.action === "mkdir"
              ? "New folder"
              : edit.action === "rename"
                ? "Rename"
                : "Delete item"
          }
          onClose={() => {
            if (!busy) setEdit(undefined);
          }}
        >
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void modify();
            }}
          >
            {edit.action === "delete" ? (
              <p>
                Delete “{selection?.entry.name}”? Only empty folders can be
                deleted. This cannot be undone.
              </p>
            ) : (
              <label>
                Name
                <input
                  aria-label="File name"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                />
              </label>
            )}
            {error && <p role="alert">{error}</p>}
            <div className="modal-actions">
              <button
                type="button"
                disabled={busy}
                onClick={() => setEdit(undefined)}
              >
                Cancel
              </button>
              <button
                type="submit"
                className={edit.action === "delete" ? "danger" : "primary"}
                disabled={
                  busy || (edit.action !== "delete" && !validName(name))
                }
              >
                {edit.action === "delete" ? "Delete" : "Save"}
              </button>
            </div>
          </form>
        </Modal>
      )}
    </section>
  );
}
