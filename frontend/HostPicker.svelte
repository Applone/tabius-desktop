<script lang="ts">
  import Field from "./Field.svelte";
  import Modal from "./Modal.svelte";
  import type { Host, Workspace } from "./model";

  let {
    workspace,
    onChoose,
    onNewHost,
    onClose,
  }: {
    workspace: Workspace;
    onChoose: (host: Host) => void;
    onNewHost: () => void;
    onClose: () => void;
  } = $props();

  let query = $state("");
  const groupName = (host: Host) =>
    workspace.groups?.find((group) => group.id === host.groupId)?.name ?? "";
  let matches = $derived(
    workspace.hosts.filter((host) =>
      `${host.name} ${host.address} ${groupName(host)}`
        .toLowerCase()
        .includes(query.trim().toLowerCase()),
    ),
  );
</script>

<Modal title="Open a session" {onClose}>
  <form
    onsubmit={(event) => {
      event.preventDefault();
      if (matches[0]) onChoose(matches[0]);
    }}
  >
    <Field label="Find a host">
      {#snippet control(id, describedBy)}
        <input
          {id}
          aria-describedby={describedBy}
          value={query}
          oninput={(event) => (query = event.currentTarget.value)}
          placeholder="Name, address, or group"
        >
      {/snippet}
    </Field>
    <div class="host-picker-list">
      {#each matches as host (host.id)}
        <button type="button" onclick={() => onChoose(host)}>
          <span class="picker-label"
            >{host.name}<small>{groupName(host)}</small></span
          ><span class="muted picker-label">{host.address}:{host.port}</span>
        </button>
      {/each}
      {#if !matches.length}
        <p class="helper">
          {workspace.hosts.length
            ? "No hosts match your search."
            : "Add a host to open your first session."}
        </p>
      {/if}
    </div>
    <button type="button" onclick={onNewHost}>New host</button>
  </form>
</Modal>
