export function terminalFontFamily(font: string): string {
  if (font === "monospace") return font;
  // Nerd Fonts use different family names from the upstream JetBrains font.
  if (font === "JetBrains Mono")
    return '"JetBrains Mono", "JetBrainsMono Nerd Font Mono", "JetBrainsMono Nerd Font", monospace';
  return `${JSON.stringify(font)}, monospace`;
}
