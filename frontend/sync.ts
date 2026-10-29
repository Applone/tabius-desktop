import { parseDocument, stringify } from "yaml";
import type { Host, KeyEntry, KnownHost, Workspace } from "./model";
import { newHost, validateHost } from "./model";

type Row = Record<string, unknown>;
export interface SyncProfile {
  id: string;
  name: string;
  address: string;
  port: number;
  username: string;
  authentication: string;
  keyRef: string | null;
}
export interface KeySummary {
  id: string;
  name: string;
  digest: string;
}
export interface SyncFingerprint {
  id: string;
  host: string;
  port: number;
  type: string;
  digest: string;
}
export interface SyncState {
  server: string;
  configurationId: number;
  baseline: SyncProfile[];
  keyBaseline: KeySummary[];
  knownBaseline: SyncFingerprint[];
  fingerprints: SyncFingerprint[];
  lastSyncedAt?: string;
}
export interface RemoteConfiguration {
  id: number;
  name: string;
  content?: string;
  modified_at?: string;
}
export interface SyncResponse {
  body: unknown;
  etag?: string | null;
}
export interface SecretKey {
  id: string;
  name: string;
  secret: string;
  publicKey: string;
  passphrase?: string;
}
export interface SyncServices {
  ensureUnchanged?: () => void;
  request: (
    id?: number,
    content?: string,
    etag?: string | null,
  ) => Promise<SyncResponse>;
  readKeys: (
    keys: KeyEntry[],
  ) => Promise<{ id: string; secret: string; publicKey: string }[]>;
  importKey: (key: SecretKey) => Promise<KeyEntry>;
  deleteKey: (id: string) => Promise<void>;
  knownHosts: () => Promise<KnownHost[]>;
}
const LIMIT = 8 * 1024 * 1024;
const encoder = new TextEncoder();
function row(value: unknown, label = "configuration"): Row {
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw new Error(`Invalid Tabby ${label}.`);
  return value as Row;
}
function rows(value: unknown, label: string): Row[] {
  if (value === undefined) return [];
  if (!Array.isArray(value)) throw new Error(`Invalid Tabby ${label} list.`);
  return value.map((value) => row(value, label));
}
function text(value: unknown, fallback = ""): string {
  return typeof value === "string" ? value : fallback;
}
function equal(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (!a || !b || typeof a !== "object" || typeof b !== "object") return false;
  const left = Object.keys(a).sort(),
    right = Object.keys(b).sort();
  return (
    equalKeys(left, right) &&
    left.every((key) => equal((a as Row)[key], (b as Row)[key]))
  );
}
function equalKeys(a: string[], b: string[]) {
  return a.length === b.length && a.every((key, i) => key === b[i]);
}
export function merge<T extends { id: string }>(
  local: T[],
  remote: T[],
  baseline: T[],
  fields = false,
): T[] {
  for (const list of [local, remote, baseline])
    if (new Set(list.map((x) => x.id)).size !== list.length)
      throw new Error("Duplicate sync IDs.");
  const l = new Map(local.map((x) => [x.id, x])),
    r = new Map(remote.map((x) => [x.id, x])),
    b = new Map(baseline.map((x) => [x.id, x]));
  const result: T[] = [];
  for (const id of new Set([...r.keys(), ...l.keys(), ...b.keys()])) {
    const a = l.get(id),
      c = r.get(id),
      d = b.get(id);
    let value: T | undefined;
    if (equal(a, c) || equal(c, d)) value = a;
    else if (equal(a, d)) value = c;
    else if (fields && a && c && d) {
      const merged = { ...c };
      for (const key of Object.keys(c) as (keyof T)[]) {
        if (equal(a[key], c[key]) || equal(c[key], d[key]))
          merged[key] = a[key];
        else if (equal(a[key], d[key])) merged[key] = c[key];
        else
          throw new Error(
            `Sync conflict in ${id}: both copies changed ${String(key)}. Nothing was uploaded.`,
          );
      }
      value = merged;
    } else
      throw new Error(
        `Sync conflict in ${id}: both copies changed. Nothing was uploaded.`,
      );
    if (value) result.push(value);
  }
  return result;
}
function bytes64(value: string): Uint8Array<ArrayBuffer> {
  try {
    return Uint8Array.from(atob(value), (c) => c.charCodeAt(0));
  } catch {
    throw new Error("Invalid base64 in the Tabby vault.");
  }
}
function base64(value: Uint8Array): string {
  let result = "";
  for (let offset = 0; offset < value.length; offset += 8192)
    result += String.fromCharCode(...value.subarray(offset, offset + 8192));
  return btoa(result);
}
function hex(value: Uint8Array) {
  return Array.from(value, (v) => v.toString(16).padStart(2, "0")).join("");
}
function fromHex(value: string, size: number) {
  if (!new RegExp(`^[0-9a-fA-F]{${size * 2}}$`).test(value))
    throw new Error("Invalid Tabby vault format.");
  return Uint8Array.from(value.match(/../g) ?? [], (v) => parseInt(v, 16));
}
async function derive(password: string, salt: Uint8Array<ArrayBuffer>) {
  const material = await crypto.subtle.importKey(
    "raw",
    encoder.encode(password),
    "PBKDF2",
    false,
    ["deriveKey"],
  );
  return crypto.subtle.deriveKey(
    { name: "PBKDF2", salt, iterations: 100000, hash: "SHA-512" },
    material,
    { name: "AES-CBC", length: 256 },
    false,
    ["encrypt", "decrypt"],
  );
}
export async function decryptVault(
  envelope: Row,
  password: string,
): Promise<Row> {
  if (!password) throw new Error("Enter the Tabby vault password.");
  if (envelope.version !== 1)
    throw new Error("Unsupported Tabby vault version.");
  const salt = fromHex(text(envelope.keySalt), 8),
    iv = fromHex(text(envelope.iv), 16),
    ciphertext = bytes64(text(envelope.contents));
  if (!ciphertext.length || ciphertext.length > LIMIT || ciphertext.length % 16)
    throw new Error("Invalid Tabby vault content.");
  try {
    const key = await derive(password, salt);
    const plain = await crypto.subtle.decrypt(
      { name: "AES-CBC", iv },
      key,
      ciphertext,
    );
    return row(
      JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(plain)),
      "vault",
    );
  } catch {
    throw new Error(
      "The Tabby vault password is incorrect or the vault is damaged.",
    );
  }
}
export async function encryptVault(vault: Row, password: string): Promise<Row> {
  const salt = crypto.getRandomValues(new Uint8Array(8)),
    iv = crypto.getRandomValues(new Uint8Array(16));
  const key = await derive(password, salt);
  const plain = encoder.encode(JSON.stringify(vault));
  if (plain.length > LIMIT) throw new Error("Tabby vault exceeds 8 MB.");
  const encrypted = await crypto.subtle.encrypt(
    { name: "AES-CBC", iv },
    key,
    plain,
  );
  return {
    version: 1,
    contents: base64(new Uint8Array(encrypted)),
    keySalt: hex(salt),
    iv: hex(iv),
  };
}
export class TabbyDocument {
  root: Row;
  config: Row;
  vault?: Row;
  private constructor(root: Row, vault?: Row) {
    this.root = root;
    this.vault = vault;
    this.config = vault ? row(vault.config) : root;
  }
  static async open(content: string, password = "") {
    if (encoder.encode(content).length > LIMIT)
      throw new Error("Tabby configuration exceeds 8 MB.");
    const document = parseDocument(content, { uniqueKeys: true });
    if (document.errors.length)
      throw new Error("Invalid Tabby YAML configuration.");
    const root = row(document.toJS({ maxAliasCount: 50 }) ?? {});
    const encrypted = root.encrypted === true;
    // Tabby can encrypt only its secrets without encrypting the entire config.
    const vault = root.vault
      ? await decryptVault(row(root.vault, "vault"), password)
      : undefined;
    const result = new TabbyDocument(root, encrypted ? vault : undefined);
    if (encrypted && !vault)
      throw new Error("Encrypted Tabby configuration has no vault.");
    if (!encrypted && vault) result.vault = vault;
    return result;
  }
  profiles(): SyncProfile[] {
    const result = rows(this.config.profiles, "profiles")
      .filter((p) => p.type === "ssh" && p.isTemplate !== true)
      .map((p) => {
        const options = row(p.options ?? {}, "profile options");
        const address = text(options.host);
        if (!address) return null;
        const id = text(p.id);
        if (!id) throw new Error("A Tabby SSH profile has no ID.");
        const refs =
          options.privateKeys === undefined ? [] : options.privateKeys;
        if (!Array.isArray(refs) || refs.some((r) => typeof r !== "string"))
          throw new Error("Invalid private key references.");
        for (const key of ["proxyCommand", "httpProxyHost", "socksProxyHost"])
          if (options[key])
            throw new Error(
              `${text(p.name, address)} uses an unsupported ${key}. Nothing was uploaded.`,
            );
        const keyRef = typeof refs[0] === "string" ? refs[0] : null;
        const profile = {
          id,
          name: text(p.name, address),
          address,
          port: options.port === undefined ? 22 : Number(options.port),
          username: text(options.user, "root"),
          authentication: text(options.auth, keyRef ? "publicKey" : "password"),
          keyRef,
        };
        const invalid = validateHost({ ...newHost(), ...profile });
        if (invalid) throw new Error(`${profile.name}: ${invalid}`);
        return profile;
      })
      .filter((p): p is SyncProfile => p !== null);
    if (new Set(result.map((x) => x.id)).size !== result.length)
      throw new Error("Duplicate remote profile IDs.");
    return result;
  }
  keys(): SecretKey[] {
    if (!this.vault) return [];
    const secrets = rows(this.vault.secrets, "secrets");
    const result: SecretKey[] = [];
    for (const item of secrets.filter((x) => x.type === "file")) {
      const key = row(item.key, "file key");
      const id = text(key.id);
      if (!id) throw new Error("A vault file has no ID.");
      const data = bytes64(text(item.value));
      if (data.length > 128 * 1024)
        throw new Error("A vault key exceeds 128 KB.");
      let secret: string;
      try {
        secret = new TextDecoder("utf-8", { fatal: true }).decode(data);
      } catch {
        continue;
      }
      if (!secret.includes("PRIVATE KEY")) continue; // Preserve non-key vault files.
      const metadata = item.tabiusDesktop ? row(item.tabiusDesktop) : {};
      result.push({
        id: `file:${id}`,
        name: text(key.description, "SSH key"),
        secret,
        publicKey: text(metadata.publicKey),
      });
    }
    return result;
  }
  async keyPassphrase(secret: string): Promise<string> {
    const hash = hex(
      new Uint8Array(
        await crypto.subtle.digest("SHA-512", encoder.encode(secret)),
      ),
    );
    return text(
      rows(this.vault?.secrets, "secrets").find(
        (x) =>
          x.type === "ssh:key-passphrase" && row(x.key ?? {}).hash === hash,
      )?.value,
    );
  }
  fingerprints(): SyncFingerprint[] {
    return rows(row(this.config.ssh ?? {}).knownHosts, "known hosts").map(
      (entry) => {
        const host = text(entry.host).toLowerCase(),
          port = Number(entry.port ?? 22),
          type = text(entry.type),
          digest = text(entry.digest);
        if (
          validateHost({ ...newHost(), name: host, address: host, port }) ||
          !type ||
          bytes64(digest).length !== 32
        )
          throw new Error("Invalid Tabby host fingerprint.");
        return {
          id: `${host}|${port}|${type}`,
          host,
          port,
          type,
          digest: base64(bytes64(digest)),
        };
      },
    );
  }
  replaceProfiles(profiles: SyncProfile[], baseline: SyncProfile[]) {
    const original = rows(this.config.profiles, "profiles");
    const managed = new Set([...profiles, ...baseline].map((p) => p.id));
    const kept = original.filter(
      (p) =>
        p.type !== "ssh" || p.isTemplate === true || !managed.has(text(p.id)),
    );
    for (const profile of profiles) {
      const previous = original.find((p) => p.id === profile.id) ?? {};
      const options = { ...row(previous.options ?? {}) };
      Object.assign(options, {
        host: profile.address,
        port: profile.port,
        user: profile.username,
        auth: profile.authentication,
      });
      const previousRef =
        (Array.isArray(options.privateKeys) ? options.privateKeys : [])[0] ??
        null;
      if (previousRef !== profile.keyRef) {
        if (profile.keyRef) options.privateKeys = [profile.keyRef];
        else delete options.privateKeys;
      }
      kept.push({
        ...previous,
        id: profile.id,
        name: profile.name,
        type: "ssh",
        options,
      });
    }
    this.config.profiles = kept;
  }
  replaceKeys(keys: SecretKey[], baseline: KeySummary[]) {
    if (!this.vault) return;
    const original = rows(this.vault.secrets, "secrets");
    const managed = new Set([...keys, ...baseline].map((k) => k.id));
    const kept = original.filter(
      (entry) =>
        entry.type !== "file" ||
        !managed.has(`file:${text(row(entry.key ?? {}).id)}`),
    );
    for (const key of keys) {
      const previous =
        original.find(
          (x) =>
            x.type === "file" && text(row(x.key ?? {}).id) === key.id.slice(5),
        ) ?? {};
      kept.push({
        ...previous,
        type: "file",
        key: {
          ...row(previous.key ?? {}),
          id: key.id.slice(5),
          description: key.name,
        },
        value: base64(encoder.encode(key.secret)),
        tabiusDesktop: {
          ...row(previous.tabiusDesktop ?? {}),
          publicKey: key.publicKey,
        },
      });
    }
    this.vault.secrets = kept;
  }
  replaceFingerprints(fingerprints: SyncFingerprint[]) {
    this.config.ssh = {
      ...row(this.config.ssh ?? {}),
      knownHosts: fingerprints.map(({ host, port, type, digest }) => ({
        host,
        port,
        type,
        digest,
      })),
    };
  }
  async serialize(password = "") {
    if (this.vault) {
      if (this.root.encrypted === true) this.vault.config = this.config;
      this.root.vault = await encryptVault(this.vault, password);
    }
    const content = stringify(
      this.root.encrypted === true
        ? this.root
        : { ...this.config, ...(this.vault ? { vault: this.root.vault } : {}) },
    );
    if (encoder.encode(content).length > LIMIT)
      throw new Error("Tabby configuration exceeds 8 MB.");
    return content;
  }
}
async function summarize(key: SecretKey): Promise<KeySummary> {
  return {
    id: key.id,
    name: key.name,
    digest: hex(
      new Uint8Array(
        await crypto.subtle.digest("SHA-256", encoder.encode(key.secret)),
      ),
    ),
  };
}
function localProfile(host: Host, keys: KeyEntry[]): SyncProfile {
  const key = keys.find((k) => k.id === host.keyId);
  return {
    id: host.syncId ?? `ssh:tabius:${host.id}`,
    name: host.name,
    address: host.address,
    port: host.port,
    username: host.username,
    authentication: key ? "publicKey" : (host.syncAuth ?? "password"),
    keyRef: key
      ? `vault://${(key.syncId ?? `file:${key.id}`).slice(5)}`
      : (host.syncKeyRef ?? null),
  };
}
export function nativeFingerprints(known: KnownHost[]): SyncFingerprint[] {
  const result: SyncFingerprint[] = [];
  for (const entry of known) {
    for (const name of entry.host.split(",")) {
      if (name.startsWith("|") || name.startsWith("@")) continue;
      const match = /^\[(.+)\]:(\d+)$/.exec(name);
      const host = (match?.[1] ?? name).toLowerCase(),
        port = Number(match?.[2] ?? 22);
      const digest = base64(bytes64(entry.fingerprint.replace(/^SHA256:/, "")));
      result.push({
        id: `${host}|${port}|${entry.kind}`,
        host,
        port,
        type: entry.kind,
        digest,
      });
    }
  }
  return [...new Map(result.map((x) => [x.id, x])).values()];
}
function configuration(
  response: SyncResponse,
  id: number,
): RemoteConfiguration {
  const config = row(response.body);
  if (config.id !== id || typeof config.content !== "string")
    throw new Error("Tabby-Web returned a different or empty configuration.");
  return {
    id,
    name: text(config.name),
    content: config.content,
    modified_at: text(config.modified_at),
  };
}
export async function synchronize(
  workspace: Workspace,
  server: string,
  id: number,
  password: string,
  services: SyncServices,
): Promise<{ workspace: Workspace; warnings: string[] }> {
  const previous = workspace.sync;
  const baseline =
    previous?.server === server && previous.configurationId === id
      ? previous
      : undefined;
  const response = await services.request(id),
    remote = configuration(response, id);
  const document = await TabbyDocument.open(remote.content ?? "", password);
  const warnings: string[] = [];
  const staged: KeyEntry[] = [];
  try {
    const remoteKeys = document.keys();
    const fileIds = rows(document.vault?.secrets, "secrets")
      .filter((entry) => entry.type === "file")
      .map((entry) => `file:${text(row(entry.key ?? {}).id)}`);
    if (new Set(fileIds).size !== fileIds.length)
      throw new Error("Duplicate vault file IDs.");
    const localKeyIds = workspace.keys.map(
      (key) => key.syncId ?? `file:${key.id}`,
    );
    if (
      [
        ...(baseline?.keyBaseline ?? []).map((key) => key.id),
        ...localKeyIds,
      ].some(
        (id) =>
          fileIds.includes(id) && !remoteKeys.some((key) => key.id === id),
      )
    )
      throw new Error(
        "A synchronized remote key is unreadable. Restore its format before syncing. Nothing was uploaded.",
      );
    let keys = workspace.keys.map((k) => ({ ...k }));
    const mergedKeys: SecretKey[] = [];
    let keyBaseline = baseline?.keyBaseline ?? [];
    if (document.vault) {
      const rawLocal = await services.readKeys(keys);
      const locals: SecretKey[] = rawLocal.map((key) => {
        const metadata = keys.find((k) => k.id === key.id);
        if (!metadata) throw new Error("Missing local key metadata.");
        return {
          ...key,
          id: metadata.syncId ?? `file:${metadata.id}`,
          name: metadata.name,
        };
      });
      for (const key of keys.filter((k) => !k.syncId)) {
        const local = locals.find((k) => k.id === `file:${key.id}`);
        const same =
          local &&
          remoteKeys.find(
            (k) =>
              k.secret === local.secret &&
              !keys.some((other) => other.syncId === k.id),
          );
        if (same && local) {
          key.syncId = same.id;
          local.id = same.id;
        }
      }
      const summaries = merge(
        await Promise.all(locals.map(summarize)),
        await Promise.all(remoteKeys.map(summarize)),
        keyBaseline,
        true,
      );
      for (const summary of summaries) {
        const candidates = [...locals, ...remoteKeys].filter(
          (k) => k.id === summary.id,
        );
        let source: SecretKey | undefined;
        for (const key of candidates)
          if ((await summarize(key)).digest === summary.digest) {
            source = { ...key, name: summary.name };
            break;
          }
        if (!source) throw new Error("Cannot resolve a synchronized key.");
        mergedKeys.push(source);
      }
      const nextKeys: KeyEntry[] = [];
      for (const key of mergedKeys) {
        const existing = keys.find(
          (k) => (k.syncId ?? `file:${k.id}`) === key.id,
        );
        const original = locals.find((k) => k.id === key.id);
        if (existing && original?.secret === key.secret)
          nextKeys.push({ ...existing, name: key.name, syncId: key.id });
        else {
          const imported = await services.importKey({
            ...key,
            passphrase: await document.keyPassphrase(key.secret),
          });
          staged.push(imported);
          nextKeys.push({ ...imported, syncId: key.id });
        }
      }
      keys = nextKeys;
      keyBaseline = summaries;
    } else
      warnings.push(
        "This configuration has no encrypted vault. Private keys were not synchronized.",
      );
    const remoteProfiles = document.profiles();
    const locals = workspace.hosts.map((h) => localProfile(h, keys));
    // Keep local key references out of unencrypted remote configs.
    if (!document.vault)
      for (const profile of locals) {
        profile.keyRef =
          remoteProfiles.find((p) => p.id === profile.id)?.keyRef ?? null;
        profile.authentication =
          remoteProfiles.find((p) => p.id === profile.id)?.authentication ??
          "password";
      }
    const profiles = merge(
      locals,
      remoteProfiles,
      baseline?.baseline ?? [],
      true,
    );
    const localKnown = new Map(
      (baseline?.fingerprints ?? []).map((x) => [x.id, x]),
    );
    for (const item of nativeFingerprints(await services.knownHosts()))
      localKnown.set(item.id, item);
    const fingerprints = merge(
      [...localKnown.values()],
      document.fingerprints(),
      baseline?.knownBaseline ?? [],
    );
    document.replaceProfiles(profiles, baseline?.baseline ?? []);
    if (document.vault)
      document.replaceKeys(mergedKeys, baseline?.keyBaseline ?? []);
    document.replaceFingerprints(fingerprints);
    const hosts = profiles.map((profile) => {
      const existing = workspace.hosts.find(
        (h) => (h.syncId ?? `ssh:tabius:${h.id}`) === profile.id,
      );
      const key = keys.find(
        (k) =>
          `vault://${(k.syncId ?? `file:${k.id}`).slice(5)}` === profile.keyRef,
      );
      if (profile.keyRef && !key)
        warnings.push(
          `${profile.name}: the referenced key is unavailable on this device. Choose a local key before connecting.`,
        );
      const host = {
        ...(existing ?? newHost()),
        name: profile.name,
        address: profile.address,
        port: profile.port,
        username: profile.username,
        syncId: profile.id,
        syncAuth: profile.authentication,
        syncKeyRef: profile.keyRef,
        keyId: document.vault ? (key?.id ?? "") : (existing?.keyId ?? ""),
      };
      // Tabby jumpHost is a profile ID, not a command or local filesystem path.
      if (!existing) {
        const raw = rows(document.config.profiles, "profiles").find(
          (p) => p.id === profile.id,
        );
        let jumpId = text(row(raw?.options ?? {}).jumpHost);
        const visited = new Set([profile.id]);
        const chain: string[] = [];
        while (jumpId) {
          if (visited.has(jumpId) || visited.size > 16)
            throw new Error(
              `Invalid jump-host chain for ${profile.name}. Nothing was uploaded.`,
            );
          visited.add(jumpId);
          const jump = profiles.find((p) => p.id === jumpId);
          if (!jump)
            throw new Error(
              `Missing jump host for ${profile.name}. Nothing was uploaded.`,
            );
          chain.unshift(
            `${jump.username ? `${jump.username}@` : ""}${jump.address.includes(":") ? `[${jump.address}]` : jump.address}:${jump.port}`,
          );
          const source = rows(document.config.profiles, "profiles").find(
            (p) => p.id === jumpId,
          );
          jumpId = text(row(source?.options ?? {}).jumpHost);
        }
        host.jump = chain.join(",");
      }
      return host;
    });
    const hostIds = new Set(hosts.map((h) => h.id));
    const content = await document.serialize(password);
    const latest = configuration(await services.request(id), id);
    if (
      latest.content !== remote.content ||
      latest.modified_at !== remote.modified_at
    )
      throw new Error(
        "The server configuration changed during sync. Try again. Nothing was uploaded.",
      );
    services.ensureUnchanged?.();
    await services.request(id, content, response.etag);
    return {
      workspace: {
        ...workspace,
        hosts,
        keys,
        forwards: workspace.forwards.filter((f) => hostIds.has(f.hostId)),
        sync: {
          server,
          configurationId: id,
          baseline: profiles,
          keyBaseline,
          knownBaseline: fingerprints,
          fingerprints,
          lastSyncedAt: new Date().toISOString(),
        },
      },
      warnings,
    };
  } catch (error) {
    for (const key of staged) await services.deleteKey(key.id).catch(() => {});
    throw error;
  }
}
