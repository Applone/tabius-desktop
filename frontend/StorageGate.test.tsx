import { afterAll, afterEach, beforeEach, expect, mock, test } from "bun:test";
import { Window } from "happy-dom";
import { act } from "react";
import * as originalApi from "./api";

const browser = new Window();
const originalWindow = globalThis.window;
const originalDocument = globalThis.document;
globalThis.window = browser as unknown as typeof originalWindow;
globalThis.document = browser.document as unknown as Document;
Reflect.set(globalThis, "IS_REACT_ACT_ENVIRONMENT", true);

const api = { ...originalApi };
const invoke = mock(
  async (_name: string, _args?: Record<string, unknown>): Promise<unknown> => {
    throw new Error("Unexpected command");
  },
);
mock.module("./api", () => ({ ...api, native: invoke }));
const { createRoot } = await import("react-dom/client");
const { StorageGate } = await import("./StorageGate");
let container: HTMLDivElement;
let root: ReturnType<typeof createRoot>;

beforeEach(() => {
  invoke.mockReset();
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
  globalThis.window = originalWindow;
  globalThis.document = originalDocument;
  Reflect.deleteProperty(globalThis, "IS_REACT_ACT_ENVIRONMENT");
  browser.happyDOM.abort();
});

async function mount() {
  await act(async () =>
    root.render(
      <StorageGate>
        <div>Workspace open</div>
      </StorageGate>,
    ),
  );
}
async function passwords(password: string, confirmation = password) {
  const inputs = container.querySelectorAll("input");
  for (const [index, value] of [password, confirmation].entries()) {
    const input = inputs[index];
    if (!input) continue;
    await act(async () => {
      // Use the native setter so React observes the user edit.
      Object.getOwnPropertyDescriptor(
        browser.HTMLInputElement.prototype,
        "value",
      )?.set?.call(input, value);
      input.dispatchEvent(new window.Event("input", { bubbles: true }));
    });
  }
}
async function submit() {
  const form = container.querySelector("form");
  expect(form).not.toBeNull();
  await act(async () => {
    form?.dispatchEvent(
      new window.Event("submit", { bubbles: true, cancelable: true }),
    );
  });
}
async function click(text: string) {
  const button = [...container.querySelectorAll("button")].find(
    (button) => button.textContent === text,
  );
  expect(button).toBeDefined();
  await act(async () => button?.click());
}

test("first launch confirms the password and opens the workspace only after encryption", async () => {
  invoke.mockImplementation(async (name) => {
    if (name === "storage_status")
      return { state: "setup", keyringUnavailable: false };
    return { state: "ready", keyringUnavailable: false };
  });
  await mount();
  expect(container.textContent).toContain("Protect your local storage");
  expect(container.textContent).not.toContain("Workspace open");
  await passwords("password", "different");
  await submit();
  expect(container.textContent).toContain("The passwords do not match");
  expect(invoke).toHaveBeenCalledTimes(1);
  await passwords("password");
  await submit();
  expect(invoke).toHaveBeenLastCalledWith("configure_storage", {
    mode: "system",
    password: "password",
  });
  expect(container.textContent).toContain("Workspace open");
  expect(container.querySelector("input")).toBeNull();
});

test("unavailable secret storage offers manual encryption and preserves the entered password", async () => {
  invoke.mockImplementation(async (name, args) => {
    if (name === "storage_status")
      return { state: "setup", keyringUnavailable: false };
    return {
      state: args?.mode === "system" ? "setup" : "ready",
      keyringUnavailable: args?.mode === "system",
    };
  });
  await mount();
  await passwords("password");
  await submit();
  expect(container.textContent).toContain("Continue without encryption");
  expect(container.textContent).not.toContain("Workspace open");
  await click("Ask for password on every launch");
  expect(invoke).toHaveBeenLastCalledWith("configure_storage", {
    mode: "manual",
    password: "password",
  });
  expect(container.textContent).toContain("Workspace open");
});

test("no encryption is an explicit fallback choice without sending the password", async () => {
  invoke.mockImplementation(async (name) => ({
    state: name === "storage_status" ? "setup" : "ready",
    keyringUnavailable: true,
  }));
  await mount();
  await passwords("unused password");
  await click("Continue without encryption");
  expect(invoke).toHaveBeenLastCalledWith("configure_storage", {
    mode: "plain",
    password: "",
  });
  expect(container.textContent).toContain("Workspace open");
});

test("wrong passwords keep the workspace locked and never offer a plaintext bypass", async () => {
  invoke.mockImplementation(async (name, args) => {
    if (name === "storage_status")
      return { state: "locked", keyringUnavailable: true };
    if (args?.password !== "correct") throw new Error("Incorrect password");
  });
  await mount();
  expect(container.textContent).toContain("Unlock Tabius");
  expect(container.textContent).not.toContain("Continue without encryption");
  await passwords("wrong");
  await submit();
  expect(container.querySelector('[role="alert"]')?.textContent).toContain(
    "Incorrect password",
  );
  expect(container.textContent).not.toContain("Workspace open");
  await passwords("correct");
  await submit();
  expect(invoke).toHaveBeenLastCalledWith("unlock_storage", {
    password: "correct",
  });
  expect(container.textContent).toContain("Workspace open");
});

test("automatic unlock opens the workspace without showing a password form", async () => {
  invoke.mockResolvedValue({ state: "ready", keyringUnavailable: false });
  await mount();
  expect(container.textContent).toContain("Workspace open");
  expect(container.querySelector("form")).toBeNull();
});

test("storage errors keep the workspace closed and allow retry", async () => {
  invoke.mockRejectedValue(new Error("Damaged local storage configuration"));
  await mount();
  expect(container.textContent).not.toContain("Workspace open");
  expect(container.querySelector('[role="alert"]')?.textContent).toContain(
    "Damaged local storage",
  );
  invoke.mockResolvedValue({ state: "ready", keyringUnavailable: false });
  await click("Retry");
  expect(container.textContent).toContain("Workspace open");
});
