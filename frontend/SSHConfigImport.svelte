<script lang="ts">
  import { CircleNotch, UploadSimple, Warning } from "phosphor-svelte";
  import { native } from "./api";
  import Field from "./Field.svelte";
  import Modal from "./Modal.svelte";
  import type { Host, Workspace } from "./model";

  interface Preview {
    hosts: Host[];
    keyCount: number;
    skipped: number;
    warnings: string[];
    fingerprint: string;
  }

  let {
    workspace,
    onClose,
    onSave,
    onPrepare,
  }: {
    workspace: Workspace;
    onClose: () => void;
    onSave: (workspace: Workspace, message: string) => void;
    onPrepare: () => Promise<void>;
  } = $props();

  let path = $state("~/.ssh/config");
  let loaded = $state.raw<{
    workspace: Workspace;
    preview: Preview;
  } | null>(null);
  let preview = $derived(
    loaded?.workspace === workspace ? loaded.preview : null,
  );
  let busy = $state(false);
  let error = $state("");
  async function submit() {
    const previous = workspace;
    const configPath = path;
    const pendingPreview = preview;
    busy = true;
    error = "";
    try {
      if (!pendingPreview) {
        const result = await native<Preview>("preview_ssh_config", {
          workspace: previous,
          path: configPath,
        });
        loaded = { workspace: previous, preview: result };
      } else {
        await onPrepare();
        const result = await native<{
          workspace: Workspace;
          imported: number;
          keyCount: number;
        }>("import_ssh_config", {
          workspace: previous,
          path: configPath,
          fingerprint: pendingPreview.fingerprint,
        });
        onSave(
          result.workspace,
          `Imported ${result.imported} hosts and ${result.keyCount} keys.`,
        );
      }
    } catch (err) {
      loaded = null;
      error = String(err).replace(/^Error: /, "");
    } finally {
      busy = false;
    }
  }
</script>

<Modal title="Import SSH config" {onClose}>
  <form
    onsubmit={(event) => {
      event.preventDefault();
      void submit();
    }}
  >
    <fieldset disabled={busy}>
      <Field label="Configuration file" hint="Defaults to ~/.ssh/config.">
        {#snippet control(id, describedBy)}
          <input
            {id}
            aria-describedby={describedBy}
            required
            value={path}
            oninput={(event) => {
              path = event.currentTarget.value;
              loaded = null;
              error = "";
            }}
          >
        {/snippet}
      </Field>
      <p class="helper">
        Import named hosts and migrate their private keys into Keychain.
        Encrypted keys keep their passphrases. Original files stay in place.
        Existing hosts are preserved; matching keys are reused.
      </p>
      {#if preview}
        <div class="form-section" aria-live="polite">
          <p>
            {preview.hosts.length}
            hosts and {preview.keyCount} new keys ready to import.
            {preview.skipped}
            hosts skipped.
          </p>
          {#if preview.hosts.length > 0}
            <ul>
              {#each preview.hosts as host (host.id)}
                <li>
                  <strong>{host.name}</strong>
                  · {host.address}:{host.port}
                </li>
              {/each}
            </ul>
          {/if}
          {#if preview.warnings.length > 0}
            <h3>Review before importing</h3>
            <ul>
              {#each preview.warnings as warning}
                <li>{warning}</li>
              {/each}
            </ul>
          {/if}
          {#if preview.hosts.length === 0}
            <p class="helper">
              No new named hosts were found. Wildcard entries provide defaults
              and are not imported as individual hosts.
            </p>
          {/if}
        </div>
      {/if}
      {#if error}
        <div class="inline-error" role="alert">
          <Warning size={17} aria-hidden="true" />{error}
        </div>
      {/if}
      <div class="modal-actions">
        <button type="button" onclick={onClose}>Cancel</button>
        <button
          type="submit"
          class="primary"
          disabled={preview?.hosts.length === 0}
        >
          {#if busy}
            <CircleNotch class="spin" size={17} aria-hidden="true" />
          {:else}
            <UploadSimple size={17} aria-hidden="true" />
          {/if}
          {busy ? "Working…" : preview ? "Import" : "Read configuration"}
        </button>
      </div>
    </fieldset>
  </form>
</Modal>
