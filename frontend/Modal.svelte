<script lang="ts">
  import { X } from "phosphor-svelte";
  import { onMount, type Snippet } from "svelte";

  let {
    title,
    children,
    onClose,
    wide = false,
  }: {
    title: string;
    children: Snippet;
    onClose: () => void;
    wide?: boolean;
  } = $props();

  let dialog: HTMLDialogElement | undefined = $state();

  onMount(() => {
    dialog?.showModal();
    dialog?.querySelector<HTMLElement>("input, select, textarea")?.focus();
  });

  function closeOnEscape(event: KeyboardEvent) {
    if (
      event.key === "Escape" &&
      !(
        event.target instanceof window.Element && event.target.closest("select")
      )
    ) {
      event.preventDefault();
      onClose();
    }
  }
</script>

<dialog
  bind:this={dialog}
  class={`modal ${wide ? "wide" : ""}`}
  oncancel={onClose}
  onkeydown={closeOnEscape}
  onclick={(event) => {
    if (event.target === event.currentTarget) onClose();
  }}
>
  <div class="modal-inner">
    <header>
      <div>
        <h2>{title}</h2>
      </div>
      <button
        type="button"
        class="icon-button"
        onclick={onClose}
        aria-label="Close dialog"
      >
        <X size={21} aria-hidden="true" />
      </button>
    </header>
    {@render children()}
  </div>
</dialog>
