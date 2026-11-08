import { useState } from "react";
import { native } from "./api";
import { Modal } from "./ui";

export interface HostKeyRequest {
  id: string;
  host: string;
  kind: string;
  fingerprint: string;
}
export function HostKeyPrompt({
  request,
  sessionId,
  references,
  onAnswered,
  onAbort,
}: {
  request: HostKeyRequest;
  sessionId: string;
  references: string[];
  onAnswered: () => void;
  onAbort: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  async function answer(decision: "abort" | "once" | "save") {
    if (busy) return;
    setBusy(true);
    setError("");
    try {
      await native("answer_host_key", {
        id: sessionId,
        requestId: request.id,
        decision,
      });
      if (decision === "abort") onAbort();
      else onAnswered();
    } catch (err) {
      if (decision === "abort") onAbort();
      else {
        setError(String(err));
        setBusy(false);
      }
    }
  }
  return (
    <Modal
      title="Verify host fingerprint"
      onClose={() => {
        void answer("abort");
      }}
    >
      <p>
        This host is not in Known Hosts. Compare its fingerprint with one
        provided by the server administrator before continuing.
      </p>
      <dl className="host-key-details">
        <dt>Host</dt>
        <dd>{request.host}</dd>
        <dt>Key type</dt>
        <dd>{request.kind}</dd>
        <dt>Fingerprint</dt>
        <dd className="mono">{request.fingerprint}</dd>
      </dl>
      {references.length > 0 && (
        <div className="form-section">
          <h3>Synced references for this session</h3>
          {references.map((reference) => (
            <p className="mono" key={reference}>
              {reference}
            </p>
          ))}
        </div>
      )}
      <p className="hint">
        Continue once leaves Known Hosts unchanged. Trust and continue saves
        this key for future connections.
      </p>
      {error && (
        <p className="inline-error" role="alert">
          {error}
        </p>
      )}
      <div className="host-key-actions">
        <button
          type="button"
          disabled={busy}
          onClick={() => {
            void answer("abort");
          }}
        >
          Abort login
        </button>
        <button
          type="button"
          disabled={busy}
          onClick={() => {
            void answer("once");
          }}
        >
          Continue once
        </button>
        <button
          type="button"
          className="primary"
          disabled={busy}
          onClick={() => {
            void answer("save");
          }}
        >
          Trust and continue
        </button>
      </div>
    </Modal>
  );
}
