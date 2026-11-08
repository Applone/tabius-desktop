import {
  CheckCircle,
  GearSix,
  ShieldCheck,
  Terminal,
} from "@phosphor-icons/react";
import { KeyboardSettings } from "./KeyboardSettings";
import { MigrationSettings } from "./MigrationSettings";
import type { Settings, Workspace } from "./model";
import { SyncSettings } from "./SyncSettings";
import { Field, Toggle } from "./ui";
export function SettingsView({
  settings: s,
  onChange,
  workspace,
  onSyncApply,
  onSyncBusy,
  isCurrent,
  syncing,
}: {
  syncing: boolean;
  workspace: Workspace;
  onSyncApply: (workspace: Workspace) => Promise<void>;
  onSyncBusy: (busy: boolean) => void;
  isCurrent: () => boolean;
  settings: Settings;
  onChange: (settings: Settings) => void;
}) {
  const set = <K extends keyof Settings>(key: K, value: Settings[K]) =>
    onChange({ ...s, [key]: value });
  return (
    <div className="settings-layout">
      <div>
        <div className="settings-section" inert={syncing}>
          <div className="settings-section-heading">
            <Terminal size={20} />
            <div>
              <h3>Terminal</h3>
            </div>
          </div>
          <Field label="Emulator type" hint="Applies to new connections.">
            <select
              value={s.emulator}
              onChange={(e) => set("emulator", e.target.value)}
            >
              {["xterm-256color", "xterm", "linux", "vt100"].map((v) => (
                <option key={v}>{v}</option>
              ))}
            </select>
          </Field>
          <div className="form-columns">
            <Field label="Font family">
              <select
                value={s.font}
                onChange={(e) => set("font", e.target.value)}
              >
                {[
                  "monospace",
                  "JetBrains Mono",
                  "SFMono-Regular",
                  "Menlo",
                  "Consolas",
                  "DejaVu Sans Mono",
                ].map((v) => (
                  <option key={v}>{v}</option>
                ))}
              </select>
            </Field>
            <Field label="Font size">
              <select
                value={s.fontSize}
                onChange={(e) => set("fontSize", Number(e.target.value))}
              >
                {[10, 11, 12, 13, 14, 15, 16, 18, 20, 22, 24, 28, 32].map(
                  (v) => (
                    <option key={v} value={v}>
                      {v} px
                    </option>
                  ),
                )}
              </select>
            </Field>
          </div>
          <Field label="Line height">
            <input
              type="range"
              min="1"
              max="1.8"
              step="0.05"
              value={s.lineHeight}
              onChange={(e) => set("lineHeight", Number(e.target.value))}
            />
          </Field>
          <Field label="Theme">
            <div className="theme-options">
              {["Graphite", "Midnight", "Paper"].map((theme) => (
                <button
                  type="button"
                  key={theme}
                  className={`theme-option ${theme.toLowerCase()} ${s.theme === theme ? "chosen" : ""}`}
                  onClick={() => set("theme", theme)}
                >
                  <div>
                    <span />
                    <span />
                    <span />
                  </div>
                  {theme}
                  {s.theme === theme && <CheckCircle size={15} />}
                </button>
              ))}
            </div>
          </Field>
          <Toggle
            label="ANSI highlighting"
            description="Use color for the terminal’s ANSI palette."
            value={s.highlighting}
            onChange={(v) => set("highlighting", v)}
          />
          <div
            className={`terminal-preview ${s.theme.toLowerCase()}`}
            style={{
              fontFamily: s.font,
              fontSize: s.fontSize,
              lineHeight: s.lineHeight,
            }}
          >
            <span className={s.highlighting ? "accent" : ""}>you@tabius</span>{" "}
            <span className="muted">~</span> $ ssh your-server
            <br />
            <span className="muted">Linux server 6.12.0 x86_64</span>
            <br />$ <span className="preview-cursor" />
          </div>
        </div>
        <div className="settings-section" inert={syncing}>
          <div className="settings-section-heading">
            <GearSix size={20} />
            <div>
              <h3>Session behavior</h3>
            </div>
          </div>
          <Toggle
            label="Auto reconnect"
            description="Retry after an SSH transport error (exit 255), every 3 seconds."
            value={s.autoReconnect}
            onChange={(v) => set("autoReconnect", v)}
          />
          <Toggle
            label="Bell sound"
            description="Play a short tone for terminal bell events."
            value={s.bell}
            onChange={(v) => set("bell", v)}
          />
          <Toggle
            label="Use Option as Meta"
            description="Send Meta shortcuts from the Option key on macOS."
            value={s.optionAsMeta}
            onChange={(v) => set("optionAsMeta", v)}
          />
          <Toggle
            label="Detect host operating system"
            description="Run uname after login and update the host’s OS label."
            value={s.detectOS}
            onChange={(v) => set("detectOS", v)}
          />
        </div>
        <div className="settings-section" inert={syncing}>
          <div className="settings-section-heading">
            <ShieldCheck size={20} />
            <div>
              <h3>Connection security</h3>
            </div>
          </div>
          <Toggle
            label="Post-quantum key exchange"
            description="Use OpenSSH defaults. Disable to exclude ML-KEM and sntrup algorithms on new connections."
            value={s.postQuantum}
            onChange={(v) => set("postQuantum", v)}
          />
          <div className="security-note">
            <ShieldCheck size={19} />
            <p>
              Host keys are verified by OpenSSH. Unknown fingerprints require
              your confirmation; changed fingerprints block the connection.
            </p>
          </div>
        </div>
      </div>
      <div>
        <SyncSettings
          disabled={syncing}
          workspace={workspace}
          onApply={onSyncApply}
          onBusy={onSyncBusy}
          isCurrent={isCurrent}
        />
        <KeyboardSettings settings={s} onChange={onChange} disabled={syncing} />
        <MigrationSettings disabled={syncing} onBusy={onSyncBusy} />
      </div>
    </div>
  );
}
