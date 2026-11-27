<script lang="ts">
  import {
    LinuxLogo,
    PencilSimple,
    HardDrives as Server,
    Trash,
    WindowsLogo,
  } from "phosphor-svelte";
  import appleIcon from "./assets/tabius/apple.svg";
  import type { Host } from "./model";

  let {
    host,
    onConnect,
    onEdit,
    onDelete,
  }: {
    host: Host;
    onConnect: () => void;
    onEdit: () => void;
    onDelete: () => void;
  } = $props();

  let system = $derived(host.system.toLowerCase());
  let isMac = $derived(/darwin|macos|mac os/.test(system));
  let Icon = $derived(
    /windows|mingw/.test(system)
      ? WindowsLogo
      : /linux|ubuntu|debian|arch|fedora|centos|alpine|redhat|rhel|rocky|alma|suse|gentoo|nixos/.test(
            system,
          )
        ? LinuxLogo
        : Server,
  );
</script>

<article class="host-card">
  <button
    type="button"
    class="host-open"
    aria-label={`Open session with ${host.name}`}
    title="Double-click to open a session"
    ondblclick={onConnect}
    onclick={(event) => {
      // Keyboard and assistive activation produce clicks without a pointer.
      if (event.detail === 0) onConnect();
    }}
  >
    <span
      class="host-icon"
      role="img"
      aria-label={host.system}
      title={host.system}
      >{#if isMac}
        <img src={appleIcon} alt="" width={24} height={24}>
      {:else}
        <Icon size={24} aria-hidden="true" />
      {/if}</span
    ><span class="host-name">{host.name}</span
    ><span class="host-address"
      >{host.address}<span class="host-port">:{host.port}</span></span
    >
  </button>
  <div class="card-actions">
    <button
      type="button"
      class="icon-button"
      aria-label={`Edit ${host.name}`}
      onclick={onEdit}
    >
      <PencilSimple size={16} aria-hidden="true" />
    </button>
    <button
      type="button"
      class="icon-button"
      aria-label={`Delete ${host.name}`}
      onclick={onDelete}
    >
      <Trash size={16} aria-hidden="true" />
    </button>
  </div>
</article>
