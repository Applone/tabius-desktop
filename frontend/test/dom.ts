import { Window } from "happy-dom";
import { tick } from "svelte";

export const browser = new Window();
Object.assign(globalThis, {
  window: browser,
  document: browser.document,
  Element: browser.Element,
  HTMLElement: browser.HTMLElement,
  HTMLMediaElement: browser.HTMLMediaElement,
  Node: browser.Node,
  Text: browser.Text,
  Comment: browser.Comment,
  Event: browser.Event,
  customElements: browser.customElements,
  MutationObserver: browser.MutationObserver,
});

export async function settle(interaction?: () => unknown) {
  interaction?.();
  // Allow mocked Tauri promises and the effects they schedule to finish.
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
  await tick();
}
