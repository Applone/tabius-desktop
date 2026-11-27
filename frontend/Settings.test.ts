import { afterAll, afterEach, beforeEach, expect, mock, test } from "bun:test";
import { mount as mountComponent, unmount } from "svelte";
import * as originalApi from "./api";
import type { Settings, Workspace } from "./model";
import { settle } from "./test/dom";

const api = { ...originalApi };
const invoke = mock(async (command: string): Promise<string[]> => {
  if (command !== "list_system_fonts") throw new Error(command);
  return ["DejaVu Sans Mono", "JetBrainsMono Nerd Font Mono"];
});
mock.module("./api", () => ({ ...api, native: invoke }));
const { default: SettingsView } = await import("./SettingsView.svelte");
let root: Record<string, unknown> | undefined;
let container: HTMLDivElement;
const changed = mock((_settings: Settings) => {});

beforeEach(() => {
  invoke.mockReset();
  invoke.mockImplementation(async (command) => {
    if (command !== "list_system_fonts") throw new Error(command);
    return ["DejaVu Sans Mono", "JetBrainsMono Nerd Font Mono"];
  });
  changed.mockClear();
  container = document.createElement("div");
  document.body.append(container);
});
afterEach(async () => {
  if (root) await unmount(root);
  root = undefined;
  container.remove();
});
afterAll(() => {
  mock.module("./api", () => api);
});

const workspace: Workspace = {
  hosts: [],
  keys: [],
  snippets: [],
  forwards: [],
  settings: {
    autoReconnect: false,
    bell: false,
    optionAsMeta: true,
    emulator: "xterm-256color",
    font: "monospace",
    fontSize: 14,
    lineHeight: 1.2,
    theme: "Graphite",
    highlighting: true,
    detectOS: true,
    postQuantum: true,
  },
};

async function mount(font = "monospace", syncing = false) {
  root = mountComponent(SettingsView, {
    target: container,
    props: {
      workspace,
      settings: { ...workspace.settings, font },
      syncing,
      onChange: changed,
      onSyncApply: async () => {},
      onSyncBusy: () => {},
      isCurrent: () => true,
    },
  });
  await settle();
}
function fontSelect() {
  const label = [...container.querySelectorAll("label")].find(
    (label) => label.textContent === "Font family",
  );
  const select =
    label?.parentElement?.querySelector<HTMLSelectElement>("select");
  if (!select || select.id !== label?.htmlFor)
    throw new Error("Font picker is missing its labelled select");
  return select;
}

test("settings group terminal, session, and security on the left", async () => {
  await mount("monospace", true);
  const [left, right] =
    container.querySelector(".settings-layout")?.children ?? [];
  expect(left).toBeDefined();
  expect(right).toBeDefined();
  for (const heading of [
    "Terminal",
    "Session behavior",
    "Connection security",
  ]) {
    expect(left?.textContent).toContain(heading);
    expect(right?.textContent).not.toContain(heading);
  }
  for (const heading of ["Configuration sync", "Migrate from Tabby-Web"]) {
    expect(right?.textContent).toContain(heading);
    expect(left?.textContent).not.toContain(heading);
  }
  expect(left?.querySelectorAll("[inert]")).toHaveLength(3);
});

test("font preview recognizes system JetBrains Nerd Fonts and keeps the saved family", async () => {
  await mount("JetBrains Mono");
  expect(fontSelect().selectedOptions[0]?.textContent).toBe("JetBrains Mono");
  expect(
    container.querySelector<HTMLElement>(".terminal-preview")?.style.fontFamily,
  ).toBe(
    '"JetBrains Mono", "JetBrainsMono Nerd Font Mono", "JetBrainsMono Nerd Font", monospace',
  );
});

test("font picker lists detected families instead of hardcoded unavailable fonts", async () => {
  await mount();
  expect(invoke).toHaveBeenCalledWith("list_system_fonts");
  expect([...fontSelect().options].map((option) => option.value)).toEqual([
    "monospace",
    "DejaVu Sans Mono",
    "JetBrainsMono Nerd Font Mono",
  ]);
  expect(fontSelect().selectedOptions[0]?.textContent).toBe("System monospace");
  expect(container.textContent).toContain(
    "Monospace fonts installed on this computer.",
  );
  expect(changed).not.toHaveBeenCalled();
});

test("choosing a detected family saves its exact name and preserves other settings", async () => {
  await mount();
  await settle(() => {
    const select = fontSelect();
    select.value = "JetBrainsMono Nerd Font Mono";
    select.dispatchEvent(new window.Event("change", { bubbles: true }));
  });
  expect(changed).toHaveBeenCalledWith({
    ...workspace.settings,
    font: "JetBrainsMono Nerd Font Mono",
  });
});

test("missing saved fonts remain selected and explain the monospace fallback", async () => {
  await mount("Missing Font");
  expect(fontSelect().value).toBe("Missing Font");
  expect(fontSelect().selectedOptions[0]?.textContent).toBe(
    "Missing Font (not installed)",
  );
  expect(container.textContent).toContain(
    "Saved font is not installed. Using the system monospace font.",
  );
  expect(changed).not.toHaveBeenCalled();
});

test("legacy JetBrains selections recognize installed Nerd Font aliases", async () => {
  await mount("JetBrains Mono");
  expect(fontSelect().value).toBe("JetBrains Mono");
  expect(fontSelect().selectedOptions[0]?.textContent).toBe("JetBrains Mono");
  expect(container.textContent).not.toContain("not installed");
  expect(changed).not.toHaveBeenCalled();
});

test("a pending scan preserves the current selection and refreshes when it completes", async () => {
  let complete: (fonts: string[]) => void = () => {};
  invoke.mockImplementation(
    () => new Promise((resolve) => (complete = resolve)),
  );
  await mount("Hack");
  expect(fontSelect().value).toBe("Hack");
  expect(container.textContent).toContain(
    "Detecting installed monospace fonts…",
  );
  expect(container.textContent).not.toContain("not installed");
  await settle(() => complete(["Hack"]));
  expect(fontSelect().value).toBe("Hack");
  expect([...fontSelect().options].map((option) => option.value)).toEqual([
    "monospace",
    "Hack",
  ]);
  expect(changed).not.toHaveBeenCalled();
});

test("failed detection preserves the saved font and keeps system monospace available", async () => {
  invoke.mockRejectedValue(new Error("Scanner unavailable"));
  await mount("Hack");
  expect(fontSelect().value).toBe("Hack");
  expect([...fontSelect().options].map((option) => option.value)).toEqual([
    "monospace",
    "Hack",
  ]);
  expect(container.textContent).toContain(
    "Couldn’t detect installed fonts. Your saved font is preserved.",
  );
  expect(container.textContent).not.toContain("not installed");
  expect(changed).not.toHaveBeenCalled();
});

test("an empty scan leaves system monospace selectable", async () => {
  invoke.mockResolvedValue([]);
  await mount();
  expect([...fontSelect().options].map((option) => option.value)).toEqual([
    "monospace",
  ]);
  expect(fontSelect().value).toBe("monospace");
});
