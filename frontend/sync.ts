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
  encryption?: "modern" | "tabby";
  baseline: SyncProfile[];
  keyBaseline: KeySummary[];
  knownBaseline: SyncFingerprint[];
  fingerprints: SyncFingerprint[];
  lastSyncedAt?: string;
}
export interface RemoteConfiguration {
  id: number;
  name: string;
}
