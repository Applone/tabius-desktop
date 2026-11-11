import { afterAll, afterEach, beforeEach, expect, mock, test } from "bun:test";
import type { Channel } from "@tauri-apps/api/core";
import { Window as HappyWindow } from "happy-dom";
import { act } from "react";
import * as originalApi from "./api";
import type { Settings } from "./model";
import type { Session } from "./Terminal";

const api = { ...originalApi };
const xterm = { ...(await import("@xterm/xterm")) };
const addon = { ...(await import("@xterm/addon-fit")) };
const browser = new HappyWindow();
const previousWindow = globalThis.window;
const previousDocument = globalThis.document;
const previousStyle = globalThis.getComputedStyle;
const previousObserver = globalThis.ResizeObserver;
const previousFrame = globalThis.requestAnimationFrame;
const previousCancelFrame = globalThis.cancelAnimationFrame;
globalThis.window = browser as unknown as typeof window;
globalThis.document = browser.document as unknown as Document;
globalThis.getComputedStyle = browser.getComputedStyle.bind(
  browser,
) as unknown as typeof getComputedStyle;
globalThis.ResizeObserver = browser.ResizeObserver as typeof ResizeObserver;
globalThis.requestAnimationFrame = browser.requestAnimationFrame.bind(
  browser,
) as unknown as typeof requestAnimationFrame;
globalThis.cancelAnimationFrame = browser.cancelAnimationFrame.bind(
  browser,
) as unknown as typeof cancelAnimationFrame;
Reflect.set(globalThis, "IS_REACT_ACT_ENVIRONMENT", true);
Reflect.set(window, "__TAURI_INTERNALS__", { transformCallback: () => 1 });

let terminalInput: ((data: string) => void) | undefined;
class FakeTerminal {
  rows = 24;
  cols = 80;
  options = {};
  parser = { registerOscHandler: () => {} };
  loadAddon() {}
  open() {}
  onData(handler: (data: string) => void) {
    terminalInput = handler;
    return { dispose() {} };
  }
  onBell() {
    return { dispose() {} };
  }
  onResize() {
    return { dispose() {} };
  }
  write() {}
  writeln() {}
  focus() {}
  dispose() {}
}
mock.module("@xterm/xterm", () => ({ Terminal: FakeTerminal }));
mock.module("@xterm/addon-fit", () => ({
  FitAddon: class {
    fit() {}
  },
}));
let output: Channel<unknown>;
let failStart = false;
let failAnswer = false;
let resolveStart: ((id: string) => void) | undefined;
let deferStart = false;
const invoke = mock(async (name: string, args?: Record<string, unknown>) => {
  if (name === "answer_host_key" && failAnswer)
    throw new Error("Confirmation expired");
  if (name !== "start_session") return;
  output = args?.onOutput as typeof output;
  if (failStart) throw new Error("SSH unavailable");
  if (deferStart)
    return new Promise<string>((resolve) => {
      resolveStart = resolve;
    });
  return "session-test";
});
mock.module("./api", () => ({ ...api, native: invoke }));
const { TerminalPane } = await import("./Terminal");
const { createRoot } = await import("react-dom/client");
let root: ReturnType<typeof createRoot>;
let container: HTMLDivElement;
const trustChanged = mock(() => {});
const close = mock(() => {});
const session: Session = {
  id: "session-test",
  host: {
    id: "host",
    name: "Test",
    address: "test.example.com",
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
  },
};
const settings: Settings = {
  autoReconnect: false,
  bell: false,
  optionAsMeta: false,
  emulator: "xterm-256color",
  font: "monospace",
  fontSize: 14,
  lineHeight: 1.2,
  theme: "Graphite",
  highlighting: true,
  detectOS: false,
  postQuantum: true,
};
beforeEach(() => {
  failStart = false;
  terminalInput = undefined;
  failAnswer = false;
  deferStart = false;
  resolveStart = undefined;
  invoke.mockClear();
  trustChanged.mockClear();
  close.mockClear();
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
  mock.module("@xterm/xterm", () => xterm);
  mock.module("@xterm/addon-fit", () => addon);
  globalThis.window = previousWindow;
  globalThis.document = previousDocument;
  globalThis.getComputedStyle = previousStyle;
  globalThis.ResizeObserver = previousObserver;
  globalThis.requestAnimationFrame = previousFrame;
  globalThis.cancelAnimationFrame = previousCancelFrame;
  Reflect.deleteProperty(globalThis, "IS_REACT_ACT_ENVIRONMENT");
  browser.happyDOM.abort();
});
async function mount(active = false) {
  await act(async () =>
    root.render(
      <TerminalPane
        session={session}
        active={active}
        focusRequest={0}
        settings={settings}
        onClose={close}
        onOS={() => {}}
        onTrustChanged={trustChanged}
      />,
    ),
  );
}
const reconnect = () =>
  [...container.querySelectorAll("button")].find((button) =>
    button.textContent?.includes("Reconnect"),
  );
async function emit(message: unknown) {
  await act(async () => output.onmessage(message));
}
test("terminal viewport padding matches each theme instead of xterm's black default", async () => {
  const style = document.createElement("style");
  style.textContent =
    (await Bun.file(
      new URL("../node_modules/@xterm/xterm/css/xterm.css", import.meta.url),
    ).text()) +
    (await Bun.file(new URL("./style.css", import.meta.url)).text());
  document.head.append(style);
  const surface = document.createElement("div");
  const element = document.createElement("div");
  element.className = "xterm";
  const viewport = document.createElement("div");
  viewport.className = "xterm-viewport";
  element.append(viewport);
  surface.append(element);
  container.append(surface);
  try {
    for (const theme of ["graphite", "midnight", "paper"]) {
      surface.className = `terminal-surface ${theme}`;
      const background = getComputedStyle(surface).backgroundColor;
      expect(background).not.toBe("");
      expect(background).not.toBe("rgb(0, 0, 0)");
      expect(getComputedStyle(viewport).backgroundColor).toBe(background);
    }
  } finally {
    style.remove();
  }
});
test("terminal fitting leaves room for padding around every row and column", async () => {
  const style = document.createElement("style");
  style.textContent = await Bun.file(
    new URL("./style.css", import.meta.url),
  ).text();
  document.head.append(style);
  const surface = document.createElement("div");
  surface.className = "terminal-surface";
  surface.style.width = "800px";
  surface.style.height = "400px";
  const element = document.createElement("div");
  element.className = "xterm";
  surface.append(element);
  container.append(surface);
  const fit = new addon.FitAddon();
  fit.activate({
    element,
    options: { scrollback: 0 },
    _core: {
      _renderService: {
        dimensions: { css: { cell: { width: 8, height: 20 } } },
      },
    },
  } as unknown as Parameters<typeof fit.activate>[0]);
  try {
    const dimensions = fit.proposeDimensions();
    expect(dimensions).toEqual({ cols: 96, rows: 18 });
    const parentStyle = getComputedStyle(surface);
    const terminalStyle = getComputedStyle(element);
    const padding = (value: string) => Number.parseFloat(value) || 0;
    const availableHeight =
      400 -
      padding(parentStyle.paddingTop) -
      padding(parentStyle.paddingBottom);
    const availableWidth =
      800 -
      padding(parentStyle.paddingLeft) -
      padding(parentStyle.paddingRight);
    expect(
      (dimensions?.rows ?? 0) * 20 +
        padding(terminalStyle.paddingTop) +
        padding(terminalStyle.paddingBottom),
    ).toBeLessThanOrEqual(availableHeight);
    expect(
      (dimensions?.cols ?? 0) * 8 +
        padding(terminalStyle.paddingLeft) +
        padding(terminalStyle.paddingRight),
    ).toBeLessThanOrEqual(availableWidth);
  } finally {
    fit.dispose();
    style.remove();
  }
});
test("reconnect is hidden while connecting, running, and after a clean logout", async () => {
  deferStart = true;
  await mount();
  expect(reconnect()).toBeUndefined();
  await act(async () => resolveStart?.("session-test"));
  expect(container.textContent).toContain("Running");
  expect(reconnect()).toBeUndefined();
  await emit({ type: "exit", code: 0 });
  expect(container.textContent).toContain("Disconnected");
  expect(reconnect()).toBeUndefined();
});
test("SSH errors offer reconnect and retry hides it until another failure", async () => {
  await mount();
  await emit({ type: "exit", code: 255 });
  expect(reconnect()).toBeDefined();
  await act(async () => reconnect()?.click());
  expect(reconnect()).toBeUndefined();
  expect(
    invoke.mock.calls.filter(([name]) => name === "start_session"),
  ).toHaveLength(2);
  await emit({ type: "status", status: "Reconnecting" });
  expect(reconnect()).toBeUndefined();
  await emit({ type: "exit", code: -1 });
  expect(reconnect()).toBeDefined();
});
test("start failures offer reconnect", async () => {
  failStart = true;
  await mount();
  expect(container.textContent).toContain("SSH unavailable");
  expect(reconnect()).toBeDefined();
});
test("an early exit is not overwritten by the start acknowledgement", async () => {
  deferStart = true;
  await mount();
  await emit({ type: "exit", code: 255 });
  await act(async () => resolveStart?.("session-test"));
  expect(container.textContent).toContain("Disconnected");
  expect(reconnect()).toBeDefined();
});
test("trust notifications refresh host fingerprints and late messages are ignored", async () => {
  await mount();
  await emit({ type: "data", data: [65] });
  expect(trustChanged).not.toHaveBeenCalled();
  await emit({ type: "trustchanged" });
  expect(trustChanged).toHaveBeenCalledTimes(1);
  await act(async () => root.render(null));
  await emit({ type: "trustchanged" });
  expect(trustChanged).toHaveBeenCalledTimes(1);
});

const hostKey = {
  id: "request-1",
  host: "[test.example.com]:2222",
  kind: "ssh-ed25519",
  fingerprint: "SHA256:server-key",
};
async function clickAction(label: string) {
  const button = [
    ...container.querySelectorAll<HTMLButtonElement>("dialog button"),
  ].find((button) => button.textContent?.trim() === label);
  expect(button).toBeDefined();
  await act(async () => button?.click());
}
test.each([
  ["Continue once", "once"],
  ["Trust and continue", "save"],
])(
  "host verification action %s sends only the pending request decision",
  async (label, decision) => {
    await mount(true);
    await emit({ type: "hostkey", request: hostKey });
    const dialog = container.querySelector("dialog[open]");
    expect(dialog?.textContent).toContain(hostKey.host);
    expect(dialog?.textContent).toContain(hostKey.kind);
    expect(dialog?.textContent).toContain(hostKey.fingerprint);
    await clickAction(label);
    expect(invoke).toHaveBeenCalledWith("answer_host_key", {
      id: session.id,
      requestId: hostKey.id,
      decision,
    });
    expect(container.querySelector("dialog")).toBeNull();
    expect(trustChanged).not.toHaveBeenCalled();
    expect(close).not.toHaveBeenCalled();
  },
);
test("aborting or dismissing host verification closes the login", async () => {
  await mount(true);
  await emit({ type: "hostkey", request: hostKey });
  await clickAction("Abort login");
  expect(invoke).toHaveBeenCalledWith("answer_host_key", {
    id: session.id,
    requestId: hostKey.id,
    decision: "abort",
  });
  expect(close).toHaveBeenCalledTimes(1);
});
test("Escape aborts verification and connection exit removes pending dialogs", async () => {
  await mount(true);
  await emit({ type: "hostkey", request: hostKey });
  await act(async () =>
    container.querySelector("dialog")?.dispatchEvent(
      new window.KeyboardEvent("keydown", {
        key: "Escape",
        bubbles: true,
        cancelable: true,
      }),
    ),
  );
  expect(close).toHaveBeenCalledTimes(1);
  await emit({ type: "exit", code: 255 });
  expect(container.querySelector("dialog")).toBeNull();
});
test("failed approval stays visible with an error and can still be aborted", async () => {
  failAnswer = true;
  await mount(true);
  await emit({ type: "hostkey", request: hostKey });
  await clickAction("Trust and continue");
  expect(container.querySelector('[role="alert"]')?.textContent).toContain(
    "Confirmation expired",
  );
  expect(container.querySelector("dialog[open]")).not.toBeNull();
  await clickAction("Abort login");
  expect(close).toHaveBeenCalledTimes(1);
});
test("background verification waits for its tab and start acknowledgement does not hide it", async () => {
  deferStart = true;
  await mount();
  await emit({ type: "hostkey", request: hostKey });
  expect(container.querySelector("dialog")).toBeNull();
  await act(async () => resolveStart?.(session.id));
  expect(container.textContent).toContain("Awaiting host verification");
  await mount(true);
  expect(container.querySelector("dialog[open]")?.textContent).toContain(
    hostKey.fingerprint,
  );
});

test("terminal input pauses during verification and resumes after approval", async () => {
  await mount(true);
  await emit({ type: "hostkey", request: hostKey });
  await act(async () => terminalInput?.("yes\r"));
  expect(
    invoke.mock.calls.filter(([name]) => name === "write_session"),
  ).toHaveLength(0);
  await clickAction("Continue once");
  await act(async () => terminalInput?.("uptime\r"));
  expect(invoke).toHaveBeenCalledWith("write_session", {
    id: "session-test",
    data: "uptime\r",
  });
});
