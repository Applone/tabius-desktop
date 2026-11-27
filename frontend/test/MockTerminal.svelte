<script lang="ts" module>
  export const mounts: string[] = [];
  export const unmounts: string[] = [];
</script>

<script lang="ts">
  import { onMount } from "svelte";
  import type { Session } from "../model";

  let {
    session,
    active,
    onClose,
    onTrustChanged,
  }: {
    session: Session;
    active: boolean;
    onClose: () => void;
    onTrustChanged: () => void;
  } = $props();

  onMount(() => {
    const { id } = session;
    mounts.push(id);
    return () => {
      unmounts.push(id);
    };
  });
</script>

<div class="xterm" data-active={String(active)} data-session={session.id}>
  <textarea aria-label={`Terminal ${session.id}`}></textarea>
  <button type="button" onclick={onTrustChanged}>
    Accept test fingerprint
  </button>
  <button type="button" onclick={onClose}>Report logout</button>
</div>
