import { afterAll, afterEach, beforeEach, expect, mock, test } from "bun:test";
import { Window } from "happy-dom";
import { act } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import * as originalApi from "./api";
import type { Settings, Workspace } from "./model";

const browser = new Window();
const previousWindow = globalThis.window;
const previousDocument = globalThis.document;
globalThis.window = browser as unknown as typeof window;
globalThis.document = browser.document as unknown as Document;
Reflect.set(globalThis, "IS_REACT_ACT_ENVIRONMENT", true);
const api = { ...originalApi };
const invoke = mock(async (command: string): Promise<string[]> => {
  if (command !== "list_system_fonts") throw new Error(command);
  return ["DejaVu Sans Mono", "JetBrainsMono Nerd Font Mono"];
});
mock.module("./api", () => ({ ...api, native: invoke }));
const { SettingsView } = await import("./Settings");
const { createRoot } = await import("react-dom/client");
let root: ReturnType<typeof createRoot>;
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
  root = createRoot(container);
});
afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
});
afterAll(() => {
  mock.module("./api", () => api);
  globalThis.window = previousWindow;
  globalThis.document = previousDocument;
  Reflect.deleteProperty(globalThis, "IS_REACT_ACT_ENVIRONMENT");
  browser.happyDOM.abort();
});

const workspace: Workspace = {
  hosts: [],
  keys: [],
  snippets: [],
  forwards: [],
  history: [],
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

async function mount(font = "monospace") {
  await act(async () =>
    root.render(
      <SettingsView
        workspace={workspace}
        settings={{ ...workspace.settings, font }}
        syncing={false}
        onChange={changed}
        onSyncApply={async () => {}}
        onSyncBusy={() => {}}
        isCurrent={() => true}
      />,
    ),
  );
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

test("settings group terminal, session, and security on the left", () => {
  const view = SettingsView({
    workspace,
    settings: workspace.settings,
    syncing: true,
    onChange: () => {},
    onSyncApply: async () => {},
    onSyncBusy: () => {},
    isCurrent: () => true,
  });
  const [left, right] = view.props.children;
  const leftMarkup = renderToStaticMarkup(left);
  const rightMarkup = renderToStaticMarkup(right);
  for (const heading of [
    "Terminal",
    "Session behavior",
    "Connection security",
  ]) {
    expect(leftMarkup).toContain(`<h3>${heading}</h3>`);
    expect(rightMarkup).not.toContain(`<h3>${heading}</h3>`);
  }
  for (const heading of ["Configuration sync", "Migrate from Tabby-Web"]) {
    expect(rightMarkup).toContain(`<h3>${heading}</h3>`);
    expect(leftMarkup).not.toContain(`<h3>${heading}</h3>`);
  }
  expect(leftMarkup.match(/inert=""/g)).toHaveLength(3);
});

test("font preview recognizes system JetBrains Nerd Fonts and keeps the saved family", () => {
  const markup = renderToStaticMarkup(
    <SettingsView
      workspace={workspace}
      settings={{ ...workspace.settings, font: "JetBrains Mono" }}
      syncing={false}
      onChange={() => {}}
      onSyncApply={async () => {}}
      onSyncBusy={() => {}}
      isCurrent={() => true}
    />,
  );
  expect(markup).toContain(
    '<option value="JetBrains Mono" selected="">JetBrains Mono</option>',
  );
  expect(markup).toContain(
    "font-family:&quot;JetBrains Mono&quot;, &quot;JetBrainsMono Nerd Font Mono&quot;, &quot;JetBrainsMono Nerd Font&quot;, monospace;",
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
  await act(async () => {
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
  await act(async () => complete(["Hack"]));
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
