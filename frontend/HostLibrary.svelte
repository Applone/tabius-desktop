<script lang="ts">
  import { Folder, PencilSimple, Plus, Trash } from "phosphor-svelte";
  import HostCard from "./HostCard.svelte";
  import type { Host, HostGroup, Workspace } from "./model";

  let {
    workspace,
    onConnect,
    onEdit,
    onDelete,
    onEditGroup,
    onDeleteGroup,
    onNewHost,
  }: {
    workspace: Workspace;
    onConnect: (host: Host) => void;
    onEdit: (host: Host) => void;
    onDelete: (host: Host) => void;
    onEditGroup: (group: HostGroup) => void;
    onDeleteGroup: (group: HostGroup) => void;
    onNewHost: (groupId: string) => void;
  } = $props();

  let groups = $derived(workspace.groups ?? []);
  let sections = $derived([
    ...groups,
    { id: "", name: groups.length ? "Ungrouped" : "Hosts" },
  ]);
</script>

{#each sections as group (group.id)}
  {@const hosts = workspace.hosts.filter((host) =>
    group.id
      ? host.groupId === group.id
      : !groups.some((g) => g.id === host.groupId),
  )}
  {#if !(!group.id && !hosts.length && groups.length)}
    <section class="host-group">
      <div class="group-heading">
        <h2>
          {#if group.id}
            <Folder size={17} aria-hidden="true" />
          {/if}
          {group.name}<span class="group-count">{hosts.length}</span>
        </h2>
        {#if group.id}
          <div class="group-actions">
            <button
              type="button"
              class="icon-button"
              aria-label={`Add host to ${group.name}`}
              onclick={() => onNewHost(group.id)}
            >
              <Plus size={16} aria-hidden="true" />
            </button>
            <button
              type="button"
              class="icon-button"
              aria-label={`Rename ${group.name}`}
              onclick={() => onEditGroup(group)}
            >
              <PencilSimple size={16} aria-hidden="true" />
            </button>
            <button
              type="button"
              class="icon-button"
              aria-label={`Delete group ${group.name}`}
              onclick={() => onDeleteGroup(group)}
            >
              <Trash size={16} aria-hidden="true" />
            </button>
          </div>
        {/if}
      </div>
      {#if hosts.length}
        <div class="host-grid">
          {#each hosts as host (host.id)}
            <HostCard
              {host}
              onConnect={() => onConnect(host)}
              onEdit={() => onEdit(host)}
              onDelete={() => onDelete(host)}
            />
          {/each}
        </div>
      {:else}
        <p class="helper">
          No hosts in this group. Add a host or assign one in host details.
        </p>
      {/if}
    </section>
  {/if}
{/each}
