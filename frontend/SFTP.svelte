<script lang="ts">
  import { Channel } from "@tauri-apps/api/core";
  import {
    ArrowClockwise,
    ArrowDown,
    ArrowLeft,
    ArrowUp,
    File,
    Folder,
    HardDrives,
    Laptop,
    Plug,
    Plus,
    X,
  } from "phosphor-svelte";
  import { onMount } from "svelte";
  import { native } from "./api";
  import {
    type FileEntry,
    fileSize,
    joinPath,
    parentPath,
    validName,
  } from "./file-paths";
  import HostKeyPrompt from "./HostKeyPrompt.svelte";
  import Modal from "./Modal.svelte";
  import type { Host, HostKeyRequest, Workspace } from "./model";

  interface Listing {
    path: string;
    entries: FileEntry[];
  }

  let {
    workspace,
    onTrustChanged,
  }: {
    workspace: Workspace;
    onTrustChanged: () => void;
  } = $props();

  let local = $state.raw<Listing>({ path: "", entries: [] });
  let remote = $state.raw<Listing>({ path: ".", entries: [] });
  let host = $state.raw<Host | undefined>();
  let connection = $state<string | undefined>();
  let request = $state.raw<HostKeyRequest | undefined>();
  let busy = $state(false);
  let connecting = $state(false);
  let error = $state("");
  let status = $state("");
  let selection = $state.raw<
    | {
        side: "local" | "remote";
        entry: FileEntry;
      }
    | undefined
  >();
  let hidden = $state(false);
  let filter = $state("");
  let edit = $state.raw<
    | {
        side: "local" | "remote";
        action: "mkdir" | "rename" | "delete";
      }
    | undefined
  >();
  let name = $state("");
  async function load(side: "local" | "remote", path: string, id = connection) {
    const listing = await native<Listing>("sftp_list", {
      id: side === "remote" ? id : null,
      path,
    });
    if (side === "local") local = listing;
    else remote = listing;
    selection = undefined;
  }
  onMount(() => {
    void native<Listing>("sftp_list", { id: null, path: "" })
      .then((value) => {
        local = value;
      })
      .catch((e) => (error = String(e)));
    return () => {
      if (connection)
        void native("sftp_disconnect", { id: connection }).catch(() => {});
    };
  });
  async function perform(work: () => Promise<void>) {
    busy = true;
    error = "";
    status = "";
    try {
      await work();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
  function disconnect() {
    const id = connection;
    connection = undefined;
    request = undefined;
    host = undefined;
    selection = undefined;
    if (id)
      void native("sftp_disconnect", { id }).catch((e) => (error = String(e)));
  }
  async function connect(next: Host) {
    const id = crypto.randomUUID();
    connection = id;
    host = next;
    connecting = true;
    await perform(async () => {
      try {
        const channel = new Channel<HostKeyRequest>();
        channel.onmessage = (message) => {
          if (connection === id) request = message;
        };
        await native("sftp_connect", {
          id,
          host: next,
          settings: workspace.settings,
          onKey: channel,
        });
        if (connection !== id) {
          await native("sftp_disconnect", { id });
          return;
        }
        await load("remote", ".", id);
        onTrustChanged();
      } catch (e) {
        if (connection === id) disconnect();
        throw e;
      }
    });
    connecting = false;
  }
  async function transfer() {
    if (!selection || !connection) return;
    const { entry, side } = selection;
    await perform(async () => {
      await native("sftp_transfer", {
        id: connection,
        local: joinPath(local.path, entry.name),
        remote: joinPath(remote.path, entry.name),
        upload: side === "local",
      });
      await load(
        side === "local" ? "remote" : "local",
        side === "local" ? remote.path : local.path,
      );
      status = `Transferred ${entry.name}`;
    });
  }
  async function modify() {
    if (!edit) return;
    const { side, action } = edit;
    const listing = side === "local" ? local : remote;
    const entry = selection?.side === side ? selection.entry : undefined;
    await perform(async () => {
      await native("sftp_modify", {
        id: side === "remote" ? connection : null,
        action: action === "delete" && entry?.directory ? "rmdir" : action,
        path: joinPath(
          listing.path,
          action === "mkdir" ? name : (entry?.name ?? ""),
        ),
        destination: joinPath(listing.path, name),
      });
      edit = undefined;
      await load(side, listing.path);
    });
  }
</script>

<section class="sftp-workspace" aria-label="SFTP file browser">
  <header class="sftp-heading">
    <div>
      <h1>File transfer</h1>
      <p>Local files and remote servers, side by side.</p>
    </div>
    <label class="sftp-hidden"
      ><input
        type="checkbox"
        checked={hidden}
        onchange={(e) => (hidden = e.currentTarget.checked)}
      >{" "}Show hidden files</label
    >
    <input
      aria-label="Filter files"
      placeholder="Filter files…"
      value={filter}
      oninput={(e) => (filter = e.currentTarget.value)}
    >
  </header>
  <div class="file-panes">
    {#each ["local", "remote"] as const as side (side)}
      {@const listing = side === "local" ? local : remote}
      {@const connected = side === "local" || (connection && !connecting)}
      {@const selected = selection?.side === side ? selection.entry : undefined}
      {@const entries = listing.entries
        .filter(
          (entry) =>
            (hidden || !entry.name.startsWith(".")) &&
            entry.name.toLowerCase().includes(filter.toLowerCase()),
        )
        .sort(
          (a, b) =>
            Number(b.directory) - Number(a.directory) ||
            a.name.localeCompare(b.name),
        )}
      <section
        class="file-pane"
        aria-label={side === "local" ? "Local files" : "Remote files"}
      >
        <header class="file-pane-heading">
          {#if side === "local"}
            <Laptop size={20} aria-hidden="true" />
          {:else}
            <HardDrives size={20} aria-hidden="true" />
          {/if}
          <div>
            <strong
              >{side === "local"
                ? "This computer"
                : (host?.name ?? "Remote server")}</strong
            ><span
              >{side === "local"
                ? ""
                : host
                  ? `${host.username ? `${host.username}@` : ""}${host.address}`
                  : ""}</span
            >
          </div>
          {#if connection && side === "remote"}
            <button
              type="button"
              class="icon-button"
              aria-label="Disconnect SFTP"
              onclick={disconnect}
            >
              <X size={18} aria-hidden="true" />
            </button>
          {/if}
        </header>
        {#if connected}
          <form
            class="file-path"
            onsubmit={(e) => {
              e.preventDefault();
              const data = new FormData(e.currentTarget);
              void perform(() => load(side, String(data.get("path"))));
            }}
          >
            <button
              type="button"
              class="icon-button"
              aria-label={`Parent ${side} directory`}
              disabled={busy}
              onclick={() =>
                void perform(() => load(side, parentPath(listing.path)))}
            >
              <ArrowLeft size={17} aria-hidden="true" />
            </button>
            {#key listing.path}
              <input
                name="path"
                aria-label={`${side} path`}
                defaultValue={listing.path}
                disabled={busy}
              >
            {/key}
            <button
              type="button"
              class="icon-button"
              aria-label={`Refresh ${side}`}
              disabled={busy}
              onclick={() => void perform(() => load(side, listing.path))}
            >
              <ArrowClockwise size={17} aria-hidden="true" />
            </button>
          </form>
          <div class="file-actions">
            <button
              type="button"
              disabled={busy}
              onclick={() => {
                name = "";
                edit = { side, action: "mkdir" };
              }}
            >
              <Plus size={15} aria-hidden="true" />Folder
            </button>
            <button
              type="button"
              disabled={busy || !selected}
              onclick={() => {
                name = selected?.name ?? "";
                edit = { side, action: "rename" };
              }}
            >
              Rename
            </button>
            <button
              type="button"
              disabled={busy || !selected}
              onclick={() => (edit = { side, action: "delete" })}
            >
              Delete
            </button>
            <button
              type="button"
              disabled={busy ||
                !connection ||
                connecting ||
                !selected ||
                selected.directory}
              onclick={() => void transfer()}
            >
              {#if side === "local"}
                <ArrowUp size={15} aria-hidden="true" />
              {:else}
                <ArrowDown size={15} aria-hidden="true" />
              {/if}
              {side === "local" ? "Upload" : "Download"}
            </button>
          </div>
          <div class="file-table-wrap">
            <table class="file-table">
              <thead>
                <tr>
                  <th>Name</th>
                  <th>Size</th>
                  <th>Modified</th>
                </tr>
              </thead>
              <tbody>
                {#each entries as entry (entry.name)}
                  <tr class={selected?.name === entry.name ? "selected" : ""}>
                    <td>
                      <button
                        type="button"
                        disabled={busy}
                        onclick={() => (selection = { side, entry })}
                        ondblclick={() => {
                          if (entry.directory)
                            void perform(() =>
                              load(side, joinPath(listing.path, entry.name)),
                            );
                        }}
                        onkeydown={(e) => {
                          if (e.key === "Enter" && entry.directory)
                            void perform(() =>
                              load(side, joinPath(listing.path, entry.name)),
                            );
                        }}
                      >
                        {#if entry.directory}
                          <Folder
                            size={18}
                            weight="duotone"
                            aria-hidden="true"
                          />
                        {:else}
                          <File size={18} aria-hidden="true" />
                        {/if}
                        <span>{entry.name}</span>
                      </button>
                    </td>
                    <td>{entry.directory ? "—" : fileSize(entry.size)}</td>
                    <td>
                      {entry.modified
                        ? new Date(entry.modified * 1000).toLocaleDateString()
                        : "—"}
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
            {#if entries.length === 0}
              <p class="file-empty">
                {filter ? "No matching files" : "This folder is empty"}
              </p>
            {/if}
          </div>
          <footer class="file-footer">
            {entries.length}
            items <span>{selected?.name ?? ""}</span>
          </footer>
        {:else}
          <div class="remote-picker">
            <div class="remote-symbol">
              <Plug size={32} aria-hidden="true" />
            </div>
            <h2>{connecting ? "Connecting…" : "Connect to a host"}</h2>
            <p>
              {connecting
                ? "Establishing a secure SFTP connection."
                : "Choose a saved host to browse and transfer files."}
            </p>
            {#if !connecting}
              <p class="hint">Use an SSH key or agent for authentication.</p>
              {#if workspace.hosts.length}
                <div class="sftp-hosts">
                  {#each workspace.hosts as item (item.id)}
                    <button
                      type="button"
                      onclick={() => void connect(item)}
                      disabled={busy}
                    >
                      <HardDrives size={19} aria-hidden="true" />
                      <span
                        ><strong>{item.name}</strong
                        ><small>{item.address}</small></span
                      ><Plug size={17} aria-hidden="true" />
                    </button>
                  {/each}
                </div>
              {:else}
                <p>Add a host in Library to get started.</p>
              {/if}
            {/if}
          </div>
        {/if}
      </section>
    {/each}
  </div>
  {#if busy || status}
    <footer class="transfer-status" role="status" aria-live="polite">
      {busy ? "Working…" : status}
    </footer>
  {/if}
  {#if error}
    <div class="sftp-error" role="alert">
      {error}<button
        type="button"
        class="icon-button"
        aria-label="Dismiss file error"
        onclick={() => (error = "")}
      >
        <X size={15} aria-hidden="true" />
      </button>
    </div>
  {/if}
  {#if request && connection}
    <HostKeyPrompt
      command="sftp_answer"
      {request}
      sessionId={connection}
      references={[]}
      onAnswered={() => {
        request = undefined;
        onTrustChanged();
      }}
      onAbort={disconnect}
    />
  {/if}
  {#if edit}
    <Modal
      title={edit.action === "mkdir"
        ? "New folder"
        : edit.action === "rename"
          ? "Rename"
          : "Delete item"}
      onClose={() => {
        if (!busy) edit = undefined;
      }}
    >
      <form
        onsubmit={(e) => {
          e.preventDefault();
          void modify();
        }}
      >
        {#if edit.action === "delete"}
          <p>
            Delete “{selection?.entry.name}”? Only empty folders can be deleted.
            This cannot be undone.
          </p>
        {:else}
          <label
            >Name<input
              aria-label="File name"
              value={name}
              oninput={(e) => (name = e.currentTarget.value)}
            ></label
          >
        {/if}
        {#if error}
          <p role="alert">{error}</p>
        {/if}
        <div class="modal-actions">
          <button
            type="button"
            disabled={busy}
            onclick={() => (edit = undefined)}
          >
            Cancel
          </button>
          <button
            type="submit"
            class={edit.action === "delete" ? "danger" : "primary"}
            disabled={busy || (edit.action !== "delete" && !validName(name))}
          >
            {edit.action === "delete" ? "Delete" : "Save"}
          </button>
        </div>
      </form>
    </Modal>
  {/if}
</section>
