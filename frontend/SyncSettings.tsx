import {
  ArrowsClockwise,
  CircleNotch,
  CloudArrowDown,
} from "@phosphor-icons/react";
import { useState } from "react";
import { native } from "./api";
import type { KeyEntry, KnownHost, Workspace } from "./model";
import type { RemoteConfiguration, SecretKey, SyncResponse } from "./sync";
import { synchronize } from "./sync";
import { Field } from "./ui";
export function SyncSettings({
  workspace,
  onApply,
  onBusy,
  isCurrent,
}: {
  workspace: Workspace;
  onApply: (workspace: Workspace) => Promise<void>;
  onBusy: (busy: boolean) => void;
  isCurrent: () => boolean;
}) {
  const [server, setServer] = useState(workspace.sync?.server ?? "");
  const [token, setToken] = useState("");
  const [password, setPassword] = useState("");
  const [configurationId, setConfigurationId] = useState<number | undefined>(
    workspace.sync?.configurationId,
  );
  const [configurations, setConfigurations] = useState<RemoteConfiguration[]>(
    [],
  );
  const [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [status, setStatus] = useState("");
  const [warnings, setWarnings] = useState<string[]>([]);
  function request(id?: number, content?: string, etag?: string | null) {
    return native<SyncResponse>("tabby_request", {
      server: server.trim().replace(/\/+$/, ""),
      token: token.trim(),
      configurationId: id ?? null,
      content: content ?? null,
      etag: etag ?? null,
    });
  }
  async function load() {
    setBusy(true);
    setError("");
    setStatus("");
    try {
      const response = await request();
      if (!Array.isArray(response.body))
        throw new Error("Invalid Tabby-Web configurations response.");
      const list: RemoteConfiguration[] = response.body.map(
        (entry: unknown) => {
          if (
            !entry ||
            typeof entry !== "object" ||
            !("id" in entry) ||
            !Number.isSafeInteger(entry.id) ||
            Number(entry.id) <= 0 ||
            !("name" in entry) ||
            typeof entry.name !== "string"
          )
            throw new Error("Invalid Tabby-Web configuration.");
          return { id: Number(entry.id), name: entry.name };
        },
      );
      setConfigurations(list);
      setConfigurationId(
        list.some((x) => x.id === configurationId)
          ? configurationId
          : list.length === 1
            ? list[0].id
            : undefined,
      );
      if (!list.length)
        setError("No configurations found. Create one in Tabby-Web first.");
    } catch (error) {
      setError(String(error).replace(/^Error: /, ""));
    } finally {
      setBusy(false);
    }
  }
  async function sync() {
    if (!configurationId) return;
    setBusy(true);
    onBusy(true);
    setError("");
    setStatus("");
    setWarnings([]);
    try {
      const result = await synchronize(
        workspace,
        server.trim().replace(/\/+$/, ""),
        configurationId,
        password,
        {
          request,
          readKeys: (keys) =>
            native("read_sync_keys", { ids: keys.map((k) => k.id) }),
          importKey: (key: SecretKey) =>
            native<KeyEntry>("import_sync_key", {
              name: key.name,
              secret: key.secret,
              publicKey: key.publicKey,
              passphrase: key.passphrase ?? "",
            }),
          deleteKey: (id) => native("delete_key", { id }),
          knownHosts: () => native<KnownHost[]>("known_hosts"),
          ensureUnchanged: () => {
            if (!isCurrent())
              throw new Error(
                "The local workspace changed during sync. Try again.",
              );
          },
        },
      );
      if (!isCurrent())
        throw new Error(
          "The local workspace changed after upload. Sync again to apply the remote result.",
        );
      await onApply(result.workspace);
      setWarnings(result.warnings);
      setStatus("Synchronization complete.");
    } catch (error) {
      setError(String(error).replace(/^Error: /, ""));
    } finally {
      setPassword("");
      setBusy(false);
      onBusy(false);
    }
  }
  return (
    <section className="settings-section sync-section">
      <div className="settings-section-heading">
        <ArrowsClockwise size={20} />
        <h3>Tabby-Web sync</h3>
      </div>
      <fieldset disabled={busy}>
        <Field label="Server URL">
          <input
            type="url"
            value={server}
            placeholder="https://tabby.example.com"
            onChange={(e) => {
              setServer(e.target.value);
              setConfigurations([]);
              setConfigurationId(undefined);
              setStatus("");
            }}
          />
        </Field>
        <Field label="Synchronization token">
          <input
            type="password"
            autoComplete="off"
            value={token}
            onChange={(e) => {
              setToken(e.target.value);
              setConfigurations([]);
              setStatus("");
            }}
          />
        </Field>
        <button
          type="button"
          disabled={!server.trim() || !token.trim()}
          onClick={() => {
            void load();
          }}
        >
          <CloudArrowDown size={17} />
          Load configurations
        </button>
        <Field label="Configuration">
          <select
            value={configurationId ?? ""}
            onChange={(e) => {
              setConfigurationId(
                e.target.value ? Number(e.target.value) : undefined,
              );
              setStatus("");
            }}
          >
            <option value="">Select configuration</option>
            {configurationId &&
              !configurations.some((c) => c.id === configurationId) && (
                <option value={configurationId}>
                  Configuration {configurationId}
                </option>
              )}
            {configurations.map((config) => (
              <option value={config.id} key={config.id}>
                {config.name}
              </option>
            ))}
          </select>
        </Field>
        <Field
          label="Tabby vault password"
          hint="Required for encrypted vaults. Not saved."
        >
          <input
            type="password"
            autoComplete="off"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
        </Field>
        <p className="helper">
          Sync hosts, private keys, and fingerprint references. Keys require an
          encrypted Tabby vault. Saved passwords stay in Tabby; SSH asks for
          them in the terminal. Advanced SSH options, snippets, forwarding
          rules, and terminal settings remain local.
        </p>
        <button
          type="button"
          className="primary"
          disabled={!configurationId || !server.trim() || !token.trim()}
          onClick={() => {
            void sync();
          }}
        >
          {busy ? (
            <CircleNotch className="spin" size={17} />
          ) : (
            <ArrowsClockwise size={17} />
          )}
          Synchronize
        </button>
      </fieldset>
      {busy && (
        <p role="status" className="sync-status">
          Connecting to Tabby-Web…
        </p>
      )}
      {error && (
        <p className="inline-error" role="alert">
          {error}
        </p>
      )}
      {status && (
        <p className="sync-status" role="status">
          {status}
        </p>
      )}
      {workspace.sync?.lastSyncedAt && (
        <p className="helper sync-date">
          Last sync: {new Date(workspace.sync.lastSyncedAt).toLocaleString()}
        </p>
      )}
      {warnings.length > 0 && (
        <ul className="sync-warnings">
          {warnings.map((warning) => (
            <li key={warning}>{warning}</li>
          ))}
        </ul>
      )}
    </section>
  );
}
