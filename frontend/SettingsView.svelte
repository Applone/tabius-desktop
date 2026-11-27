<script lang="ts">
  import { CheckCircle, GearSix, ShieldCheck, Terminal } from "phosphor-svelte";
  import Field from "./Field.svelte";
  import FontPicker from "./FontPicker.svelte";
  import { terminalFontFamily } from "./fonts";
  import KeyboardSettings from "./KeyboardSettings.svelte";
  import MigrationSettings from "./MigrationSettings.svelte";
  import type { Settings, Workspace } from "./model";
  import SyncSettings from "./SyncSettings.svelte";
  import Toggle from "./Toggle.svelte";

  let {
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
    isCurrent: (previous: Workspace) => boolean;
    settings: Settings;
    onChange: (settings: Settings) => void;
  } = $props();

  const set = <K extends keyof Settings>(key: K, value: Settings[K]) =>
    onChange({ ...s, [key]: value });
</script>

<div class="settings-layout">
  <div>
    <div class="settings-section" inert={syncing}>
      <div class="settings-section-heading">
        <Terminal size={20} aria-hidden="true" />
        <div><h3>Terminal</h3></div>
      </div>
      <Field label="Emulator type" hint="Applies to new connections.">
        {#snippet control(id, describedBy)}
          <select
            {id}
            aria-describedby={describedBy}
            value={s.emulator}
            onchange={(e) => set("emulator", e.currentTarget.value)}
          >
            {#each ["xterm-256color", "xterm", "linux", "vt100"] as v (v)}
              <option>{v}</option>
            {/each}
          </select>
        {/snippet}
      </Field>
      <div class="form-columns">
        <FontPicker font={s.font} onChange={(font) => set("font", font)} />
        <Field label="Font size">
          {#snippet control(id, describedBy)}
            <select
              {id}
              aria-describedby={describedBy}
              value={s.fontSize}
              onchange={(e) => set("fontSize", Number(e.currentTarget.value))}
            >
              {#each [
                10, 11, 12, 13, 14, 15, 16, 18, 20, 22, 24, 28, 32,
              ] as v (v)}
                <option value={v}>{v} pt</option>
              {/each}
            </select>
          {/snippet}
        </Field>
      </div>
      <Field label="Line height">
        {#snippet control(id, describedBy)}
          <input
            {id}
            aria-describedby={describedBy}
            type="range"
            min="1"
            max="1.8"
            step="0.05"
            value={s.lineHeight}
            oninput={(e) => set("lineHeight", Number(e.currentTarget.value))}
          >
        {/snippet}
      </Field>
      <Field label="Theme">
        <div class="theme-options">
          {#each ["Graphite", "Midnight", "Paper"] as theme (theme)}
            <button
              type="button"
              class={`theme-option ${theme.toLowerCase()} ${s.theme === theme ? "chosen" : ""}`}
              onclick={() => set("theme", theme)}
            >
              <div><span></span><span></span><span></span></div>
              {theme}
              {#if s.theme === theme}
                <CheckCircle size={15} aria-hidden="true" />
              {/if}
            </button>
          {/each}
        </div>
      </Field>
      <Toggle
        label="ANSI highlighting"
        description="Use color for the terminal’s ANSI palette."
        value={s.highlighting}
        onChange={(v) => set("highlighting", v)}
      />
      <div
        class={`terminal-preview ${s.theme.toLowerCase()}`}
        style:font-family={terminalFontFamily(s.font)}
        style:font-size={`${s.fontSize}pt`}
        style:line-height={s.lineHeight}
      >
        <span class={s.highlighting ? "accent" : ""}>you@tabius</span>{" "}<span
          class="muted"
          >~</span
        >
        $ ssh your-server<br>
        <span class="muted">Linux server 6.12.0 x86_64</span><br>$
        <span class="preview-cursor"></span>
      </div>
    </div>
    <div class="settings-section" inert={syncing}>
      <div class="settings-section-heading">
        <GearSix size={20} aria-hidden="true" />
        <div><h3>Session behavior</h3></div>
      </div>
      <Toggle
        label="Auto reconnect"
        description="Retry after an SSH transport error (exit 255), every 3 seconds."
        value={s.autoReconnect}
        onChange={(v) => set("autoReconnect", v)}
      /><Toggle
        label="Bell sound"
        description="Play a short tone for terminal bell events."
        value={s.bell}
        onChange={(v) => set("bell", v)}
      /><Toggle
        label="Use Option as Meta"
        description="Send Meta shortcuts from the Option key on macOS."
        value={s.optionAsMeta}
        onChange={(v) => set("optionAsMeta", v)}
      /><Toggle
        label="Detect host operating system"
        description="Run uname after login and update the host’s OS label."
        value={s.detectOS}
        onChange={(v) => set("detectOS", v)}
      />
    </div>
    <div class="settings-section" inert={syncing}>
      <div class="settings-section-heading">
        <ShieldCheck size={20} aria-hidden="true" />
        <div><h3>Connection security</h3></div>
      </div>
      <Toggle
        label="Post-quantum key exchange"
        description="Use OpenSSH defaults. Disable to exclude ML-KEM and sntrup algorithms on new connections."
        value={s.postQuantum}
        onChange={(v) => set("postQuantum", v)}
      />
      <div class="security-note">
        <ShieldCheck size={19} aria-hidden="true" />
        <p>
          Host keys are verified by OpenSSH. Unknown fingerprints require your
          confirmation; changed fingerprints block the connection.
        </p>
      </div>
    </div>
  </div>
  <div>
    <SyncSettings
      disabled={syncing}
      {workspace}
      onApply={onSyncApply}
      onBusy={onSyncBusy}
      {isCurrent}
    /><KeyboardSettings settings={s} {onChange} disabled={syncing} />
    <MigrationSettings disabled={syncing} onBusy={onSyncBusy} />
  </div>
</div>
