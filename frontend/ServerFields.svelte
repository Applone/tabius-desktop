<script lang="ts">
  import Field from "./Field.svelte";
  import type { RemoteConfiguration } from "./sync";

  interface Connection {
    server: string;
    token: string;
    configurationId?: number;
    configurations: RemoteConfiguration[];
  }
  let {
    label,
    connection,
    onChange,
    onLoad,
  }: {
    label: string;
    connection: Connection;
    onChange: (connection: Connection) => void;
    onLoad: () => void;
  } = $props();
</script>

<div class="form-section">
  <h4>{label}</h4>
  <Field label={`${label} URL`}>
    {#snippet control(id, describedBy)}
      <input
        {id}
        aria-describedby={describedBy}
        type="url"
        placeholder="https://sync.example.com"
        value={connection.server}
        oninput={(e) =>
          onChange({
            ...connection,
            server: e.currentTarget.value,
            configurations: [],
            configurationId: undefined,
          })}
      >
    {/snippet}
  </Field>
  <Field label={`${label} synchronization token`}>
    {#snippet control(id, describedBy)}
      <input
        {id}
        aria-describedby={describedBy}
        type="password"
        autocomplete="off"
        value={connection.token}
        oninput={(e) =>
          onChange({
            ...connection,
            token: e.currentTarget.value,
            configurations: [],
            configurationId: undefined,
          })}
      >
    {/snippet}
  </Field>
  <button
    type="button"
    disabled={!connection.server.trim() || !connection.token.trim()}
    onclick={onLoad}
  >
    Load {label.toLowerCase()} configurations
  </button>
  <Field label={`${label} configuration`}>
    {#snippet control(id, describedBy)}
      <select
        {id}
        aria-describedby={describedBy}
        value={connection.configurationId ?? ""}
        onchange={(e) =>
          onChange({
            ...connection,
            configurationId: e.currentTarget.value
              ? Number(e.currentTarget.value)
              : undefined,
          })}
      >
        <option value="">Select configuration</option>
        {#each connection.configurations as config (config.id)}
          <option value={config.id}>{config.name}</option>
        {/each}
      </select>
    {/snippet}
  </Field>
</div>
