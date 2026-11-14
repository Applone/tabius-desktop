const jetBrainsFamilies = [
  "JetBrains Mono",
  "JetBrainsMono Nerd Font Mono",
  "JetBrainsMono Nerd Font",
];

export function terminalFontFamily(font: string): string {
  if (font === "monospace") return font;
  // Nerd Fonts use different family names from the upstream JetBrains font.
  if (font === "JetBrains Mono")
    return `${jetBrainsFamilies.map((family) => JSON.stringify(family)).join(", ")}, monospace`;
  return `${JSON.stringify(font)}, monospace`;
}

export function hasTerminalFont(font: string, installed: string[]): boolean {
  const families = font === "JetBrains Mono" ? jetBrainsFamilies : [font];
  return (
    font === "monospace" ||
    installed.some((name) =>
      families.some((family) => family.toLowerCase() === name.toLowerCase()),
    )
  );
}
