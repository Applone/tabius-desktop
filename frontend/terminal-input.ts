// Terminal control sequences use physical keys; ordinary text keeps its layout.
export function terminalControl(event: KeyboardEvent): string | undefined {
  if (event.isComposing || event.getModifierState("AltGraph") || event.metaKey)
    return;
  const letter = /^Key([A-Z])$/.exec(event.code)?.[1];
  if (event.ctrlKey && !event.shiftKey) {
    const code = letter
      ? letter.charCodeAt(0) - 64
      : (
          {
            Space: 0,
            BracketLeft: 27,
            Backslash: 28,
            BracketRight: 29,
            Digit6: 30,
            Minus: 31,
          } as Record<string, number>
        )[event.code];
    if (code !== undefined)
      return `${event.altKey ? "\x1b" : ""}${String.fromCharCode(code)}`;
  }
  if (event.altKey && !event.ctrlKey && letter)
    return `\x1b${event.shiftKey ? letter : letter.toLowerCase()}`;
}

export function bannerSystem(banner: string): string | undefined {
  return [
    ["Ubuntu", /\bUbuntu\b/i],
    ["Debian", /\bDebian\b/i],
    ["Fedora", /\bFedora\b/i],
    ["Arch Linux", /\bArch Linux\b/i],
    ["Alpine", /\bAlpine\b/i],
    ["macOS", /\b(?:Darwin|macOS)\b/i],
    ["Windows", /\b(?:Windows|Microsoft Windows)\b/i],
    ["Linux", /\bLinux\b/i],
  ].find(([, pattern]) => (pattern as RegExp).test(banner))?.[0] as
    | string
    | undefined;
}
