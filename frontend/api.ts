import { invoke, isTauri } from "@tauri-apps/api/core";
import type { Workspace } from "./model";
export const desktop = isTauri();
export function loadWorkspace(): Promise<Workspace> {
  return native("load_workspace");
}
export function saveWorkspace(workspace: Workspace): Promise<void> {
  return native("save_workspace", { workspace });
}
export async function native<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!desktop)
    throw new Error(
      "Open the desktop app to use the workspace. Run cargo run.",
    );
  return invoke<T>(command, args);
}
