import { invoke, isTauri } from "@tauri-apps/api/core";
import type { Workspace } from "./model";
import { emptyWorkspace } from "./model";
export const desktop = isTauri();
export async function loadWorkspace(): Promise<Workspace> {
  if (desktop) return invoke("load_workspace");
  const stored = localStorage.getItem("tabius-preview");
  return stored ? (JSON.parse(stored) as Workspace) : emptyWorkspace();
}
export async function saveWorkspace(workspace: Workspace): Promise<void> {
  if (desktop) return invoke("save_workspace", { workspace });
  localStorage.setItem("tabius-preview", JSON.stringify(workspace));
}
export async function native<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!desktop)
    throw new Error(
      "Open the desktop app to use SSH and local key files. Run cargo run.",
    );
  return invoke<T>(command, args);
}
