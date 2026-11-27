<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    label,
    hint,
    control,
    children,
  }: {
    label: string;
    hint?: string;
    control?: Snippet<[id: string, describedBy: string | undefined]>;
    children?: Snippet;
  } = $props();

  const id = $props.id();
  let describedBy = $derived(hint ? `${id}-hint` : undefined);
</script>

{#snippet description()}
  {#if hint}
    <small id={describedBy}>{hint}</small>
  {/if}
{/snippet}

{#if control}
  <div class="field">
    <label for={id}>{label}</label>
    {@render control(id, describedBy)}
    {@render description()}
  </div>
{:else}
  <fieldset class="field" aria-labelledby={`${id}-label`}>
    <legend id={`${id}-label`}>{label}</legend>
    {@render children?.()}
    {@render description()}
  </fieldset>
{/if}
