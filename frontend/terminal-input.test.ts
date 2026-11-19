import { expect, test } from "bun:test";
import { bannerSystem, terminalControl } from "./terminal-input";

test("physical control mapping preserves text, IME, and AltGraph", () => {
  const event = {
    code: "KeyF",
    ctrlKey: true,
    shiftKey: false,
    altKey: false,
    metaKey: false,
    isComposing: false,
    getModifierState: () => false,
  } as unknown as KeyboardEvent;
  expect(terminalControl(event)).toBe("\x06");
  expect(terminalControl({ ...event, ctrlKey: false, altKey: true })).toBe(
    "\x1bf",
  );
  expect(terminalControl({ ...event, ctrlKey: false })).toBeUndefined();
  expect(terminalControl({ ...event, isComposing: true })).toBeUndefined();
  expect(
    terminalControl({ ...event, getModifierState: () => true }),
  ).toBeUndefined();
});

test("banner detection requires recognizable OS names", () => {
  expect(bannerSystem("Welcome to Ubuntu 24.04 LTS")).toBe("Ubuntu");
  expect(bannerSystem("SSH-2.0-OpenSSH_9.9")).toBeUndefined();
  expect(bannerSystem("Darwin host 24.0")).toBe("macOS");
});
