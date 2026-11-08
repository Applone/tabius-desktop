import {
  afterAll,
  afterEach,
  beforeEach,
  describe,
  expect,
  mock,
  test,
} from "bun:test";
import { Window as HappyWindow } from "happy-dom";
import { act, useEffect } from "react";
import * as originalApi from "./api";
import type { Host, KnownHost, Settings, Workspace } from "./model";
import {
  chordLabel,
  keyChord,
  shortcutAction,
  shortcutDefaults,
} from "./shortcuts";
import { adjacentTab, cycleTab, reorderTabs } from "./Tabs";

const browser = new HappyWindow();
const previousWindow = globalThis.window;
const previousDocument = globalThis.document;
const previousNavigator = Object.getOwnPropertyDescriptor(
  globalThis,
  "navigator",
);
globalThis.window = browser as unknown as typeof previousWindow;
globalThis.document = browser.document as unknown as Document;
Reflect.set(globalThis, "IS_REACT_ACT_ENVIRONMENT", true);
const api = { ...originalApi };
const mounts: string[] = [];
const unmounts: string[] = [];
function MockTerminal({
  session,
  active,
}: {
  session: { id: string };
  active: boolean;
}) {
  useEffect(() => {
    mounts.push(session.id);
    return () => {
      unmounts.push(session.id);
    };
  }, [session.id]);
  return (
    <div className="xterm" data-active={active} data-session={session.id}>
      <textarea aria-label={`Terminal ${session.id}`} />
    </div>
  );
}
const originalTerminal = { ...(await import("./Terminal")) };
mock.module("./Terminal", () => ({ TerminalPane: MockTerminal }));
const settings: Settings = {
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
};
function host(id: string, groupId = ""): Host {
  return {
    id,
    name: id,
    address: `${id}.example.com`,
    port: 22,
    username: "",
    keyId: "",
    agentForwarding: false,
    startup: "",
    jump: "",
    proxy: "",
    env: "",
    utf8: true,
    mosh: false,
    system: "Server",
    groupId,
  };
}
let workspace: Workspace;
let trustedHosts: KnownHost[] = [];
let sequence = 0;
const invoke = mock(
  async (name: string, args?: Record<string, unknown>): Promise<unknown> => {
    if (name === "known_hosts") return trustedHosts;
    if (name === "display_known") return args?.known ?? [];
    if (name === "prepare_session")
      return { id: `session-${++sequence}`, host: args?.host };
    if (name === "new_item")
      return args?.kind === "group"
        ? { id: "new-group", name: "" }
        : host("new-host");
    if (name === "edit_workspace" || name === "remove_item") {
      const current = args?.workspace as Workspace;
      if (args?.kind === "group") {
        const item = args?.item as { id: string; name: string };
        if (name === "remove_item")
          return {
            ...current,
            groups: current.groups?.filter((group) => group.id !== item.id),
            hosts: current.hosts.map((host) =>
              host.groupId === item.id ? { ...host, groupId: "" } : host,
            ),
          };
        return {
          ...current,
          groups: [
            ...(current.groups ?? []).filter((g) => g.id !== item.id),
            item,
          ],
        };
      }
      if (args?.kind === "host") {
        const item = args?.item as Host;
        return {
          ...current,
          hosts: [...current.hosts.filter((h) => h.id !== item.id), item],
        };
      }
    }
    throw new Error(`Unexpected command: ${name}`);
  },
);
const save = mock(async (next: Workspace) => {
  workspace = next;
});
mock.module("./api", () => ({
  ...api,
  desktop: true,
  loadWorkspace: async () => workspace,
  saveWorkspace: save,
  native: invoke,
}));
const { createRoot } = await import("react-dom/client");
const { App } = await import("./App");
let root: ReturnType<typeof createRoot>;
let container: HTMLDivElement;

afterAll(() => {
  mock.module("./api", () => api);
  mock.module("./Terminal", () => originalTerminal);
  globalThis.window = previousWindow;
  globalThis.document = previousDocument;
  if (previousNavigator)
    Object.defineProperty(globalThis, "navigator", previousNavigator);
  else Reflect.deleteProperty(globalThis, "navigator");
  Reflect.deleteProperty(globalThis, "IS_REACT_ACT_ENVIRONMENT");
  browser.happyDOM.abort();
});

describe.each(["Linux x86_64", "MacIntel", "Win32"])(
  "workspace on %s",
  (platform) => {
    const primaryShift = {
      shiftKey: true,
      ctrlKey: platform !== "MacIntel",
      metaKey: platform === "MacIntel",
    };
    beforeEach(() => {
      Object.defineProperty(globalThis, "navigator", {
        configurable: true,
        value: { platform },
      });
      workspace = {
        hosts: [host("Alpha", "production"), host("Beta")],
        groups: [{ id: "production", name: "Production" }],
        keys: [],
        snippets: [],
        forwards: [],
        history: [],
        settings: { ...settings },
      };
      sequence = 0;
      trustedHosts = [];
      mounts.length = 0;
      unmounts.length = 0;
      invoke.mockClear();
      save.mockClear();
      container = document.createElement("div");
      document.body.append(container);
      root = createRoot(container);
    });
    afterEach(async () => {
      await act(async () => root.unmount());
      container.remove();
    });

    async function mount() {
      await act(async () => root.render(<App />));
    }
    async function click(selector: string, label?: string) {
      const element = [
        ...container.querySelectorAll<HTMLElement>(selector),
      ].find(
        (element) =>
          label === undefined ||
          element.textContent?.trim() === label ||
          element.getAttribute("aria-label") === label,
      );
      expect(element).toBeDefined();
      await act(async () => element?.click());
    }
    async function key(
      code: string,
      init: KeyboardEventInit = {},
      target: Element | Window = window,
    ) {
      const event = new window.KeyboardEvent("keydown", {
        code,
        key: code.replace("Key", ""),
        bubbles: true,
        cancelable: true,
        ...init,
      });
      // Happy DOM conflates ordinary Alt with AltGraph; model ordinary key presses.
      const modifierState = event.getModifierState.bind(event);
      Object.defineProperty(event, "getModifierState", {
        value: (name: string) =>
          name === "AltGraph" ? false : modifierState(name),
      });
      await act(async () => {
        target.dispatchEvent(event);
      });
      return event;
    }
    async function input(selector: string, value: string) {
      const element = container.querySelector<HTMLInputElement>(selector);
      expect(element).not.toBeNull();
      await act(async () => {
        Object.getOwnPropertyDescriptor(
          browser.HTMLInputElement.prototype,
          "value",
        )?.set?.call(element, value);
        element?.dispatchEvent(new window.Event("input", { bubbles: true }));
      });
    }
    async function submit() {
      await act(async () => {
        container
          .querySelector("dialog form")
          ?.dispatchEvent(
            new window.Event("submit", { bubbles: true, cancelable: true }),
          );
      });
    }
    async function openHost(index = 0) {
      await click('button[aria-label="New tab"]');
      const buttons = container.querySelectorAll<HTMLButtonElement>(
        ".host-picker-list button",
      );
      await act(async () => buttons[index]?.click());
    }
    const selectedTab = () =>
      container
        .querySelector('[role="tab"][aria-selected="true"]')
        ?.getAttribute("aria-label");

    test("shortcut defaults match physical keys on Linux and macOS without duplicates", () => {
      for (const mac of [false, true]) {
        const defaults = shortcutDefaults(mac);
        expect(new Set(Object.values(defaults)).size).toBe(
          Object.keys(defaults).length,
        );
        const event = new window.KeyboardEvent("keydown", {
          code: "KeyT",
          shiftKey: true,
          ctrlKey: !mac,
          metaKey: mac,
        });
        expect(shortcutAction(event, defaults)).toBe("newTab");
        expect(
          shortcutAction(
            new window.KeyboardEvent("keydown", {
              code: "Digit9",
              shiftKey: true,
              ctrlKey: !mac,
              metaKey: mac,
            }),
            defaults,
          ),
        ).toBe("tab9");
        expect(
          shortcutAction(
            new window.KeyboardEvent("keydown", { code: "KeyT" }),
            defaults,
          ),
        ).toBeUndefined();
        expect(
          shortcutAction(
            new window.KeyboardEvent("keydown", {
              code: "KeyT",
              ctrlKey: !mac,
              metaKey: mac,
              shiftKey: true,
              repeat: true,
            }),
            defaults,
          ),
        ).toBeUndefined();
        expect(
          shortcutAction(
            new window.KeyboardEvent("keydown", {
              code: "KeyT",
              ctrlKey: !mac,
              metaKey: mac,
              shiftKey: true,
              isComposing: true,
            }),
            defaults,
          ),
        ).toBeUndefined();
      }
      expect(
        keyChord(
          new window.KeyboardEvent("keydown", {
            code: "ShiftLeft",
            shiftKey: true,
          }),
        ),
      ).toBe("");
      expect(
        keyChord(new window.KeyboardEvent("keydown", { code: "F6" })),
      ).toBe("F6");
      expect(chordLabel("Shift+Meta+KeyT")).toBe("Shift+Cmd+T");
      expect(chordLabel("")).toBe("Unassigned");
    });

    test("tab ordering and close neighbors preserve tab objects and handle boundaries", () => {
      const items = [{ id: "a" }, { id: "b" }, { id: "c" }];
      expect(reorderTabs(items, "a", "c", true)).toEqual([
        items[1],
        items[2],
        items[0],
      ]);
      expect(reorderTabs(items, "c", "a", false)).toEqual([
        items[2],
        items[0],
        items[1],
      ]);
      expect(reorderTabs(items, "missing", "a", false)).toBe(items);
      expect(reorderTabs(items, "a", "a", true)).toBe(items);
      expect(adjacentTab(items, "b")).toBe("c");
      expect(adjacentTab(items, "c")).toBe("b");
      expect(adjacentTab([{ id: "a" }], "a")).toBe("library");
      expect(cycleTab(items, "c", 1)).toBe("library");
      expect(cycleTab(items, "library", -1)).toBe("c");
      expect(cycleTab([], "library", 1)).toBe("library");
    });

    test("sidebar counts load before visiting pages and refresh known hosts on entry", async () => {
      workspace.snippets = [
        { id: "status", name: "Status", command: "uptime" },
      ];
      trustedHosts = [
        {
          line: "1",
          host: "alpha.example.com",
          kind: "ssh-ed25519",
          fingerprint: "SHA256:test",
        },
      ];
      await mount();
      const counts = () =>
        [...container.querySelectorAll(".sidebar nav .nav-count")].map(
          (item) => item.textContent,
        );
      expect(container.querySelector("h1")?.textContent).toBe("Hosts");
      expect(counts()).toEqual(["2", "1", "1"]);
      expect(invoke).toHaveBeenCalledWith("known_hosts");
      await click(".sidebar nav button:nth-child(4)");
      expect(container.querySelector("h1")?.textContent).toBe("Snippets");
      expect(counts()).toEqual(["2", "1", "1"]);
      trustedHosts = [];
      await click(".sidebar nav button:nth-child(5)");
      expect(counts()).toEqual(["2", "1"]);
    });

    test("new tab shortcut searches by group and opens a session with Enter", async () => {
      await mount();
      await key("KeyT", primaryShift);
      expect(container.querySelector("dialog[open]")?.textContent).toContain(
        "Open a session",
      );
      await input("dialog input", "Production");
      expect(
        container.querySelectorAll(".host-picker-list button"),
      ).toHaveLength(1);
      await submit();
      expect(selectedTab()).toBe("Alpha");
      expect(
        container.querySelector('[role="tab"][aria-selected="true"] .tab-title')
          ?.tagName,
      ).toBe("SPAN");
      expect(mounts).toEqual(["session-1"]);
    });

    test("tab shortcuts intercept xterm keys, preserve ordinary input, and close adjacent sessions", async () => {
      await mount();
      await openHost();
      await openHost(1);
      await key("Digit1", primaryShift);
      expect(selectedTab()).toBe("Alpha");
      await key("Digit9", primaryShift);
      expect(selectedTab()).toBe("Beta");
      const terminal = container.querySelector(
        '[data-session="session-2"] textarea',
      );
      expect(terminal).not.toBeNull();
      const shell = mock(() => {});
      terminal?.addEventListener("keydown", shell);
      const ordinary = await key("KeyC", { ctrlKey: true }, terminal ?? window);
      expect(ordinary.defaultPrevented).toBe(false);
      expect(shell).toHaveBeenCalledTimes(1);
      const switching = await key(
        "Tab",
        { ctrlKey: true, shiftKey: true },
        terminal ?? window,
      );
      expect(switching.defaultPrevented).toBe(true);
      expect(shell).toHaveBeenCalledTimes(1);
      expect(selectedTab()).toBe("Alpha");
      expect(
        container
          .querySelector('[data-session="session-1"]')
          ?.getAttribute("data-active"),
      ).toBe("true");
      await key("KeyW", primaryShift);
      expect(selectedTab()).toBe("Beta");
      expect(unmounts).toEqual(["session-1"]);
      await key("KeyW", primaryShift);
      expect(selectedTab()).toBe("Library");
      await key("KeyS", primaryShift);
      expect(container.querySelector("h1")?.textContent).toBe("Snippets");
    });

    test("dragging reorders live sessions without reconnecting or changing the active tab", async () => {
      await mount();
      await openHost();
      await openHost(1);
      const tabs = container.querySelectorAll('[role="tab"]');
      const transfer = new browser.DataTransfer();
      const drag = (type: string) => {
        const event = new window.Event(type, {
          bubbles: true,
          cancelable: true,
        });
        Object.defineProperties(event, {
          dataTransfer: { value: transfer },
          clientX: { value: 20 },
        });
        return event;
      };
      await act(async () => {
        tabs[1].dispatchEvent(drag("dragstart"));
        tabs[2].dispatchEvent(drag("dragover"));
        tabs[2].dispatchEvent(drag("drop"));
      });
      expect(
        [...container.querySelectorAll('[role="tab"]')].map((tab) =>
          tab.getAttribute("aria-label"),
        ),
      ).toEqual(["Library", "Beta", "Alpha"]);
      expect(selectedTab()).toBe("Beta");
      expect(mounts).toEqual(["session-1", "session-2"]);
      expect(unmounts).toEqual([]);
      await click('button[aria-label="Close Alpha"]');
      expect(selectedTab()).toBe("Beta");
    });

    test("shortcut recording persists overrides, rejects conflicts, and resets defaults", async () => {
      await mount();
      await click(".sidebar button", "Settings");
      await click("button", "Change shortcut for New tab");
      const recorder = container.querySelector(
        '[aria-label="Change shortcut for New tab"]',
      );
      await key("KeyT", { altKey: true }, recorder ?? window);
      expect(workspace.settings.shortcuts?.newTab).toBe("Alt+KeyT");
      await click("button", "Change shortcut for Close active tab");
      await key(
        "KeyT",
        { altKey: true },
        container.querySelector(
          '[aria-label="Change shortcut for Close active tab"]',
        ) ?? window,
      );
      expect(container.querySelector('[role="alert"]')?.textContent).toContain(
        "Already used for New tab",
      );
      expect(workspace.settings.shortcuts?.closeTab).toBeUndefined();
      await key(
        "Escape",
        { key: "Escape" },
        container.querySelector(
          '[aria-label="Change shortcut for Close active tab"]',
        ) ?? window,
      );
      await key("KeyT", primaryShift);
      expect(container.querySelector("dialog")).toBeNull();
      await key("KeyT", { altKey: true });
      expect(container.querySelector("dialog[open]")).not.toBeNull();
      await click('button[aria-label="Close dialog"]');
      await act(async () => root.unmount());
      root = createRoot(container);
      await mount();
      await key("KeyT", { altKey: true });
      expect(container.querySelector("dialog[open]")).not.toBeNull();
      await click('button[aria-label="Close dialog"]');
      await click(".sidebar button", "Settings");
      await click("button", "Reset shortcuts");
      expect(workspace.settings.shortcuts).toEqual({});
      await key("KeyT", primaryShift);
      expect(container.querySelector("dialog[open]")).not.toBeNull();
    });

    test("unassigning a shortcut disables it and regular editable fields keep their input", async () => {
      await mount();
      await click(".sidebar button", "Settings");
      await click("button", "Change shortcut for New tab");
      const recorder = container.querySelector(
        '[aria-label="Change shortcut for New tab"]',
      );
      await key("Backspace", { key: "Backspace" }, recorder ?? window);
      expect(workspace.settings.shortcuts?.newTab).toBe("");
      expect(recorder?.textContent).toBe("Unassigned");
      await key("KeyT", primaryShift);
      expect(container.querySelector("dialog")).toBeNull();
      const field = container.querySelector("input");
      const event = await key("KeyS", primaryShift, field ?? window);
      expect(event.defaultPrevented).toBe(false);
      expect(container.querySelector("h1")?.textContent).toBe("Settings");
      const editable = document.createElement("div");
      editable.setAttribute("contenteditable", "");
      container.append(editable);
      await key("KeyS", primaryShift, editable);
      expect(container.querySelector("h1")?.textContent).toBe("Settings");
    });

    test("host groups render separately and new hosts inherit the chosen group", async () => {
      await mount();
      expect(container.querySelectorAll(".host-group")).toHaveLength(2);
      await click('button[aria-label="Add host to Production"]');
      expect(
        container.querySelector<HTMLSelectElement>("dialog select")?.value,
      ).toBe("production");
      await key("KeyT", primaryShift);
      expect(container.querySelector("dialog")?.textContent).toContain(
        "Host details",
      );
      await submit();
      expect(
        workspace.hosts.find((host) => host.id === "new-host")?.groupId,
      ).toBe("production");
      await click("button", "New group");
      await input("dialog input", "Staging");
      await submit();
      expect(workspace.groups?.some((group) => group.name === "Staging")).toBe(
        true,
      );
      expect(container.textContent).toContain("No hosts in this group");
      await click('button[aria-label="Rename Production"]');
      await input("dialog input", "Live");
      await submit();
      expect(workspace.hosts.find((host) => host.id === "Alpha")?.groupId).toBe(
        "production",
      );
      await click('button[aria-label="Delete group Live"]');
      expect(container.querySelector("dialog")?.textContent).toContain(
        "Its hosts will remain in Ungrouped",
      );
      await click("dialog button", "Delete");
      expect(workspace.groups?.some((group) => group.id === "production")).toBe(
        false,
      );
      expect(workspace.hosts).toHaveLength(3);
      expect(workspace.hosts.find((host) => host.id === "Alpha")?.groupId).toBe(
        "",
      );
    });
  },
);
