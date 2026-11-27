<script lang="ts">
  import { Code, Palette, Play, Plus, X } from "phosphor-svelte";
  import { native } from "./api";
  import type { Workspace } from "./model";

  let {
    workspace,
    sessionId,
    onTheme,
    onNewSnippet,
    onClose,
  }: {
    workspace: Workspace;
    sessionId: string;
    onTheme: (theme: string) => void;
    onNewSnippet: () => void;
    onClose: () => void;
  } = $props();

  let tab = $state("Snippets");
  let filter = $state("");
  let error = $state("");
  async function run(command: string) {
    try {
      await native("write_session", {
        id: sessionId,
        data: `${command}\r`,
      });
      error = "";
    } catch (e) {
      error = String(e);
    }
  }
</script>

<aside class="session-tools" aria-label="Session tools">
  <header>
    <strong>Session tools</strong>
    <button
      type="button"
      class="icon-button"
      aria-label="Close session tools"
      onclick={onClose}
    >
      <X size={16} aria-hidden="true" />
    </button>
  </header>
  <nav aria-label="Session tools tabs">
    {#each [
      { name: "Snippets", icon: Code },
      { name: "Themes", icon: Palette },
    ] as { name, icon: Icon } (name)}
      <button
        type="button"
        class={tab === name ? "chosen" : ""}
        aria-pressed={tab === name}
        onclick={() => (tab = name)}
      >
        <Icon size={17} aria-hidden="true" />{name}
      </button>
    {/each}
  </nav>
  <div class="tools-content">
    {#if tab === "Themes"}
      <p class="muted">Terminal appearance</p>
      <div class="theme-options">
        {#each ["Graphite", "Midnight", "Paper"] as theme (theme)}
          <button
            type="button"
            class={`theme-option ${theme.toLowerCase()} ${workspace.settings.theme === theme ? "chosen" : ""}`}
            aria-pressed={workspace.settings.theme === theme}
            onclick={() => onTheme(theme)}
          >
            <div><span></span><span></span><span></span></div>
            {theme}
          </button>
        {/each}
      </div>
    {:else}
      <input
        aria-label="Search snippets"
        placeholder="Search snippets…"
        value={filter}
        oninput={(e) => (filter = e.currentTarget.value)}
      >
      <button type="button" class="tools-add" onclick={onNewSnippet}>
        <Plus size={16} aria-hidden="true" />New snippet
      </button>
      {#each workspace.snippets.filter((item) =>
        `${item.name} ${item.command}`
          .toLowerCase()
          .includes(filter.toLowerCase()),
      ) as item (item.id)}
        <article class="tool-command">
          <strong>{item.name}</strong>
          <pre>{item.command}</pre>
          <button type="button" onclick={() => void run(item.command)}>
            <Play size={14} aria-hidden="true" />Run in terminal
          </button>
        </article>
      {/each}
      {#if workspace.snippets.length === 0}
        <div class="tools-empty">
          <Code size={26} aria-hidden="true" />
          <h3>Keep useful commands close</h3>
          <p>Save a snippet to run it in this session.</p>
        </div>
      {/if}
    {/if}
    {#if error}
      <p role="alert">{error}</p>
    {/if}
  </div>
</aside>
