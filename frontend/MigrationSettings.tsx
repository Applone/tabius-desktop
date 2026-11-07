import { CircleNotch, CloudArrowUp } from "@phosphor-icons/react";
import { useState } from "react";
import { native } from "./api";
import type { RemoteConfiguration } from "./sync";
import { Field } from "./ui";

interface Connection {
  server: string;
  token: string;
  configurationId?: number;
  configurations: RemoteConfiguration[];
}

interface Preview {
  fingerprint: string;
  profiles: number;
  secrets: number;
  fingerprints: number;
  destinationHasContent: boolean;
}

function ServerFields({
  label,
  connection,
  onChange,
  onLoad,
}: {
  label: string;
  connection: Connection;
  onChange: (connection: Connection) => void;
  onLoad: () => void;
}) {
  return (
    <div className="form-section">
      <h4>{label}</h4>
      <Field label={`${label} URL`}>
        <input
          type="url"
          placeholder="https://sync.example.com"
          value={connection.server}
          onChange={(e) =>
            onChange({
              ...connection,
              server: e.target.value,
              configurations: [],
              configurationId: undefined,
            })
          }
        />
      </Field>
      <Field label={`${label} synchronization token`}>
        <input
          type="password"
          autoComplete="off"
          value={connection.token}
          onChange={(e) =>
            onChange({
              ...connection,
              token: e.target.value,
              configurations: [],
              configurationId: undefined,
            })
          }
        />
      </Field>
      <button
        type="button"
        disabled={!connection.server.trim() || !connection.token.trim()}
        onClick={onLoad}
      >
        Load {label.toLowerCase()} configurations
      </button>
      <Field label={`${label} configuration`}>
        <select
          value={connection.configurationId ?? ""}
          onChange={(e) =>
            onChange({
              ...connection,
              configurationId: e.target.value
                ? Number(e.target.value)
                : undefined,
            })
          }
        >
          <option value="">Select configuration</option>
          {connection.configurations.map((config) => (
            <option value={config.id} key={config.id}>
              {config.name}
            </option>
          ))}
        </select>
      </Field>
    </div>
  );
}

export function MigrationSettings({
  disabled,
  onBusy,
}: {
  disabled: boolean;
  onBusy: (busy: boolean) => void;
}) {
  const [source, setSource] = useState<Connection>({
    server: "",
    token: "",
    configurations: [],
  });
  const [destination, setDestination] = useState<Connection>({
    server: "",
    token: "",
    configurations: [],
  });
  const [sourcePassword, setSourcePassword] = useState("");
  const [password, setPassword] = useState("");
  const [preview, setPreview] = useState<Preview | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [status, setStatus] = useState("");

  function reset() {
    setPreview(null);
    setError("");
    setStatus("");
  }

  async function run(action: () => Promise<void>) {
    setBusy(true);
    onBusy(true);
    setError("");
    setStatus("");
    try {
      await action();
    } catch (err) {
      setPreview(null);
      setError(String(err).replace(/^Error: /, ""));
    } finally {
      setBusy(false);
      onBusy(false);
    }
  }

  async function load(which: "source" | "destination") {
    const connection = which === "source" ? source : destination;
    const configurations = await native<RemoteConfiguration[]>(
      "list_configurations",
      { server: connection.server, token: connection.token },
    );
    const update = which === "source" ? setSource : setDestination;
    update({
      ...connection,
      configurations,
      configurationId:
        configurations.length === 1 ? configurations[0].id : undefined,
    });
    setPreview(null);
    if (!configurations.length)
      setStatus(
        `No ${which} configurations found. Create one on that server first.`,
      );
  }

  async function submit() {
    const result = await native<Preview | { migrated: true }>(
      "migrate_configuration",
      {
        source: {
          server: source.server,
          token: source.token,
          configurationId: source.configurationId,
        },
        destination: {
          server: destination.server,
          token: destination.token,
          configurationId: destination.configurationId,
        },
        sourcePassword,
        password,
        fingerprint: preview?.fingerprint ?? null,
      },
    );
    if ("migrated" in result) {
      setPreview(null);
      setSourcePassword("");
      setPassword("");
      setSource({ ...source, token: "" });
      setDestination({ ...destination, token: "" });
      setStatus(
        "Migration complete. The encrypted copy was verified on the Tabius sync server. Connect Configuration sync to that server using the new vault password.",
      );
    } else {
      setPreview(result);
    }
  }

  return (
    <section className="settings-section sync-section migration-section">
      <div className="settings-section-heading">
        <CloudArrowUp size={20} />
        <h3>Migrate from Tabby-Web</h3>
      </div>
      <p className="helper">
        Transfer your configuration from Tabby-Web to a Tabius sync server. Your
        old vault is decrypted on this device, then the complete copy is
        encrypted with Argon2id and XChaCha20-Poly1305 before upload. Profiles,
        keys, saved secrets, and fingerprints are preserved. Tabby-Web stays
        unchanged. Create a configuration on the Tabius server before starting.
      </p>
      <fieldset disabled={busy || disabled}>
        <ServerFields
          label="Tabby-Web server"
          connection={source}
          onChange={(value) => {
            setSource(value);
            reset();
          }}
          onLoad={() => void run(() => load("source"))}
        />
        <Field
          label="Old Tabby-Web vault password"
          hint="Leave empty if the legacy configuration is unencrypted."
        >
          <input
            type="password"
            autoComplete="off"
            value={sourcePassword}
            onChange={(e) => {
              setSourcePassword(e.target.value);
              reset();
            }}
          />
        </Field>
        <ServerFields
          label="Tabius sync server"
          connection={destination}
          onChange={(value) => {
            setDestination(value);
            reset();
          }}
          onLoad={() => void run(() => load("destination"))}
        />
        <Field
          label="New Tabius vault password"
          hint="Required. Encrypts the full copy; tokens and passwords are not saved."
        >
          <input
            type="password"
            autoComplete="new-password"
            value={password}
            onChange={(e) => {
              setPassword(e.target.value);
              reset();
            }}
          />
        </Field>
        {preview && (
          <div className="form-section helper" role="status">
            <p className="helper">
              Ready to copy {preview.profiles} profiles, {preview.secrets} vault
              secrets, and {preview.fingerprints} host fingerprints.
            </p>
            <p>
              {preview.destinationHasContent
                ? "This replaces all data in the selected destination configuration."
                : "This writes to the selected empty destination configuration."}{" "}
              Clients using the destination must support Tabius version 2
              encryption. Legacy Tabby clients cannot read the new copy.
            </p>
          </div>
        )}
        <button
          type="button"
          className="primary"
          disabled={
            !source.configurationId ||
            !destination.configurationId ||
            !source.token.trim() ||
            !destination.token.trim() ||
            !password
          }
          onClick={() => void run(submit)}
        >
          {busy && <CircleNotch className="spin" size={17} />}
          {preview
            ? preview.destinationHasContent
              ? "Migrate and replace destination"
              : "Migrate to Tabius"
            : "Preview migration"}
        </button>
      </fieldset>
      {busy && <p role="status">Contacting sync servers…</p>}
      {error && (
        <p className="inline-error" role="alert">
          {error}
        </p>
      )}
      {status && <p role="status">{status}</p>}
    </section>
  );
}
