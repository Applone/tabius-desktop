import { Keyboard } from "@phosphor-icons/react";
import { useState } from "react";
import type { Settings } from "./model";
import {
  chordLabel,
  isMac,
  keyChord,
  type ShortcutAction,
  shortcutDefaults,
  shortcutLabels,
} from "./shortcuts";

export function KeyboardSettings({
  settings,
  onChange,
  disabled,
}: {
  settings: Settings;
  onChange: (settings: Settings) => void;
  disabled: boolean;
}) {
  const [recording, setRecording] = useState<ShortcutAction>();
  const [error, setError] = useState("");
  const bindings = {
    ...shortcutDefaults(typeof navigator !== "undefined" && isMac()),
    ...settings.shortcuts,
  };
  return (
    <section className="settings-section" inert={disabled}>
      <div className="settings-section-heading">
        <Keyboard size={20} />
        <h3>Keyboard shortcuts</h3>
      </div>
      <p className="helper">
        Click a shortcut and press a new combination. Use Ctrl, Alt, or Cmd with
        a key, or F1–F12. Escape cancels; Backspace clears.
      </p>
      {(Object.keys(shortcutLabels) as ShortcutAction[]).map((action) => (
        <div className="shortcut-row" key={action}>
          <span>{shortcutLabels[action]}</span>
          <button
            type="button"
            data-shortcut-recorder
            aria-label={`Change shortcut for ${shortcutLabels[action]}`}
            aria-pressed={recording === action}
            onClick={() => {
              setRecording(action);
              setError("");
            }}
            onBlur={() => setRecording(undefined)}
            onKeyDown={(event) => {
              if (recording !== action) return;
              if (
                event.key === "Tab" &&
                !event.ctrlKey &&
                !event.metaKey &&
                !event.altKey
              ) {
                setRecording(undefined);
                return;
              }
              event.preventDefault();
              event.stopPropagation();
              if (event.key === "Escape") {
                setRecording(undefined);
                return;
              }
              if (
                event.nativeEvent.isComposing ||
                event.getModifierState("AltGraph")
              )
                return;
              const clear =
                (event.key === "Backspace" || event.key === "Delete") &&
                !event.ctrlKey &&
                !event.metaKey &&
                !event.altKey &&
                !event.shiftKey;
              const chord = clear ? "" : keyChord(event);
              if (!clear && !chord) return;
              const conflict = (Object.keys(bindings) as ShortcutAction[]).find(
                (other) =>
                  other !== action && bindings[other] === chord && chord,
              );
              if (conflict) {
                setError(
                  `Already used for ${shortcutLabels[conflict]}. Choose another combination.`,
                );
                return;
              }
              onChange({
                ...settings,
                shortcuts: { ...settings.shortcuts, [action]: chord },
              });
              setRecording(undefined);
              setError("");
            }}
          >
            {recording === action
              ? "Press shortcut…"
              : chordLabel(bindings[action])}
          </button>
        </div>
      ))}
      {error && (
        <p role="alert" className="inline-error">
          {error}
        </p>
      )}
      <button
        type="button"
        onClick={() => {
          onChange({ ...settings, shortcuts: {} });
          setRecording(undefined);
          setError("");
        }}
      >
        Reset shortcuts
      </button>
    </section>
  );
}
