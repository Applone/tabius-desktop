<script lang="ts">
  import {
    ArrowsClockwise,
    CircleNotch,
    CloudArrowDown,
  } from "phosphor-svelte";
  import { untrack } from "svelte";
  import { native } from "./api";
  import Field from "./Field.svelte";
  import type { Workspace } from "./model";
  import type { RemoteConfiguration } from "./sync";

  let {
    disabled,
    workspace,
    onApply,
    onBusy,
    isCurrent,
  }: {
    disabled: boolean;
    workspace: Workspace;
    onApply: (workspace: Workspace) => Promise<void>;
    onBusy: (busy: boolean) => void;
    isCurrent: (previous: Workspace) => boolean;
  } = $props();

  let server = $state(untrack(() => workspace.sync?.server ?? ""));
  let token = $state("");
  let password = $state("");
  let encryption = $state<"modern" | "tabby">(
    untrack(() =>
      workspace.sync ? (workspace.sync.encryption ?? "tabby") : "modern",
    ),
  );
  let configurationId = $state<number | undefined>(
    untrack(() => workspace.sync?.configurationId),
  );
  let configurations = $state.raw<RemoteConfiguration[]>([]);
  let busy = $state(false);
  let error = $state("");
  let status = $state("");
  let warnings = $state.raw<string[]>([]);
  function selectConfiguration(id: number | undefined) {
    if (id !== configurationId) {
      encryption =
        server.trim().replace(/\/+$/, "") === workspace.sync?.server &&
        id === workspace.sync.configurationId
          ? (workspace.sync.encryption ?? "tabby")
          : "modern";
    }
    configurationId = id;
    status = "";
  }
  async function load() {
    busy = true;
    error = "";
    status = "";
    try {
      const list = await native<RemoteConfiguration[]>("list_configurations", {
        server,
        token,
      });
      configurations = list;
      selectConfiguration(
        list.some((x) => x.id === configurationId)
          ? configurationId
          : list.length === 1
            ? list[0].id
            : undefined,
      );
      if (!list.length)
        error =
          "No configurations found. Create one on your sync server first.";
    } catch (err) {
      error = String(err).replace(/^Error: /, "");
    } finally {
      busy = false;
    }
  }
  async function sync() {
    if (!configurationId) return;
    const previous = workspace;
    busy = true;
    onBusy(true);
    error = "";
    status = "";
    warnings = [];
    try {
      const result = await native<{ workspace: Workspace; warnings: string[] }>(
        "synchronize_workspace",
        {
          workspace: previous,
          server,
          token,
          configurationId,
          password,
          encryption,
        },
      );
      if (!isCurrent(previous))
        throw new Error(
          "The local workspace changed after upload. Sync again to apply the remote result.",
        );
      await onApply(result.workspace);
      warnings = result.warnings;
      status = "Synchronization complete.";
    } catch (err) {
      error = String(err).replace(/^Error: /, "");
    } finally {
      password = "";
      busy = false;
      onBusy(false);
    }
  }
</script>

<section class="settings-section sync-section">
  <div class="settings-section-heading">
    <ArrowsClockwise size={20} aria-hidden="true" />
    <h3>Configuration sync</h3>
  </div>
  <fieldset disabled={busy || disabled}>
    <Field label="Server URL">
      {#snippet control(id, describedBy)}
        <input
          {id}
          aria-describedby={describedBy}
          type="url"
          value={server}
          placeholder="https://sync.example.com"
          oninput={(e) => {
            server = e.currentTarget.value;
            encryption = "modern";
            configurations = [];
            configurationId = undefined;
            status = "";
          }}
        >
      {/snippet}
    </Field>
    <Field label="Synchronization token">
      {#snippet control(id, describedBy)}
        <input
          {id}
          aria-describedby={describedBy}
          type="password"
          autocomplete="off"
          value={token}
          oninput={(e) => {
            token = e.currentTarget.value;
            configurations = [];
            status = "";
          }}
        >
      {/snippet}
    </Field>
    <button
      type="button"
      disabled={!server.trim() || !token.trim()}
      onclick={() => {
        void load();
      }}
    >
      <CloudArrowDown size={17} aria-hidden="true" />Load configurations
    </button>
    <Field label="Configuration">
      {#snippet control(id, describedBy)}
        <select
          {id}
          aria-describedby={describedBy}
          value={configurationId ?? ""}
          onchange={(e) => {
            selectConfiguration(
              e.currentTarget.value ? Number(e.currentTarget.value) : undefined,
            );
            status = "";
          }}
        >
          <option value="">Select configuration</option>
          {#if configurationId &&
            !configurations.some((c) => c.id === configurationId)}
            <option value={configurationId}>
              Configuration {configurationId}
            </option>
          {/if}
          {#each configurations as config (config.id)}
            <option value={config.id}>{config.name}</option>
          {/each}
        </select>
      {/snippet}
    </Field>
    <Field
      label="Encryption"
      hint={encryption === "modern"
        ? "Encrypts the entire configuration. Upgrades existing vaults on sync. Requires a compatible server and clients."
        : "Legacy PBKDF2-SHA512 and AES-256-CBC. For Tabby-Web compatibility only; not recommended."}
    >
      {#snippet control(id, describedBy)}
        <select
          {id}
          aria-describedby={describedBy}
          value={encryption}
          onchange={(e) => {
            encryption = e.currentTarget.value === "tabby" ? "tabby" : "modern";
            status = "";
          }}
        >
          <option value="modern">
            Argon2id + XChaCha20-Poly1305 (recommended)
          </option>
          <option value="tabby">Tabby compatibility (legacy)</option>
        </select>
      {/snippet}
    </Field>
    <Field
      label="Vault password"
      hint="Required for modern encryption and existing encrypted vaults. Not saved."
    >
      {#snippet control(id, describedBy)}
        <input
          {id}
          aria-describedby={describedBy}
          type="password"
          autocomplete="off"
          value={password}
          oninput={(e) => (password = e.currentTarget.value)}
        >
      {/snippet}
    </Field>
    <p class="helper">
      Sync hosts, private keys, and fingerprint references. Keys require an
      encrypted vault. Saved passwords stay in the vault; SSH asks for them in
      the terminal. Advanced SSH options, snippets, forwarding rules, and
      terminal settings remain local.
    </p>
    <button
      type="button"
      class="primary"
      disabled={!configurationId ||
        !server.trim() ||
        !token.trim() ||
        (encryption === "modern" && !password)}
      onclick={() => {
        void sync();
      }}
    >
      {#if busy}
        <CircleNotch class="spin" size={17} aria-hidden="true" />
      {:else}
        <ArrowsClockwise size={17} aria-hidden="true" />
      {/if}
      Synchronize
    </button>
  </fieldset>
  {#if busy}
    <p role="status" class="sync-status">Connecting to sync server…</p>
  {/if}
  {#if error}
    <p class="inline-error" role="alert">{error}</p>
  {/if}
  {#if status}
    <p class="sync-status" role="status">{status}</p>
  {/if}
  {#if workspace.sync?.lastSyncedAt}
    <p class="helper sync-date">
      Last sync: {new Date(workspace.sync.lastSyncedAt).toLocaleString()}
    </p>
  {/if}
  {#if warnings.length > 0}
    <ul class="sync-warnings">
      {#each warnings as warning}
        <li>{warning}</li>
      {/each}
    </ul>
  {/if}
</section>
