<script lang="ts">
  import {
    ArrowsLeftRight,
    Books,
    Plus,
    SidebarSimple,
    Terminal,
    X,
  } from "phosphor-svelte";
  import type { Session } from "./model";

  let {
    sessions,
    active,
    disabled,
    onSelect,
    onClose,
    onNew,
    onReorder,
    onTools,
    toolsOpen,
  }: {
    onTools?: () => void;
    toolsOpen: boolean;
    sessions: Session[];
    active: string;
    disabled: boolean;
    onSelect: (id: string, focusTerminal?: boolean) => void;
    onClose: (id: string) => void;
    onNew: () => void;
    onReorder: (source: string, target: string, after: boolean) => void;
  } = $props();

  let dragged: string | undefined;
  let drop = $state.raw<{ id: string; after: boolean } | undefined>();
  let items = $derived([
    { id: "library", label: "Library" },
    { id: "sftp", label: "SFTP" },
    ...sessions.map((session) => ({
      id: session.id,
      label: `${session.host.name}${session.forward ? " · Tunnel" : ""}`,
    })),
  ]);
</script>

<div class="tabbar" inert={disabled}>
  <div class="tab-list" role="tablist" aria-label="Workspace tabs">
    {#each [
      { kind: "navigation", items: items.slice(0, 2), offset: 0 },
      { kind: "session", items: items.slice(2), offset: 2 },
    ] as group (group.kind)}
      <div class={`${group.kind}-tabs`}>
        {#each group.items as item, groupIndex (item.id)}
          <div
            role="tab"
            aria-selected={active === item.id}
            aria-label={item.label}
            title={item.label}
            tabindex={active === item.id ? 0 : -1}
            class={`session-tab ${item.id === "library" || item.id === "sftp" ? "navigation-tab" : ""} ${active === item.id ? "active" : ""} ${drop?.id === item.id ? (drop.after ? "drop-after" : "drop-before") : ""}`}
            draggable={item.id !== "library" && item.id !== "sftp"}
            onclick={() => onSelect(item.id)}
            onkeydown={(event) => {
              if (event.target !== event.currentTarget) return;
              const index = group.offset + groupIndex;
              let next: number | undefined;
              if (event.key === "ArrowRight") next = (index + 1) % items.length;
              if (event.key === "ArrowLeft")
                next = (index - 1 + items.length) % items.length;
              if (event.key === "Home") next = 0;
              if (event.key === "End") next = items.length - 1;
              if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                onSelect(item.id);
              }
              if (
                event.key === "Delete" &&
                item.id !== "library" &&
                item.id !== "sftp"
              ) {
                event.preventDefault();
                onClose(item.id);
              }
              if (next !== undefined) {
                event.preventDefault();
                const target = items[next];
                onSelect(target.id, false);
                const elements = event.currentTarget
                  .closest('[role="tablist"]')
                  ?.querySelectorAll<HTMLElement>('[role="tab"]');
                elements?.[next]?.focus();
              }
            }}
            ondragstart={(event) => {
              if (
                !event.dataTransfer ||
                item.id === "library" ||
                item.id === "sftp" ||
                (event.target instanceof window.Element &&
                  event.target.closest("button"))
              ) {
                event.preventDefault();
                return;
              }
              dragged = item.id;
              event.dataTransfer.effectAllowed = "move";
              event.dataTransfer.setData("application/x-tabius-tab", item.id);
            }}
            ondragover={(event) => {
              if (
                !event.dataTransfer ||
                !dragged ||
                item.id === "library" ||
                item.id === "sftp" ||
                dragged === item.id
              )
                return;
              event.preventDefault();
              event.dataTransfer.dropEffect = "move";
              const rect = event.currentTarget.getBoundingClientRect();
              drop = {
                id: item.id,
                after: event.clientX > rect.left + rect.width / 2,
              };
            }}
            ondrop={(event) => {
              event.preventDefault();
              if (dragged && item.id !== "library" && item.id !== "sftp") {
                const rect = event.currentTarget.getBoundingClientRect();
                onReorder(
                  dragged,
                  item.id,
                  event.clientX > rect.left + rect.width / 2,
                );
              }
              dragged = undefined;
              drop = undefined;
            }}
            ondragend={() => {
              dragged = undefined;
              drop = undefined;
            }}
          >
            {#if item.id === "library"}
              <Books size={17} aria-hidden="true" />
            {:else if item.id === "sftp"}
              <ArrowsLeftRight size={17} aria-hidden="true" />
            {:else}
              <Terminal size={15} aria-hidden="true" />
            {/if}
            <span class="tab-title">{item.label}</span>
            {#if item.id !== "library" && item.id !== "sftp"}
              <button
                type="button"
                class="icon-button"
                aria-label={`Close ${item.label}`}
                onpointerdown={(event) => event.stopPropagation()}
                onclick={(event) => {
                  event.stopPropagation();
                  onClose(item.id);
                }}
              >
                <X size={13} aria-hidden="true" />
              </button>
            {/if}
          </div>
        {/each}
      </div>
    {/each}
    <button
      type="button"
      class="icon-button new-tab"
      aria-label="New tab"
      title="New tab"
      onclick={onNew}
    >
      <Plus size={17} aria-hidden="true" />
    </button>
  </div>
  {#if onTools}
    <button
      type="button"
      class="icon-button tools-toggle"
      aria-label="Toggle session tools"
      aria-pressed={toolsOpen}
      onclick={onTools}
    >
      <SidebarSimple size={19} aria-hidden="true" />
    </button>
  {/if}
</div>
