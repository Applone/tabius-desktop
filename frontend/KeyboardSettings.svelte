<script lang="ts">
  import { Keyboard } from "phosphor-svelte";
  import type { Settings } from "./model";
  import {
    chordLabel,
    isMac,
    keyChord,
    type ShortcutAction,
    shortcutDefaults,
    shortcutLabels,
  } from "./shortcuts";

  let {
    settings,
    onChange,
    disabled,
  }: {
    settings: Settings;
    onChange: (settings: Settings) => void;
    disabled: boolean;
  } = $props();

  const actions = Object.keys(shortcutLabels) as ShortcutAction[];
  let recording = $state<ShortcutAction | undefined>();
  let error = $state("");
  let bindings = $derived({
    ...shortcutDefaults(typeof navigator !== "undefined" && isMac()),
    ...settings.shortcuts,
  });
</script>

<section class="settings-section" inert={disabled}>
  <div class="settings-section-heading">
    <Keyboard size={20} aria-hidden="true" />
    <h3>Keyboard shortcuts</h3>
  </div>
  <p class="helper">
    Click a shortcut and press a new combination. Use Ctrl, Alt, or Cmd with a
    key, or F1–F12. Escape cancels; Backspace clears.
  </p>
  {#each actions as action (action)}
    <div class="shortcut-row">
      <span>{shortcutLabels[action]}</span>
      <button
        type="button"
        data-shortcut-recorder
        aria-label={`Change shortcut for ${shortcutLabels[action]}`}
        aria-pressed={recording === action}
        onclick={() => {
          recording = action;
          error = "";
        }}
        onblur={() => (recording = undefined)}
        onkeydown={(event) => {
          if (recording !== action) return;
          if (
            event.key === "Tab" &&
            !event.ctrlKey &&
            !event.metaKey &&
            !event.altKey
          ) {
            recording = undefined;
            return;
          }
          event.preventDefault();
          event.stopPropagation();
          if (event.key === "Escape") {
            recording = undefined;
            return;
          }
          if (event.isComposing || event.getModifierState("AltGraph")) return;
          const clear =
            (event.key === "Backspace" || event.key === "Delete") &&
            !event.ctrlKey &&
            !event.metaKey &&
            !event.altKey &&
            !event.shiftKey;
          const chord = clear ? "" : keyChord(event);
          if (!clear && !chord) return;
          const conflict = (Object.keys(bindings) as ShortcutAction[]).find(
            (other) => other !== action && bindings[other] === chord && chord,
          );
          if (conflict) {
            error = `Already used for ${shortcutLabels[conflict]}. Choose another combination.`;
            return;
          }
          onChange({
            ...settings,
            shortcuts: { ...settings.shortcuts, [action]: chord },
          });
          recording = undefined;
          error = "";
        }}
      >
        {recording === action
          ? "Press shortcut…"
          : chordLabel(bindings[action])}
      </button>
    </div>
  {/each}
  {#if error}
    <p role="alert" class="inline-error">{error}</p>
  {/if}
  <button
    type="button"
    onclick={() => {
      onChange({ ...settings, shortcuts: {} });
      recording = undefined;
      error = "";
    }}
  >
    Reset shortcuts
  </button>
</section>
