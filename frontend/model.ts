import type { SyncState } from "./sync";
export interface Host {
  syncId?: string;
  syncAuth?: string;
  syncKeyRef?: string | null;
  id: string;
  name: string;
  address: string;
  port: number;
  username: string;
  keyId: string;
  agentForwarding: boolean;
  startup: string;
  jump: string;
  proxy: string;
  env: string;
  utf8: boolean;
  mosh: boolean;
  system: string;
}
export interface KeyEntry {
  syncId?: string;
  id: string;
  name: string;
  kind: string;
  publicKey: string;
  fingerprint: string;
}
export interface Snippet {
  id: string;
  name: string;
  command: string;
}
export interface Forward {
  id: string;
  name: string;
  hostId: string;
  kind: "local" | "remote" | "dynamic";
  bindAddress: string;
  bindPort: number;
  destination: string;
  destinationPort: number;
}
export interface HistoryEntry {
  id: string;
  host: string;
  command: string;
  date: string;
}
export interface Settings {
  autoReconnect: boolean;
  bell: boolean;
  optionAsMeta: boolean;
  emulator: string;
  font: string;
  fontSize: number;
  lineHeight: number;
  theme: string;
  highlighting: boolean;
  detectOS: boolean;
  postQuantum: boolean;
}
export interface Workspace {
  sync?: SyncState;
  hosts: Host[];
  keys: KeyEntry[];
  snippets: Snippet[];
  forwards: Forward[];
  history: HistoryEntry[];
  settings: Settings;
}
export interface KnownHost {
  synced?: boolean;
  line: string;
  host: string;
  kind: string;
  fingerprint: string;
}
export const defaults: Settings = {
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
export const emptyWorkspace = (): Workspace => ({
  hosts: [],
  keys: [],
  snippets: [],
  forwards: [],
  history: [],
  settings: { ...defaults },
});
export const newHost = (): Host => ({
  id: crypto.randomUUID(),
  name: "",
  address: "",
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
});
export const newForward = (): Forward => ({
  id: crypto.randomUUID(),
  name: "",
  hostId: "",
  kind: "local",
  bindAddress: "127.0.0.1",
  bindPort: 8080,
  destination: "127.0.0.1",
  destinationPort: 80,
});
export function validateHost(host: Host): string | undefined {
  if (!host.name.trim()) return "Give this host a name.";
  if (
    !host.address ||
    !/^[a-zA-Z0-9:._-]+$/.test(host.address) ||
    host.address.startsWith("-")
  )
    return "Enter a hostname or IP address without a protocol or path.";
  if (!Number.isInteger(host.port) || host.port < 1 || host.port > 65535)
    return "Port must be between 1 and 65535.";
}
export function historyToSnippets(text: string): string[] {
  return [
    ...new Set(
      text
        .split(/\r?\n/)
        .map((line) => line.replace(/^: \d+:\d+;/, "").trim())
        .filter((line) => line && !line.startsWith("#")),
    ),
  ]
    .slice(-500)
    .reverse();
}
export function upsert<T extends { id: string }>(items: T[], item: T): T[] {
  return items.some((value) => value.id === item.id)
    ? items.map((value) => (value.id === item.id ? item : value))
    : [...items, item];
}
