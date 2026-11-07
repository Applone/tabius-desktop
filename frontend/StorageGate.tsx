import { CircleNotch, LockKey, Warning } from "@phosphor-icons/react";
import type { FormEvent, ReactNode } from "react";
import { useCallback, useEffect, useState } from "react";
import { native } from "./api";
import { Field } from "./ui";

export interface StorageStatus {
  state: "setup" | "locked" | "ready";
  keyringUnavailable: boolean;
}

export function StorageGate({ children }: { children: ReactNode }) {
  const [status, setStatus] = useState<StorageStatus>();
  const [password, setPassword] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const check = useCallback(async () => {
    setBusy(true);
    setError("");
    try {
      setStatus(await native<StorageStatus>("storage_status"));
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }, []);
  useEffect(() => {
    void check();
  }, [check]);

  async function configure(mode: "system" | "manual" | "plain") {
    if (busy) return;
    if (mode !== "plain" && password !== confirmation) {
      setError("The passwords do not match.");
      return;
    }
    setBusy(true);
    setError("");
    try {
      const next = await native<StorageStatus>("configure_storage", {
        mode,
        password: mode === "plain" ? "" : password,
      });
      setStatus(next);
      if (next.state === "ready") {
        setPassword("");
        setConfirmation("");
      }
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (busy) return;
    if (status?.state !== "locked") {
      await configure(status?.keyringUnavailable ? "manual" : "system");
      return;
    }
    setBusy(true);
    setError("");
    try {
      await native("unlock_storage", { password });
      setPassword("");
      setStatus({ state: "ready", keyringUnavailable: false });
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  if (status?.state === "ready") return children;
  return (
    <main className="storage-gate">
      <div className="settings-section storage-panel">
        <LockKey size={28} className="accent" />
        <h1>
          {!status
            ? error
              ? "Local storage unavailable"
              : "Opening Tabius"
            : status.state === "locked"
              ? "Unlock Tabius"
              : "Protect your local storage"}
        </h1>
        {!status ? (
          <>
            <p>Checking local storage…</p>
            {busy && <CircleNotch className="spin" size={22} />}
          </>
        ) : (
          <form onSubmit={(event) => void submit(event)}>
            <fieldset disabled={busy}>
              <p>
                {status.state === "locked"
                  ? "Enter your local storage password to open your workspace."
                  : "Choose a password to encrypt your hosts, keys, snippets, history, and settings. Existing local data will be encrypted too."}
              </p>
              {status.keyringUnavailable && (
                <p className="storage-note">
                  {status.state === "setup"
                    ? "System secret storage is unavailable. Choose whether to enter your password on every launch or continue without encryption."
                    : "The saved password could not be retrieved from system secret storage. You can still unlock with your original password."}
                </p>
              )}
              <Field label="Local storage password">
                <input
                  type="password"
                  required
                  autoComplete={
                    status.state === "locked"
                      ? "current-password"
                      : "new-password"
                  }
                  value={password}
                  onChange={(event) => setPassword(event.target.value)}
                />
              </Field>
              {status.state === "setup" && (
                <>
                  <Field label="Confirm password">
                    <input
                      type="password"
                      required
                      autoComplete="new-password"
                      value={confirmation}
                      onChange={(event) => setConfirmation(event.target.value)}
                    />
                  </Field>
                  <p className="storage-note">
                    {status.keyringUnavailable
                      ? "Keep this password safe. It cannot be recovered if you forget it."
                      : "Your password will be saved in your system's secret storage so Tabius can unlock automatically. Keep a copy in case that entry becomes unavailable."}
                  </p>
                </>
              )}
              <div className="storage-actions">
                {status.state === "setup" && status.keyringUnavailable ? (
                  <>
                    <button
                      type="button"
                      className="primary"
                      disabled={!password || password !== confirmation}
                      onClick={() => void configure("manual")}
                    >
                      Ask for password on every launch
                    </button>
                    <button
                      type="button"
                      onClick={() => void configure("plain")}
                    >
                      Continue without encryption
                    </button>
                  </>
                ) : (
                  <button type="submit" className="primary">
                    {busy && <CircleNotch className="spin" size={16} />}
                    {status.state === "locked"
                      ? "Unlock"
                      : "Encrypt local storage"}
                  </button>
                )}
              </div>
            </fieldset>
          </form>
        )}
        {error && (
          <>
            <p className="storage-error" role="alert">
              <Warning size={17} />
              {error}
            </p>
            <button type="button" disabled={busy} onClick={() => void check()}>
              Retry
            </button>
          </>
        )}
      </div>
    </main>
  );
}
