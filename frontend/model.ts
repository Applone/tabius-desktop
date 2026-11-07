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
export function hostChain(host: Host): string {
  const address = host.address.includes(":")
    ? `[${host.address}]`
    : host.address;
  const endpoint = `${host.username ? `${host.username}@` : ""}${address}:${host.port}`;
  return [host.proxy, host.jump, endpoint].filter(Boolean).join(",");
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
