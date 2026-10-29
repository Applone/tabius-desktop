import { describe, expect, test } from "bun:test";
import { stringify } from "yaml";
import { emptyWorkspace, newHost } from "../model";
import type { SecretKey, SyncResponse, SyncServices } from "../sync";
import {
  decryptVault,
  encryptVault,
  merge,
  nativeFingerprints,
  synchronize,
  TabbyDocument,
} from "../sync";
import fixture from "./fixtures/tabby-vault-v1.json";

const profile = {
  id: "ssh:remote",
  name: "Remote",
  address: "remote.example.com",
  port: 22,
  username: "root",
  authentication: "password",
  keyRef: null,
};
function transport(content: string) {
  const writes: string[] = [];
  const removed: string[] = [];
  let current = content;
  let reads = 0;
  let failPatch = false;
  let changeOnSecondRead = false;
  const services: SyncServices = {
    request: async (_id, content, etag) => {
      if (content !== undefined) {
        if (failPatch) throw new Error("HTTP 412");
        expect(etag).toBe("etag-1");
        writes.push(content);
        current = content;
        return { body: null };
      }
      reads++;
      return {
        body: {
          id: 1,
          name: "Test",
          content:
            changeOnSecondRead && reads === 2
              ? "profiles: []\n# concurrent change"
              : current,
          modified_at: "now",
        },
        etag: "etag-1",
      };
    },
    readKeys: async () => [],
    importKey: async (key: SecretKey) => ({
      id: "imported",
      name: key.name,
      kind: "ssh-ed25519",
      publicKey: "ssh-ed25519 test",
      fingerprint: "SHA256:test",
    }),
    deleteKey: async (id) => {
      removed.push(id);
    },
    knownHosts: async () => [],
  };
  return {
    services,
    writes,
    removed,
    get content() {
      return current;
    },
    set failPatch(value: boolean) {
      failPatch = value;
    },
    set changeOnSecondRead(value: boolean) {
      changeOnSecondRead = value;
    },
  };
}
describe("three-way synchronization", () => {
  test("propagates deletions and independent field edits", () => {
    expect(merge([], [profile], [profile], true)).toEqual([]);
    expect(merge([profile], [], [profile], true)).toEqual([]);
    expect(
      merge(
        [{ ...profile, name: "Local" }],
        [{ ...profile, port: 2222 }],
        [profile],
        true,
      ),
    ).toEqual([{ ...profile, name: "Local", port: 2222 }]);
  });
  test("blocks edit/delete, duplicate IDs, and same-field conflicts", () => {
    expect(() =>
      merge([{ ...profile, name: "Changed" }], [], [profile], true),
    ).toThrow("conflict");
    expect(() => merge([profile, profile], [], [])).toThrow("Duplicate");
    expect(() =>
      merge(
        [{ ...profile, name: "Local" }],
        [{ ...profile, name: "Remote edit" }],
        [profile],
        true,
      ),
    ).toThrow("name");
  });
});
describe("Tabby vault compatibility", () => {
  test("decrypts the same independent fixture as iOS", async () => {
    const vault = await decryptVault(fixture, "fixture-password");
    expect(vault.config).toBeDefined();
    expect(vault.secrets).toHaveLength(1);
    await expect(decryptVault(fixture, "wrong")).rejects.toThrow("password");
  });
  test("fresh salts and IVs round-trip secrets and unicode", async () => {
    const vault = {
      config: { profiles: [] },
      secrets: [{ type: "unknown", value: "café" }],
    };
    const a = await encryptVault(vault, "password"),
      b = await encryptVault(vault, "password");
    expect(a.iv).not.toEqual(b.iv);
    expect(a.keySalt).not.toEqual(b.keySalt);
    expect(await decryptVault(a, "password")).toEqual(vault);
  });
  test("rejects unsupported envelopes, duplicate YAML keys, and non-mappings", async () => {
    await expect(
      decryptVault({ ...fixture, version: 2 }, "password"),
    ).rejects.toThrow("version");
    await expect(
      TabbyDocument.open("profiles: []\nprofiles: []"),
    ).rejects.toThrow("YAML");
    await expect(TabbyDocument.open("- one")).rejects.toThrow("Invalid");
  });
  test("preserves unrelated profiles, options, settings, and password secrets", async () => {
    const document = await TabbyDocument.open(
      "appearance: {frame: native}\nprofiles:\n  - {id: local, type: local, options: {command: /bin/zsh}}\n  - {id: ssh:remote, type: ssh, name: Remote, options: {host: remote.example.com, port: 22, user: root, keepaliveInterval: 5000, password: retained}}",
    );
    document.replaceProfiles(
      [{ ...document.profiles()[0], name: "Renamed" }],
      [],
    );
    const serialized = await document.serialize();
    expect(serialized).toContain("retained");
    expect(serialized).toContain("5000");
    expect(serialized).toContain("/bin/zsh");
    expect(serialized).toContain("frame: native");
  });
  test("secrets-only vaults stay encrypted with unencrypted profile YAML", async () => {
    const vault = await encryptVault(
      {
        secrets: [
          {
            type: "ssh:password",
            value: "keep-me",
            key: { host: "example.com" },
          },
        ],
      },
      "pw",
    );
    const document = await TabbyDocument.open(
      stringify({ profiles: [], vault }),
      "pw",
    );
    const serialized = await document.serialize("pw");
    expect(serialized).not.toContain("keep-me");
    const reopened = await TabbyDocument.open(serialized, "pw");
    expect(reopened.vault?.secrets).toEqual([
      { type: "ssh:password", value: "keep-me", key: { host: "example.com" } },
    ]);
  });
});
describe("sync transactions", () => {
  test("pulls remote hosts, pushes local hosts, and repeats without duplication", async () => {
    const remote = transport(
      "profiles: [{id: ssh:remote, type: ssh, name: Remote, options: {host: remote.example.com, user: root}}]",
    );
    const workspace = emptyWorkspace();
    workspace.hosts.push({
      ...newHost(),
      name: "Local",
      address: "local.example.com",
    });
    const result = await synchronize(
      workspace,
      "https://tabby.example.com",
      1,
      "",
      remote.services,
    );
    expect(result.workspace.hosts).toHaveLength(2);
    expect(remote.writes).toHaveLength(1);
    expect(remote.content).toContain("local.example.com");
    const repeated = await synchronize(
      result.workspace,
      "https://tabby.example.com",
      1,
      "",
      remote.services,
    );
    expect(repeated.workspace.hosts.map((h) => h.id)).toEqual(
      result.workspace.hosts.map((h) => h.id),
    );
  });
  test("local host deletion removes the managed remote row, retaining unrelated profiles", async () => {
    const remote = transport(
      "profiles: [{id: ssh:remote, type: ssh, name: Remote, options: {host: remote.example.com, user: root}}, {id: local, type: local}]",
    );
    const first = await synchronize(
      emptyWorkspace(),
      "https://tabby.example.com",
      1,
      "",
      remote.services,
    );
    first.workspace.hosts = [];
    const result = await synchronize(
      first.workspace,
      "https://tabby.example.com",
      1,
      "",
      remote.services,
    );
    expect(result.workspace.hosts).toEqual([]);
    expect(remote.content).not.toContain("remote.example.com");
    expect(remote.content).toContain("type: local");
  });
  test("detects remote and local concurrent changes before PATCH", async () => {
    const remote = transport("profiles: []");
    remote.changeOnSecondRead = true;
    await expect(
      synchronize(
        emptyWorkspace(),
        "https://tabby.example.com",
        1,
        "",
        remote.services,
      ),
    ).rejects.toThrow("server configuration changed");
    expect(remote.writes).toEqual([]);
    const other = transport("profiles: []");
    other.services.ensureUnchanged = () => {
      throw new Error("Local revision changed");
    };
    await expect(
      synchronize(
        emptyWorkspace(),
        "https://tabby.example.com",
        1,
        "",
        other.services,
      ),
    ).rejects.toThrow("revision");
    expect(other.writes).toEqual([]);
  });
  test("encrypted key import preserves assignments and keeps secrets out of persisted metadata", async () => {
    const secret =
      "-----BEGIN OPENSSH PRIVATE KEY-----\nfixture\n-----END OPENSSH PRIVATE KEY-----";
    const vault = await encryptVault(
      {
        config: {
          profiles: [
            {
              id: "remote",
              type: "ssh",
              name: "Server",
              options: {
                host: "server.example.com",
                user: "root",
                auth: "publicKey",
                privateKeys: ["vault://key-one"],
              },
            },
          ],
        },
        secrets: [
          {
            type: "file",
            key: { id: "key-one", description: "Key" },
            value: btoa(secret),
          },
          {
            type: "ssh:password",
            key: { host: "keep.example" },
            value: "retained-password",
          },
        ],
      },
      "pw",
    );
    const remote = transport(stringify({ encrypted: true, vault }));
    const result = await synchronize(
      emptyWorkspace(),
      "https://tabby.example.com",
      1,
      "pw",
      remote.services,
    );
    expect(result.workspace.keys[0].syncId).toBe("file:key-one");
    expect(result.workspace.hosts[0].keyId).toBe("imported");
    expect(JSON.stringify(result.workspace)).not.toContain(secret);
    expect(JSON.stringify(result.workspace)).not.toContain("retained-password");
    const reopened = await TabbyDocument.open(remote.content, "pw");
    expect(JSON.stringify(reopened.vault?.secrets)).toContain(
      "retained-password",
    );
    expect(reopened.profiles()[0].keyRef).toBe("vault://key-one");
  });
  test("cleans staged keys on failed upload and never applies partial local metadata", async () => {
    const vault = await encryptVault(
      {
        config: { profiles: [] },
        secrets: [
          {
            type: "file",
            key: { id: "one" },
            value: btoa("-----BEGIN PRIVATE KEY-----\ntest"),
          },
        ],
      },
      "pw",
    );
    const remote = transport(stringify({ encrypted: true, vault }));
    remote.failPatch = true;
    const workspace = emptyWorkspace();
    await expect(
      synchronize(
        workspace,
        "https://tabby.example.com",
        1,
        "pw",
        remote.services,
      ),
    ).rejects.toThrow("412");
    expect(remote.removed).toEqual(["imported"]);
    expect(workspace.keys).toEqual([]);
    expect(workspace.sync).toBeUndefined();
  });
  test("validates the configuration ID and never uploads unsupported proxy profiles", async () => {
    const remote = transport(
      "profiles: [{id: proxy, type: ssh, options: {host: example.com, proxyCommand: custom}}]",
    );
    await expect(
      synchronize(
        emptyWorkspace(),
        "https://tabby.example.com",
        1,
        "",
        remote.services,
      ),
    ).rejects.toThrow("proxyCommand");
    expect(remote.writes).toEqual([]);
    const services = {
      ...remote.services,
      request: async (): Promise<SyncResponse> => ({
        body: { id: 2, content: "profiles: []" },
      }),
    };
    await expect(
      synchronize(
        emptyWorkspace(),
        "https://tabby.example.com",
        1,
        "",
        services,
      ),
    ).rejects.toThrow("different");
  });
});
test("normalizes OpenSSH fingerprints and ignores hashed host labels", () => {
  const digest = btoa("a".repeat(32));
  const known = nativeFingerprints([
    {
      host: "[EXAMPLE.com]:2222,other.example",
      kind: "ssh-ed25519",
      fingerprint: `SHA256:${digest.replace(/=+$/, "")}`,
      line: "original",
    },
    {
      host: "|1|hashed|name",
      kind: "ssh-ed25519",
      fingerprint: `SHA256:${digest}`,
      line: "hashed",
    },
  ]);
  expect(known).toHaveLength(2);
  expect(known[0]).toEqual({
    id: "example.com|2222|ssh-ed25519",
    host: "example.com",
    port: 2222,
    type: "ssh-ed25519",
    digest,
  });
});

test("a formerly synchronized key becoming unreadable is a conflict, not a deletion", async () => {
  const remote = transport(
    stringify({
      encrypted: true,
      vault: await encryptVault(
        {
          config: { profiles: [] },
          secrets: [
            {
              type: "file",
              key: { id: "one" },
              value: btoa("not a private key"),
            },
          ],
        },
        "pw",
      ),
    }),
  );
  const workspace = emptyWorkspace();
  workspace.sync = {
    server: "https://tabby.example.com",
    configurationId: 1,
    baseline: [],
    keyBaseline: [
      { id: "file:one", name: "Previously imported", digest: "previous" },
    ],
    knownBaseline: [],
    fingerprints: [],
  };
  await expect(
    synchronize(
      workspace,
      "https://tabby.example.com",
      1,
      "pw",
      remote.services,
    ),
  ).rejects.toThrow("unreadable");
  expect(remote.writes).toEqual([]);
});

test("resolves nested Tabby jump references and blocks cycles before upload", async () => {
  const configuration = {
    profiles: [
      {
        id: "outer",
        type: "ssh",
        options: { host: "outer.example", user: "jump" },
      },
      {
        id: "inner",
        type: "ssh",
        options: { host: "inner.example", user: "jump", jumpHost: "outer" },
      },
      {
        id: "target",
        type: "ssh",
        options: { host: "target.example", user: "root", jumpHost: "inner" },
      },
    ],
  };
  const remote = transport(stringify(configuration));
  const result = await synchronize(
    emptyWorkspace(),
    "https://tabby.example.com",
    1,
    "",
    remote.services,
  );
  expect(result.workspace.hosts.find((h) => h.syncId === "target")?.jump).toBe(
    "jump@outer.example:22,jump@inner.example:22",
  );
  configuration.profiles[0].options = {
    ...configuration.profiles[0].options,
    jumpHost: "target",
  };
  const cycle = transport(stringify(configuration));
  await expect(
    synchronize(
      emptyWorkspace(),
      "https://tabby.example.com",
      1,
      "",
      cycle.services,
    ),
  ).rejects.toThrow("chain");
  expect(cycle.writes).toEqual([]);
});
