<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    icon,
    title,
    text,
    action,
  }: {
    icon: Snippet;
    title: string;
    text: string;
    action?: Snippet;
  } = $props();
</script>

<div class="empty-state">
  <div class="empty-icon">{@render icon()}</div>
  <h2>{title}</h2>
  <p>{text}</p>
  {@render action?.()}
</div>
