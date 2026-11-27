<script lang="ts" module>
  export interface StorageStatus {
    state: "setup" | "locked" | "ready";
    keyringUnavailable: boolean;
  }
</script>

<script lang="ts">
  import { CircleNotch, LockKey, Warning } from "phosphor-svelte";
  import { onMount, type Snippet } from "svelte";
  import { native } from "./api";
  import Field from "./Field.svelte";

  let { children }: { children: Snippet } = $props();

  let status = $state.raw<StorageStatus | undefined>();
  let password = $state("");
  let confirmation = $state("");
  let busy = $state(false);
  let error = $state("");
  const check = async () => {
    busy = true;
    error = "";
    try {
      status = await native<StorageStatus>("storage_status");
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  };
  onMount(() => {
    void check();
  });
  async function configure(mode: "system" | "manual" | "plain") {
    if (busy) return;
    if (mode !== "plain" && password !== confirmation) {
      error = "The passwords do not match.";
      return;
    }
    busy = true;
    error = "";
    try {
      const next = await native<StorageStatus>("configure_storage", {
        mode,
        password: mode === "plain" ? "" : password,
      });
      status = next;
      if (next.state === "ready") {
        password = "";
        confirmation = "";
      }
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }
  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (busy) return;
    if (status?.state !== "locked") {
      await configure(status?.keyringUnavailable ? "manual" : "system");
      return;
    }
    busy = true;
    error = "";
    try {
      await native("unlock_storage", { password });
      password = "";
      status = { state: "ready", keyringUnavailable: false };
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }
</script>

{#if status?.state === "ready"}
  {@render children()}
{:else}
  <main class="storage-gate">
    <div class="settings-section storage-panel">
      <LockKey size={28} class="accent" aria-hidden="true" />
      <h1>
        {!status
          ? error
            ? "Local storage unavailable"
            : "Opening Tabius"
          : status.state === "locked"
            ? "Unlock Tabius"
            : "Protect your local storage"}
      </h1>
      {#if !status}
        <p>Checking local storage…</p>
        {#if busy}
          <CircleNotch class="spin" size={22} aria-hidden="true" />
        {/if}
      {:else}
        <form onsubmit={(event) => void submit(event)}>
          <fieldset disabled={busy}>
            <p>
              {status.state === "locked"
                ? "Enter your local storage password to open your workspace."
                : "Choose a password to encrypt your hosts, keys, snippets, and settings. Existing local data will be encrypted too."}
            </p>
            {#if status?.keyringUnavailable}
              <p class="storage-note">
                {status.state === "setup"
                  ? "System secret storage is unavailable. Choose whether to enter your password on every launch or continue without encryption."
                  : "The saved password could not be retrieved from system secret storage. You can still unlock with your original password."}
              </p>
            {/if}
            <Field label="Local storage password">
              {#snippet control(id, describedBy)}
                <input
                  {id}
                  aria-describedby={describedBy}
                  type="password"
                  required
                  autocomplete={status?.state === "locked"
                    ? "current-password"
                    : "new-password"}
                  value={password}
                  oninput={(event) => (password = event.currentTarget.value)}
                >
              {/snippet}
            </Field>
            {#if status.state === "setup"}
              <Field label="Confirm password">
                {#snippet control(id, describedBy)}
                  <input
                    {id}
                    aria-describedby={describedBy}
                    type="password"
                    required
                    autocomplete="new-password"
                    value={confirmation}
                    oninput={(event) =>
                      (confirmation = event.currentTarget.value)}
                  >
                {/snippet}
              </Field>
              <p class="storage-note">
                {status?.keyringUnavailable
                  ? "Keep this password safe. It cannot be recovered if you forget it."
                  : "Your password will be saved in your system's secret storage so Tabius can unlock automatically. Keep a copy in case that entry becomes unavailable."}
              </p>
            {/if}
            <div class="storage-actions">
              {#if status.state === "setup" && status?.keyringUnavailable}
                <button
                  type="button"
                  class="primary"
                  disabled={!password || password !== confirmation}
                  onclick={() => void configure("manual")}
                >
                  Ask for password on every launch
                </button>
                <button type="button" onclick={() => void configure("plain")}>
                  Continue without encryption
                </button>
              {:else}
                <button type="submit" class="primary">
                  {#if busy}
                    <CircleNotch class="spin" size={16} aria-hidden="true" />
                  {/if}
                  {status.state === "locked"
                    ? "Unlock"
                    : "Encrypt local storage"}
                </button>
              {/if}
            </div>
          </fieldset>
        </form>
      {/if}
      {#if error}
        <p class="storage-error" role="alert">
          <Warning size={17} aria-hidden="true" />{error}
        </p>
        <button type="button" disabled={busy} onclick={() => void check()}>
          Retry
        </button>
      {/if}
    </div>
  </main>
{/if}
