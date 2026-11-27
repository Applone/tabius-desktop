import { describe, expect, test } from "bun:test";
import { flushSync, mount, unmount } from "svelte";
import EditorModal from "./EditorModal.svelte";
import type { Host, Workspace } from "./model";
import { hostChain } from "./model";

function host(overrides: Partial<Host> = {}): Host {
  return {
    id: "target",
    name: "Target",
    address: "target.example.com",
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
    ...overrides,
  };
}

function chainSelect(current: Host, hosts: Host[]) {
  const workspace: Workspace = {
    hosts,
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
  const target = document.createElement("div");
  document.body.append(target);
  const component = mount(EditorModal, {
    target,
    props: {
      editor: { type: "host", value: current },
      workspace,
      onClose: () => {},
      onSave: () => {},
      onKnown: () => {},
    },
  });
  flushSync();
  const label = [...target.querySelectorAll("label")].find(
    (label) => label.textContent === "Host chaining",
  );
  const select = target.querySelector<HTMLSelectElement>(`#${label?.htmlFor}`);
  const result = {
    value: select?.value,
    options: Array.from(select?.options ?? [], (option) => ({
      value: option.value,
      label: option.textContent?.trim(),
    })),
  };
  void unmount(component);
  target.remove();
  return result;
}

describe("host chaining", () => {
  test("formats saved endpoints including IPv6 and optional usernames", () => {
    expect(hostChain(host())).toBe("target.example.com:22");
    expect(
      hostChain(
        host({ address: "2001:db8::1", username: "alice", port: 2222 }),
      ),
    ).toBe("alice@[2001:db8::1]:2222");
    expect(hostChain(host({ address: "::1" }))).toBe("[::1]:22");
  });

  test("preserves a selected host's proxy and preceding hops in route order", () => {
    expect(
      hostChain(host({ proxy: "proxy:22", jump: "first:22,second:2200" })),
    ).toBe("proxy:22,first:22,second:2200,target.example.com:22");
  });

  test("offers saved hosts by name and excludes the host being edited", () => {
    const current = host();
    const bastion = host({
      id: "bastion",
      name: "Bastion",
      address: "bastion.example.com",
      username: "alice",
      port: 2222,
    });
    const select = chainSelect(current, [current, bastion]);
    expect(select.value).toBe("");
    expect(select.options).toEqual([
      { value: "", label: "None" },
      {
        value: "alice@bastion.example.com:2222",
        label: "Bastion (bastion.example.com:2222)",
      },
    ]);
  });

  test("shows a saved route as the selected host", () => {
    const bastion = host({ id: "bastion", name: "Bastion" });
    const current = host({ jump: hostChain(bastion) });
    const select = chainSelect(current, [current, bastion]);
    expect(select.value).toBe("target.example.com:22");
    expect(select.options).toHaveLength(2);
    expect(select.options[1].label).toBe("Bastion (target.example.com:22)");
  });

  test("preserves imported routes even when no saved hosts match", () => {
    const current = host({ jump: "alice@old:22,bob@[::1]:2200" });
    const select = chainSelect(current, [current]);
    expect(select.value).toBe(current.jump);
    expect(select.options).toEqual([
      { value: "", label: "None" },
      { value: current.jump, label: `Existing route (${current.jump})` },
    ]);
  });

  test("offers None when there are no other saved hosts", () => {
    const current = host();
    for (const hosts of [[current], []]) {
      expect(chainSelect(current, hosts)).toEqual({
        value: "",
        options: [{ value: "", label: "None" }],
      });
    }
  });
});
