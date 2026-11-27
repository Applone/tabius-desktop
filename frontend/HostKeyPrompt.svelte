<script lang="ts">
  import { native } from "./api";
  import Modal from "./Modal.svelte";
  import type { HostKeyRequest } from "./model";

  let {
    request,
    sessionId,
    references,
    onAnswered,
    onAbort,
    command = "answer_host_key",
  }: {
    command?: string;
    request: HostKeyRequest;
    sessionId: string;
    references: string[];
    onAnswered: () => void;
    onAbort: () => void;
  } = $props();

  let busy = $state(false);
  let error = $state("");
  async function answer(decision: "abort" | "once" | "save") {
    if (busy) return;
    busy = true;
    error = "";
    try {
      await native(command, {
        id: sessionId,
        requestId: request.id,
        decision,
      });
      if (decision === "abort") onAbort();
      else onAnswered();
    } catch (err) {
      if (decision === "abort") onAbort();
      else {
        error = String(err);
        busy = false;
      }
    }
  }
</script>

<Modal
  title="Verify host fingerprint"
  onClose={() => {
    void answer("abort");
  }}
  ><p>
    This host is not in Known Hosts. Compare its fingerprint with one provided
    by the server administrator before continuing.
  </p>
  <dl class="host-key-details">
    <dt>Host</dt>
    <dd>{request.host}</dd>
    <dt>Key type</dt>
    <dd>{request.kind}</dd>
    <dt>Fingerprint</dt>
    <dd class="mono">{request.fingerprint}</dd>
  </dl>
  {#if references.length > 0}
    <div class="form-section">
      <h3>Synced references for this session</h3>
      {#each references as reference}
        <p class="mono">{reference}</p>
      {/each}
    </div>
  {/if}
  <p class="hint">
    Continue once leaves Known Hosts unchanged. Trust and continue saves this
    key for future connections.
  </p>
  {#if error}
    <p class="inline-error" role="alert">{error}</p>
  {/if}
  <div class="host-key-actions">
    <button
      type="button"
      disabled={busy}
      onclick={() => {
        void answer("abort");
      }}
    >
      Abort login
    </button>
    <button
      type="button"
      disabled={busy}
      onclick={() => {
        void answer("once");
      }}
    >
      Continue once
    </button>
    <button
      type="button"
      class="primary"
      disabled={busy}
      onclick={() => {
        void answer("save");
      }}
    >
      Trust and continue
    </button>
  </div>
</Modal>
