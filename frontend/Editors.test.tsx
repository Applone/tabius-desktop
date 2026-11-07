import { describe, expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";
import { EditorModal } from "./Editors";
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

function chainSelect(current: Host, hosts: Host[]): string {
  const workspace: Workspace = {
    hosts,
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
  const markup = renderToStaticMarkup(
    <EditorModal
      editor={{ type: "host", value: current }}
      workspace={workspace}
      onClose={() => {}}
      onSave={() => {}}
      onKnown={() => {}}
    />,
  );
  const field = markup.split(">Host chaining</label>")[1];
  const select = field?.match(/<select\b[^>]*>[\s\S]*?<\/select>/)?.[0];
  expect(select).toBeDefined();
  return select ?? "";
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
    expect(select).toContain('<option value="" selected="">None</option>');
    expect(select).toContain(
      '<option value="alice@bastion.example.com:2222">Bastion (bastion.example.com:2222)</option>',
    );
    expect(select).not.toContain("Target (");
    expect(select).not.toContain("Existing route");
  });

  test("shows a saved route as the selected host", () => {
    const bastion = host({ id: "bastion", name: "Bastion" });
    const current = host({ jump: hostChain(bastion) });
    const select = chainSelect(current, [current, bastion]);
    expect(select).toContain(
      '<option value="target.example.com:22" selected="">Bastion',
    );
    expect(select).not.toContain("Existing route");
  });

  test("preserves imported routes even when no saved hosts match", () => {
    const current = host({ jump: "alice@old:22,bob@[::1]:2200" });
    const select = chainSelect(current, [current]);
    expect(select).toContain(
      '<option value="alice@old:22,bob@[::1]:2200" selected="">Existing route',
    );
    expect(select).toContain('<option value="">None</option>');
  });

  test("offers None when there are no other saved hosts", () => {
    const current = host();
    expect(chainSelect(current, [current])).toContain(
      '<option value="" selected="">None</option>',
    );
    expect(chainSelect(current, [])).not.toContain("Existing route");
  });
});
