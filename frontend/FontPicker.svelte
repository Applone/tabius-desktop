<script lang="ts">
  import { onMount } from "svelte";
  import { native } from "./api";
  import Field from "./Field.svelte";
  import { hasTerminalFont } from "./fonts";

  let {
    font,
    onChange,
  }: {
    font: string;
    onChange: (font: string) => void;
  } = $props();

  let fonts = $state.raw<string[] | null>(null);
  let failed = $state(false);
  onMount(() => {
    let cancelled = false;
    void native<string[]>("list_system_fonts")
      .then((families) => {
        if (!cancelled) fonts = families;
      })
      .catch(() => {
        if (!cancelled) failed = true;
      });
    return () => {
      cancelled = true;
    };
  });
  let choices = $derived([...new Set(["monospace", ...(fonts ?? []), font])]);
  let unavailable = $derived(fonts !== null && !hasTerminalFont(font, fonts));
  let hint = $derived(
    failed
      ? "Couldn’t detect installed fonts. Your saved font is preserved."
      : fonts === null
        ? "Detecting installed monospace fonts…"
        : unavailable
          ? "Saved font is not installed. Using the system monospace font."
          : "Monospace fonts installed on this computer.",
  );
</script>

<Field label="Font family" {hint}>
  {#snippet control(id, describedBy)}
    <select
      {id}
      aria-describedby={describedBy}
      value={font}
      onchange={(event) => onChange(event.currentTarget.value)}
    >
      {#each choices as family (family)}
        <option value={family}>
          {family === "monospace"
            ? "System monospace"
            : family === font && unavailable
              ? `${family} (not installed)`
              : family}
        </option>
      {/each}
    </select>
  {/snippet}
</Field>
