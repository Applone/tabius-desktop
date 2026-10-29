import {
  Check,
  CircleNotch,
  Desktop,
  LockKey,
  HardDrives as Server,
  ShieldCheck,
  UploadSimple,
  Warning,
} from "@phosphor-icons/react";
import type { FormEvent } from "react";
import { useState } from "react";
import { native } from "./api";
import type {
  Forward,
  HistoryEntry,
  Host,
  KeyEntry,
  Snippet,
  Workspace,
} from "./model";
import {
  historyToSnippets,
  newForward,
  newHost,
  upsert,
  validateHost,
} from "./model";
import { Field, Modal, Toggle } from "./ui";
export type Editor =
  | { type: "host"; value: Host }
  | { type: "forward"; value: Forward }
  | { type: "snippet"; value: Snippet }
  | { type: "key" }
  | { type: "history" }
  | { type: "known" };
export function EditorModal({
  editor,
  workspace,
  onClose,
  onSave,
  onKnown,
}: {
  editor: Editor;
  workspace: Workspace;
  onClose: () => void;
  onSave: (next: Workspace) => void;
  onKnown: () => void;
}) {
  const [host, setHost] = useState(
    editor.type === "host" ? editor.value : newHost(),
  );
  const [rule, setRule] = useState(
    editor.type === "forward" ? editor.value : newForward(),
  );
  const [snippet, setSnippet] = useState(
    editor.type === "snippet"
      ? editor.value
      : { id: "", name: "", command: "" },
  );
  const [keyMode, setKeyMode] = useState("generate");
  const [publicKey, setPublicKey] = useState("");
  const [keyName, setKeyName] = useState("");
  const [kind, setKind] = useState("ed25519");
  const [passphrase, setPassphrase] = useState("");
  const [text, setText] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const titles = {
    host: "Host details",
    forward: "Port forwarding",
    snippet: "Snippet",
    key: "Add a key",
    history: "Import shell history",
    known: "Import known hosts",
  };
  const setH = <K extends keyof Host>(key: K, value: Host[K]) =>
    setHost({
      ...host,
      [key]: value,
      ...(key === "keyId"
        ? { syncAuth: value ? "publicKey" : "password", syncKeyRef: null }
        : {}),
    });
  const setF = <K extends keyof Forward>(key: K, value: Forward[K]) =>
    setRule({ ...rule, [key]: value });
  async function submit(e: FormEvent) {
    e.preventDefault();
    setError("");
    setBusy(true);
    try {
      if (editor.type === "host") {
        const problem = validateHost(host);
        if (problem) throw new Error(problem);
        onSave({ ...workspace, hosts: upsert(workspace.hosts, host) });
      }
      if (editor.type === "forward") {
        if (!rule.name.trim() || !rule.hostId)
          throw new Error("Enter a name and choose a host.");
        if (
          ![
            rule.bindPort,
            ...(rule.kind === "dynamic" ? [] : [rule.destinationPort]),
          ].every((p) => Number.isInteger(p) && p >= 1 && p <= 65535)
        )
          throw new Error("Ports must be between 1 and 65535.");
        onSave({ ...workspace, forwards: upsert(workspace.forwards, rule) });
      }
      if (editor.type === "snippet") {
        if (!snippet.name.trim() || !snippet.command.trim())
          throw new Error("Enter a name and script.");
        onSave({ ...workspace, snippets: upsert(workspace.snippets, snippet) });
      }
      if (editor.type === "key") {
        if (!keyName.trim()) throw new Error("Enter a key name.");
        const key = await native<KeyEntry>(
          keyMode === "generate" ? "generate_key" : "import_key",
          { name: keyName, kind, passphrase, publicKey, content: text },
        );
        onSave({ ...workspace, keys: [...workspace.keys, key] });
      }
      if (editor.type === "history") {
        const commands = historyToSnippets(text);
        if (!commands.length) throw new Error("No commands found.");
        const history: HistoryEntry[] = commands.map((command) => ({
          id: crypto.randomUUID(),
          host: "Imported history",
          command,
          date: new Date().toISOString(),
        }));
        onSave({
          ...workspace,
          history: [...history, ...workspace.history].slice(0, 1000),
        });
      }
      if (editor.type === "known") {
        await native("import_known_hosts", { content: text });
        onKnown();
        onClose();
      }
    } catch (err) {
      setError(String(err).replace(/^Error: /, ""));
    } finally {
      setBusy(false);
    }
  }
  async function readFile(file: File | undefined) {
    if (!file) return;
    if (file.size > 1024 * 1024) {
      setError("Choose a file smaller than 1 MB.");
      return;
    }
    try {
      setText(await file.text());
    } catch (err) {
      setError(String(err));
    }
  }
  return (
    <Modal
      title={titles[editor.type]}
      onClose={onClose}
      wide={editor.type === "host"}
    >
      <form
        onSubmit={(e) => {
          void submit(e);
        }}
      >
        <fieldset disabled={busy}>
          {editor.type === "host" && (
            <>
              <div className="form-section">
                <h3>Connection</h3>
                <Field label="Label">
                  <input
                    required
                    value={host.name}
                    onChange={(e) => setH("name", e.target.value)}
                    placeholder="e.g. Production server"
                  />
                </Field>
                <Field label="Hostname or IP address">
                  <input
                    required
                    value={host.address}
                    onChange={(e) => setH("address", e.target.value)}
                    placeholder="192.168.1.10"
                  />
                </Field>
                <div className="form-columns">
                  <Field label="Username">
                    <input
                      value={host.username}
                      onChange={(e) => setH("username", e.target.value)}
                      placeholder="System default"
                    />
                  </Field>
                  <Field label="SSH port">
                    <input
                      type="number"
                      min="1"
                      max="65535"
                      required
                      value={host.port}
                      onChange={(e) => setH("port", Number(e.target.value))}
                    />
                  </Field>
                </div>
                <Field
                  label="Authentication"
                  hint="Password and key passphrase prompts appear securely inside the terminal."
                >
                  <select
                    value={host.keyId}
                    onChange={(e) => setH("keyId", e.target.value)}
                  >
                    <option value="">SSH agent / interactive password</option>
                    {workspace.keys.map((k) => (
                      <option key={k.id} value={k.id}>
                        {k.name}
                      </option>
                    ))}
                  </select>
                </Field>
              </div>
              <div className="form-section">
                <h3>Session options</h3>
                <Toggle
                  label="Agent forwarding"
                  description="Allow the remote host to use your local SSH agent."
                  value={host.agentForwarding}
                  onChange={(v) => setH("agentForwarding", v)}
                />
                <Field label="Startup snippet">
                  <select
                    value={host.startup}
                    onChange={(e) => setH("startup", e.target.value)}
                  >
                    <option value="">None</option>
                    {workspace.snippets.map((s) => (
                      <option value={s.id} key={s.id}>
                        {s.name}
                      </option>
                    ))}
                  </select>
                </Field>
                <Field
                  label="Host chaining"
                  hint="Comma-separated SSH jump hosts, e.g. user@bastion:22."
                >
                  <input
                    value={host.jump}
                    onChange={(e) => setH("jump", e.target.value)}
                    placeholder="user@bastion.example.com:22"
                  />
                </Field>
                <Field
                  label="SSH proxy"
                  hint="An additional SSH proxy hop, placed before the host chain."
                >
                  <input
                    value={host.proxy}
                    onChange={(e) => setH("proxy", e.target.value)}
                    placeholder="user@proxy.example.com:22"
                  />
                </Field>
                <Field
                  label="Environment variables"
                  hint="One NAME=value per line. The server must allow these through AcceptEnv."
                >
                  <textarea
                    rows={3}
                    value={host.env}
                    onChange={(e) => setH("env", e.target.value)}
                    placeholder="LANG=en_US.UTF-8"
                  />
                </Field>
                <Toggle
                  label="UTF-8"
                  description="Send a UTF-8 locale to the remote session."
                  value={host.utf8}
                  onChange={(v) => setH("utf8", v)}
                />
                <Toggle
                  label="Mosh"
                  description="Requires mosh locally and mosh-server on the host. SSH forwarding uses OpenSSH."
                  value={host.mosh}
                  onChange={(v) => setH("mosh", v)}
                />
              </div>
            </>
          )}
          {editor.type === "forward" && (
            <>
              <div className="segmented">
                {(["local", "remote", "dynamic"] as const).map((value) => (
                  <button
                    type="button"
                    className={rule.kind === value ? "chosen" : ""}
                    onClick={() => setF("kind", value)}
                    key={value}
                  >
                    {value[0].toUpperCase() + value.slice(1)}
                  </button>
                ))}
              </div>
              <div className="forward-diagram">
                <Desktop size={30} />
                <span>········</span>
                <ShieldCheck size={33} />
                <span>········</span>
                <Server size={30} />
              </div>
              <p className="helper">
                {rule.kind === "local"
                  ? "Access a remote service from a port on your computer."
                  : rule.kind === "remote"
                    ? "Expose a local service through a listening port on your SSH server."
                    : "Create a local SOCKS5 proxy through an SSH host."}
              </p>
              <Field label="Rule name">
                <input
                  required
                  value={rule.name}
                  onChange={(e) => setF("name", e.target.value)}
                  placeholder="e.g. Database tunnel"
                />
              </Field>
              <Field label="SSH host">
                <select
                  required
                  value={rule.hostId}
                  onChange={(e) => setF("hostId", e.target.value)}
                >
                  <option value="">Select a host</option>
                  {workspace.hosts.map((h) => (
                    <option key={h.id} value={h.id}>
                      {h.name}
                    </option>
                  ))}
                </select>
              </Field>
              <div className="form-columns">
                <Field label="Bind address">
                  <select
                    value={rule.bindAddress}
                    onChange={(e) => setF("bindAddress", e.target.value)}
                  >
                    <option>127.0.0.1</option>
                    <option>::1</option>
                  </select>
                </Field>
                <Field label="Listening port">
                  <input
                    required
                    type="number"
                    min="1"
                    max="65535"
                    value={rule.bindPort}
                    onChange={(e) => setF("bindPort", Number(e.target.value))}
                  />
                </Field>
              </div>
              {rule.kind !== "dynamic" && (
                <div className="form-columns">
                  <Field label="Destination host">
                    <input
                      required
                      value={rule.destination}
                      onChange={(e) => setF("destination", e.target.value)}
                    />
                  </Field>
                  <Field label="Destination port">
                    <input
                      required
                      type="number"
                      min="1"
                      max="65535"
                      value={rule.destinationPort}
                      onChange={(e) =>
                        setF("destinationPort", Number(e.target.value))
                      }
                    />
                  </Field>
                </div>
              )}
              <p className="helper">
                Listeners are restricted to loopback. Remote forwarding also
                depends on the server’s GatewayPorts policy. Close the tunnel
                tab to stop it.
              </p>
            </>
          )}
          {editor.type === "snippet" && (
            <>
              <Field label="Label">
                <input
                  required
                  value={snippet.name}
                  onChange={(e) =>
                    setSnippet({ ...snippet, name: e.target.value })
                  }
                  placeholder="e.g. Check disk usage"
                />
              </Field>
              <Field label="Script">
                <textarea
                  required
                  className="code-input"
                  rows={9}
                  value={snippet.command}
                  onChange={(e) =>
                    setSnippet({ ...snippet, command: e.target.value })
                  }
                  placeholder="df -h"
                />
              </Field>
              <p className="helper">
                Insert snippets into an open terminal, or use one as a host’s
                startup snippet. Startup snippets execute after authentication.
              </p>
            </>
          )}
          {editor.type === "key" && (
            <>
              <div className="segmented">
                <button
                  type="button"
                  className={keyMode === "generate" ? "chosen" : ""}
                  onClick={() => setKeyMode("generate")}
                >
                  Generate key
                </button>
                <button
                  type="button"
                  className={keyMode === "import" ? "chosen" : ""}
                  onClick={() => setKeyMode("import")}
                >
                  Import key
                </button>
              </div>
              <Field label="Key label">
                <input
                  required
                  value={keyName}
                  onChange={(e) => setKeyName(e.target.value)}
                  placeholder="e.g. Personal laptop"
                />
              </Field>
              {keyMode === "generate" ? (
                <>
                  <Field label="Algorithm">
                    <select
                      value={kind}
                      onChange={(e) => setKind(e.target.value)}
                    >
                      <option value="ed25519">Ed25519 · Recommended</option>
                      <option value="rsa">RSA · 4096 bits</option>
                    </select>
                  </Field>
                  <Field
                    label="Passphrase"
                    hint="Optional. A passphrase encrypts your private key at rest."
                  >
                    <input
                      type="password"
                      autoComplete="new-password"
                      value={passphrase}
                      onChange={(e) => setPassphrase(e.target.value)}
                    />
                  </Field>
                </>
              ) : (
                <>
                  <Field label="Private key">
                    <textarea
                      required
                      className="code-input"
                      rows={7}
                      value={text}
                      onChange={(e) => setText(e.target.value)}
                      placeholder="-----BEGIN OPENSSH PRIVATE KEY-----"
                    />
                  </Field>
                  <label className="file-input">
                    <UploadSimple size={17} />
                    Choose private key file
                    <input
                      type="file"
                      onChange={(e) => {
                        void readFile(e.target.files?.[0]);
                      }}
                    />
                  </label>
                  <p className="helper">
                    For encrypted keys, include the matching .pub key below to
                    avoid needing to decrypt it during import.
                  </p>
                  <Field label="Public key (optional)">
                    <input
                      value={publicKey}
                      onChange={(e) => setPublicKey(e.target.value)}
                      placeholder="ssh-ed25519 AAAA…"
                    />
                  </Field>
                </>
              )}
              <div className="security-note">
                <LockKey size={19} />
                <p>
                  Private key files are saved in Tabius’s application data
                  directory with owner-only permissions on Linux and macOS. They
                  are never saved in browser storage.
                </p>
              </div>
            </>
          )}
          {(editor.type === "history" || editor.type === "known") && (
            <>
              <p className="helper">
                {editor.type === "history"
                  ? "Paste Bash or Zsh history, or choose a history file. Review the contents before importing: shell history can contain secrets."
                  : "Paste entries from an OpenSSH known_hosts file. Only import identities you have verified independently."}
              </p>
              <Field
                label={
                  editor.type === "history"
                    ? "Shell history"
                    : "Known hosts entries"
                }
              >
                <textarea
                  className="code-input"
                  required
                  rows={10}
                  value={text}
                  onChange={(e) => setText(e.target.value)}
                />
              </Field>
              <label className="file-input">
                <UploadSimple size={17} />
                Choose file
                <input
                  type="file"
                  onChange={(e) => {
                    void readFile(e.target.files?.[0]);
                  }}
                />
              </label>
            </>
          )}
          {error && (
            <div className="inline-error" role="alert">
              <Warning size={17} />
              {error}
            </div>
          )}
          <div className="modal-actions">
            <button type="button" onClick={onClose}>
              Cancel
            </button>
            <button className="primary" type="submit">
              {busy ? (
                <CircleNotch className="spin" size={17} />
              ) : (
                <Check size={17} />
              )}{" "}
              {busy
                ? "Working…"
                : editor.type === "key"
                  ? keyMode === "generate"
                    ? "Generate key"
                    : "Import key"
                  : editor.type === "history" || editor.type === "known"
                    ? "Import"
                    : "Save changes"}
            </button>
          </div>
        </fieldset>
      </form>
    </Modal>
  );
}
