import { CircleNotch, UploadSimple, Warning } from "@phosphor-icons/react";
import { useState } from "react";
import { native } from "./api";
import type { Host, Workspace } from "./model";
import { Field, Modal } from "./ui";

interface Preview {
  hosts: Host[];
  keyCount: number;
  skipped: number;
  warnings: string[];
  fingerprint: string;
}

export function SSHConfigImport({
  workspace,
  onClose,
  onSave,
  onPrepare,
}: {
  workspace: Workspace;
  onClose: () => void;
  onSave: (workspace: Workspace, message: string) => void;
  onPrepare: () => Promise<void>;
}) {
  const [path, setPath] = useState("~/.ssh/config");
  const [loaded, setLoaded] = useState<{
    workspace: Workspace;
    preview: Preview;
  } | null>(null);
  const preview = loaded?.workspace === workspace ? loaded.preview : null;
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  async function submit() {
    setBusy(true);
    setError("");
    try {
      if (!preview) {
        const preview = await native<Preview>("preview_ssh_config", {
          workspace,
          path,
        });
        setLoaded({ workspace, preview });
      } else {
        await onPrepare();
        const result = await native<{
          workspace: Workspace;
          imported: number;
          keyCount: number;
        }>("import_ssh_config", {
          workspace,
          path,
          fingerprint: preview.fingerprint,
        });
        onSave(
          result.workspace,
          `Imported ${result.imported} hosts and ${result.keyCount} keys.`,
        );
      }
    } catch (err) {
      setLoaded(null);
      setError(String(err).replace(/^Error: /, ""));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal title="Import SSH config" onClose={onClose}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void submit();
        }}
      >
        <fieldset disabled={busy}>
          <Field label="Configuration file" hint="Defaults to ~/.ssh/config.">
            <input
              required
              value={path}
              onChange={(event) => {
                setPath(event.target.value);
                setLoaded(null);
                setError("");
              }}
            />
          </Field>
          <p className="helper">
            Import named hosts and migrate their private keys into Keychain.
            Encrypted keys keep their passphrases. Original files stay in place.
            Existing hosts are preserved; matching keys are reused.
          </p>
          {preview && (
            <div className="form-section" aria-live="polite">
              <p>
                {preview.hosts.length} hosts and {preview.keyCount} new keys
                ready to import. {preview.skipped} hosts skipped.
              </p>
              {preview.hosts.length > 0 && (
                <ul>
                  {preview.hosts.map((host) => (
                    <li key={host.id}>
                      <strong>{host.name}</strong> · {host.address}:{host.port}
                    </li>
                  ))}
                </ul>
              )}
              {preview.warnings.length > 0 && (
                <>
                  <h3>Review before importing</h3>
                  <ul>
                    {preview.warnings.map((warning) => (
                      <li key={warning}>{warning}</li>
                    ))}
                  </ul>
                </>
              )}
              {preview.hosts.length === 0 && (
                <p className="helper">
                  No new named hosts were found. Wildcard entries provide
                  defaults and are not imported as individual hosts.
                </p>
              )}
            </div>
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
            <button
              type="submit"
              className="primary"
              disabled={preview?.hosts.length === 0}
            >
              {busy ? (
                <CircleNotch className="spin" size={17} />
              ) : (
                <UploadSimple size={17} />
              )}
              {busy ? "Working…" : preview ? "Import" : "Read configuration"}
            </button>
          </div>
        </fieldset>
      </form>
    </Modal>
  );
}
