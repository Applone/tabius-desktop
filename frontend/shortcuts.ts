import { useEffect, useRef } from "react";

export const shortcutLabels = {
  newTab: "New tab",
  closeTab: "Close active tab",
  nextTab: "Next tab",
  previousTab: "Previous tab",
  snippets: "Open snippets",
  library: "Open host library",
  tab1: "Session tab 1",
  tab2: "Session tab 2",
  tab3: "Session tab 3",
  tab4: "Session tab 4",
  tab5: "Session tab 5",
  tab6: "Session tab 6",
  tab7: "Session tab 7",
  tab8: "Session tab 8",
  tab9: "Last session tab",
} as const;
export type ShortcutAction = keyof typeof shortcutLabels;
export type Shortcuts = Record<ShortcutAction, string>;
export const isMac = () =>
  typeof navigator !== "undefined" &&
  /Mac|iPhone|iPad/.test(navigator.platform);
export function shortcutDefaults(mac: boolean): Shortcuts {
  const primaryShift = mac ? "Shift+Meta" : "Ctrl+Shift";
  return {
    newTab: `${primaryShift}+KeyT`,
    closeTab: `${primaryShift}+KeyW`,
    nextTab: "Ctrl+Tab",
    previousTab: "Ctrl+Shift+Tab",
    snippets: `${primaryShift}+KeyS`,
    library: `${primaryShift}+KeyL`,
    tab1: `${primaryShift}+Digit1`,
    tab2: `${primaryShift}+Digit2`,
    tab3: `${primaryShift}+Digit3`,
    tab4: `${primaryShift}+Digit4`,
    tab5: `${primaryShift}+Digit5`,
    tab6: `${primaryShift}+Digit6`,
    tab7: `${primaryShift}+Digit7`,
    tab8: `${primaryShift}+Digit8`,
    tab9: `${primaryShift}+Digit9`,
  };
}
export function keyChord(
  event: Pick<
    KeyboardEvent,
    "code" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey"
  >,
): string {
  if (/^(Control|Alt|Shift|Meta)(Left|Right)$/.test(event.code) || !event.code)
    return "";
  if (
    !event.ctrlKey &&
    !event.altKey &&
    !event.metaKey &&
    !/^F([1-9]|1[0-2])$/.test(event.code)
  )
    return "";
  return [
    event.ctrlKey && "Ctrl",
    event.altKey && "Alt",
    event.shiftKey && "Shift",
    event.metaKey && "Meta",
    event.code,
  ]
    .filter(Boolean)
    .join("+");
}
export function chordLabel(chord: string): string {
  return chord
    ? chord
        .replace(/Key(?=[A-Z])/g, "")
        .replace(/Digit(?=\d)/g, "")
        .replace("Meta", "Cmd")
    : "Unassigned";
}
export function shortcutAction(
  event: KeyboardEvent,
  bindings: Shortcuts,
): ShortcutAction | undefined {
  if (
    event.defaultPrevented ||
    event.repeat ||
    event.isComposing ||
    event.getModifierState("AltGraph")
  )
    return;
  const chord = keyChord(event);
  if (!chord) return;
  return (Object.keys(shortcutLabels) as ShortcutAction[]).find(
    (action) => bindings[action] === chord,
  );
}
export function useShortcuts(
  overrides: Partial<Shortcuts> | undefined,
  enabled: boolean,
  onAction: (action: ShortcutAction) => void,
) {
  const callback = useRef(onAction);
  callback.current = onAction;
  useEffect(() => {
    if (!enabled) return;
    const bindings = { ...shortcutDefaults(isMac()), ...overrides };
    const handler = (event: KeyboardEvent) => {
      const target = event.target;
      if (
        document.querySelector("dialog[open]") ||
        (target instanceof window.Element &&
          (target.closest(
            "[data-shortcut-recorder], [contenteditable]:not([contenteditable='false'])",
          ) ||
            (target.closest("input, textarea, select") &&
              !target.closest(".xterm"))))
      )
        return;
      const action = shortcutAction(event, bindings);
      if (!action) return;
      event.preventDefault();
      event.stopPropagation();
      callback.current(action);
    };
    // Capture before xterm sees the event so app shortcuts never reach the shell.
    window.addEventListener("keydown", handler, true);
    return () => window.removeEventListener("keydown", handler, true);
  }, [overrides, enabled]);
}
