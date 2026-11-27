<script lang="ts">
  let {
    label,
    description,
    value,
    onChange,
  }: {
    label: string;
    description?: string;
    value: boolean;
    onChange: (value: boolean) => void;
  } = $props();
</script>

<label class="toggle-row">
  <span>
    <strong>{label}</strong>
    {#if description}
      <small>{description}</small>
    {/if}
  </span>
  <input
    type="checkbox"
    checked={value}
    onchange={(event) => onChange(event.currentTarget.checked)}
  >
  <span class="switch"></span>
</label>
