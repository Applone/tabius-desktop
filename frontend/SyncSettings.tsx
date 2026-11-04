import {
  ArrowsClockwise,
  CircleNotch,
  CloudArrowDown,
} from "@phosphor-icons/react";
import { useState } from "react";
import { native } from "./api";
import type { Workspace } from "./model";
import type { RemoteConfiguration } from "./sync";
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
  const [encryption, setEncryption] = useState<"modern" | "tabby">(
    workspace.sync ? (workspace.sync.encryption ?? "tabby") : "modern",
  );
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
  function selectConfiguration(id: number | undefined) {
    if (id !== configurationId) {
      setEncryption(
        server.trim().replace(/\/+$/, "") === workspace.sync?.server &&
          id === workspace.sync.configurationId
          ? (workspace.sync.encryption ?? "tabby")
          : "modern",
      );
    }
    setConfigurationId(id);
    setStatus("");
  }
  async function load() {
    setBusy(true);
    setError("");
    setStatus("");
    try {
      const list = await native<RemoteConfiguration[]>("list_configurations", {
        server,
        token,
      });
      setConfigurations(list);
      selectConfiguration(
        list.some((x) => x.id === configurationId)
          ? configurationId
          : list.length === 1
            ? list[0].id
            : undefined,
      );
      if (!list.length)
        setError(
          "No configurations found. Create one on your sync server first.",
        );
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
      const result = await native<{ workspace: Workspace; warnings: string[] }>(
        "synchronize_workspace",
        { workspace, server, token, configurationId, password, encryption },
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
        <h3>Configuration sync</h3>
      </div>
      <fieldset disabled={busy}>
        <Field label="Server URL">
          <input
            type="url"
            value={server}
            placeholder="https://sync.example.com"
            onChange={(e) => {
              setServer(e.target.value);
              setEncryption("modern");
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
              selectConfiguration(
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
          label="Encryption"
          hint={
            encryption === "modern"
              ? "Encrypts the entire configuration. Upgrades existing vaults on sync. Requires a compatible server and clients."
              : "Legacy PBKDF2-SHA512 and AES-256-CBC. For Tabby-Web compatibility only; not recommended."
          }
        >
          <select
            value={encryption}
            onChange={(e) => {
              setEncryption(e.target.value === "tabby" ? "tabby" : "modern");
              setStatus("");
            }}
          >
            <option value="modern">
              Argon2id + XChaCha20-Poly1305 (recommended)
            </option>
            <option value="tabby">Tabby compatibility (legacy)</option>
          </select>
        </Field>
        <Field
          label="Vault password"
          hint="Required for modern encryption and existing encrypted vaults. Not saved."
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
          encrypted vault. Saved passwords stay in the vault; SSH asks for them
          in the terminal. Advanced SSH options, snippets, forwarding rules, and
          terminal settings remain local.
        </p>
        <button
          type="button"
          className="primary"
          disabled={
            !configurationId ||
            !server.trim() ||
            !token.trim() ||
            (encryption === "modern" && !password)
          }
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
          Connecting to sync server…
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
