<script lang="ts">
  import {
    ArrowRight,
    ArrowsLeftRight,
    CheckCircle,
    CircleNotch,
    Code,
    Copy,
    Fingerprint,
    FolderPlus,
    GearSix,
    Key,
    PencilSimple,
    Plus,
    HardDrives as Server,
    Terminal,
    Trash,
    UploadSimple,
    Warning,
    X,
  } from "phosphor-svelte";
  import { onMount } from "svelte";
  import { desktop, loadWorkspace, native, saveWorkspace } from "./api";
  import fingerprintIcon from "./assets/tabius/fingerprint.svg";
  import hostsIcon from "./assets/tabius/hosts.svg";
  import keyIcon from "./assets/tabius/key.svg";
  import forwardingIcon from "./assets/tabius/port-forwarding.svg";
  import snippetsIcon from "./assets/tabius/snippets.svg";
  import EditorModal, { type Editor } from "./EditorModal.svelte";
  import Empty from "./Empty.svelte";
  import HostLibrary from "./HostLibrary.svelte";
  import HostPicker from "./HostPicker.svelte";
  import Modal from "./Modal.svelte";
  import type {
    Forward,
    Host,
    HostGroup,
    KnownHost,
    Session,
    Snippet,
    Workspace,
  } from "./model";
  import SessionTools from "./SessionTools.svelte";
  import SettingsView from "./SettingsView.svelte";
  import SFTP from "./SFTP.svelte";
  import SSHConfigImport from "./SSHConfigImport.svelte";
  import { listenForShortcuts, type ShortcutAction } from "./shortcuts";
  import Tabs from "./Tabs.svelte";
  import TerminalPane from "./TerminalPane.svelte";
  import { adjacentTab, cycleTab, reorderTabs } from "./tab-order";

  type Page =
    | "Hosts"
    | "Keychain"
    | "Port forwarding"
    | "Snippets"
    | "Known hosts"
    | "Settings";
  const pages = [
    { name: "Hosts", icon: hostsIcon },
    { name: "Keychain", icon: keyIcon },
    { name: "Port forwarding", icon: forwardingIcon },
    { name: "Snippets", icon: snippetsIcon },
    { name: "Known hosts", icon: fingerprintIcon },
  ] as const;

  let workspace = $state.raw<Workspace>({
    hosts: [],
    keys: [],
    snippets: [],
    forwards: [],
    settings: {} as Workspace["settings"],
  });
  let loaded = $state(false);
  let loadError = $state("");
  let page = $state<Page>("Hosts");
  let editor = $state.raw<Editor | null>(null);
  let importingSSH = $state(false);
  let sessions = $state.raw<Session[]>([]);
  let active = $state("library");
  let toolsOpen = $state(true);
  let sftpVisited = $state(false);
  let focusRequest = $state(0);
  let notice = $state("");
  let known = $state.raw<KnownHost[]>([]);
  let confirm = $state.raw<{
    message: string;
    action: () => void;
  } | null>(null);
  let syncing = $state(false);
  let choosingHost = $state(false);
  let saveChain = Promise.resolve();
  onMount(() => {
    void loadWorkspace()
      .then((value) => {
        workspace = value;
        loaded = true;
      })
      .catch((err) => (loadError = String(err)));
  });
  $effect(() => {
    if (!notice) return;
    const timer = setTimeout(() => (notice = ""), 6000);
    return () => clearTimeout(timer);
  });
  function update(next: Workspace) {
    workspace = next;
    saveChain = saveChain
      .then(() => saveWorkspace(next))
      .catch((err) => {
        notice = `Could not save: ${String(err)}`;
      });
  }
  const refreshKnown = () => {
    if (desktop)
      void native<KnownHost[]>("known_hosts")
        .then((value) => {
          known = value;
        })
        .catch((err) => (notice = String(err)));
  };
  $effect(() => {
    if (loaded) refreshKnown();
  });
  $effect(() => {
    if (loaded && page === "Known hosts") refreshKnown();
  });
  async function connect(host: Host, forward?: Forward) {
    try {
      const session = await native<Session>("prepare_session", {
        workspace,
        host,
        forward: forward ?? null,
      });
      sessions = [...sessions, session];
      active = session.id;
      ++focusRequest;
    } catch (err) {
      notice = String(err);
    }
  }
  async function createEditor(
    type: "host" | "forward" | "snippet" | "group",
    groupId = "",
  ) {
    try {
      if (type === "host")
        editor = {
          type,
          value: {
            ...(await native<Host>("new_item", { kind: type })),
            groupId,
          },
        };
      else if (type === "forward")
        editor = {
          type,
          value: await native<Forward>("new_item", { kind: type }),
        };
      else if (type === "group")
        editor = {
          type,
          value: await native<HostGroup>("new_item", { kind: type }),
        };
      else
        editor = {
          type,
          value: await native<Snippet>("new_item", { kind: type }),
        };
    } catch (err) {
      notice = String(err);
    }
  }
  function remove(
    collection: "hosts" | "keys" | "snippets" | "forwards" | "groups",
    id: string,
  ) {
    confirm = {
      message:
        collection === "groups"
          ? "Delete this group? Its hosts will remain in Ungrouped."
          : `Delete this ${collection === "keys" ? "key" : collection === "forwards" ? "forwarding rule" : collection.slice(0, -1)}? This cannot be undone.`,
      action: () => {
        void (async () => {
          update(
            await native("remove_item", {
              workspace,
              kind:
                collection === "keys"
                  ? "key"
                  : collection === "hosts"
                    ? "host"
                    : collection === "forwards"
                      ? "forward"
                      : collection === "groups"
                        ? "group"
                        : "snippet",
              item: { id },
            }),
          );
        })().catch((err) => (notice = String(err)));
      },
    };
  }
  function selectTab(id: string, focusTerminal = true) {
    if (id === "sftp") sftpVisited = true;
    if (
      id === "library" ||
      id === "sftp" ||
      sessions.some((session) => session.id === id)
    )
      active = id;
    if (focusTerminal) ++focusRequest;
  }
  function closeTab(id: string) {
    if (active === id) active = adjacentTab(sessions, id);
    sessions = sessions.filter((item) => item.id !== id);
    ++focusRequest;
  }
  $effect(() => {
    if (
      loaded &&
      !syncing &&
      !editor &&
      !confirm &&
      !importingSSH &&
      !choosingHost
    )
      return listenForShortcuts(workspace.settings.shortcuts, handleShortcut);
  });
  function handleShortcut(action: ShortcutAction) {
    if (action === "newTab") choosingHost = true;
    else if (action === "closeTab" && active !== "library" && active !== "sftp")
      closeTab(active);
    else if (action === "nextTab" || action === "previousTab")
      selectTab(cycleTab(sessions, active, action === "nextTab" ? 1 : -1));
    else if (action === "snippets" || action === "library") {
      page = action === "snippets" ? "Snippets" : "Hosts";
      selectTab("library", false);
    } else if (action.startsWith("tab")) {
      const index =
        action === "tab9" ? sessions.length - 1 : Number(action.slice(3)) - 1;
      if (sessions[index]) selectTab(sessions[index].id);
    }
  }
  let activeSession = $derived(sessions.find((s) => s.id === active));
  let interactiveSession = $derived(
    activeSession?.forward ? undefined : activeSession,
  );
  let displayedKnown = $state.raw<KnownHost[]>([]);
  $effect(() => {
    if (loaded)
      void native<KnownHost[]>("display_known", { workspace, known })
        .then((value) => {
          displayedKnown = value;
        })
        .catch((err) => (notice = String(err)));
  });
  let counts = $derived([
    workspace.hosts.length,
    workspace.keys.length,
    workspace.forwards.length,
    workspace.snippets.length,
    displayedKnown.length,
  ]);
</script>

<div
  class={`app-shell ${active === "library" ? "library-open" : "workspace-open"}`}
>
  {#if loaded}
    <Tabs
      {sessions}
      {active}
      disabled={syncing}
      onSelect={selectTab}
      onTools={interactiveSession ? () => (toolsOpen = !toolsOpen) : undefined}
      {toolsOpen}
      onClose={closeTab}
      onNew={() => (choosingHost = true)}
      onReorder={(source, target, after) => {
        sessions = reorderTabs(sessions, source, target, after);
      }}
    />
  {/if}
  <aside class="sidebar" hidden={active !== "library"} inert={syncing}>
    <div class="brand">
      <div class="brand-mark">
        <Terminal size={23} weight="bold" aria-hidden="true" />
      </div>
      <span>tabius</span>
    </div>
    <nav>
      {#each pages as { name, icon }, i (name)}
        <button
          type="button"
          class={`nav-item ${page === name && active === "library" ? "selected" : ""}`}
          aria-current={page === name && active === "library"
            ? "page"
            : undefined}
          onclick={() => {
            page = name;
            selectTab("library", false);
          }}
        >
          <span
            class="nav-icon"
            style:mask-image={`url("${icon}")`}
            style:-webkit-mask-image={`url("${icon}")`}
            aria-hidden="true"
          ></span><span>{name}</span>
          {#if loaded && counts[i] > 0}
            <span class="nav-count">{counts[i]}</span>
          {/if}
        </button>
      {/each}
    </nav>
    <div class="sidebar-bottom">
      <button
        type="button"
        class={`nav-item ${page === "Settings" && active === "library" ? "selected" : ""}`}
        aria-current={page === "Settings" && active === "library"
          ? "page"
          : undefined}
        onclick={() => {
          page = "Settings";
          selectTab("library", false);
        }}
      >
        <GearSix size={19} aria-hidden="true" /><span>Settings</span>
      </button>
    </div>
  </aside>
  <main class="main">
    {#if loadError}
      <div class="empty-state">
        <Warning size={30} aria-hidden="true" />
        <h2>Workspace could not be loaded</h2>
        <p>{loadError}</p>
        <p>Your saved files have not been overwritten.</p>
      </div>
    {:else if !loaded}
      <div class="empty-state">
        <CircleNotch class="spin" size={30} aria-hidden="true" />
        <p>Opening workspace…</p>
      </div>
    {:else}
      <section class="library" hidden={active !== "library"}>
        <header class="page-header" inert={syncing}>
          <div class="heading-row">
            <div><h1>{page}</h1></div>
            {#if page === "Hosts"}
              <div class="spacer"></div>
              <button type="button" onclick={() => (importingSSH = true)}>
                <UploadSimple size={17} aria-hidden="true" />Import SSH config
              </button>
              <button type="button" onclick={() => createEditor("group")}>
                <FolderPlus size={17} aria-hidden="true" />New group
              </button>
              <button
                type="button"
                class="primary"
                onclick={() => createEditor("host")}
              >
                <Plus size={17} aria-hidden="true" />New host
              </button>
            {/if}
            {#if page === "Keychain"}
              <button
                type="button"
                class="primary"
                onclick={() => (editor = { type: "key" })}
              >
                <Plus size={17} aria-hidden="true" />New key
              </button>
            {/if}
            {#if page === "Port forwarding"}
              <button
                type="button"
                class="primary"
                onclick={() => createEditor("forward")}
              >
                <Plus size={17} aria-hidden="true" />New forwarding
              </button>
            {/if}
            {#if page === "Snippets"}
              <button
                type="button"
                class="primary"
                onclick={() => createEditor("snippet")}
              >
                <Plus size={17} aria-hidden="true" />New snippet
              </button>
            {/if}
            {#if page === "Known hosts"}
              <button
                type="button"
                onclick={() => (editor = { type: "known" })}
              >
                <UploadSimple size={17} aria-hidden="true" />Import known hosts
              </button>
            {/if}
          </div>
        </header>
        {#key page}
          <div class="page-content">
            {#if syncing}
              <div class="sync-lock" aria-hidden="true"></div>
            {/if}
            {#if page === "Hosts"}
              {#if workspace.hosts.length || workspace.groups?.length}
                <HostLibrary
                  {workspace}
                  onConnect={(host) => void connect(host)}
                  onEdit={(host) => (editor = { type: "host", value: host })}
                  onDelete={(host) => remove("hosts", host.id)}
                  onEditGroup={(group) =>
                    (editor = { type: "group", value: group })}
                  onDeleteGroup={(group) => remove("groups", group.id)}
                  onNewHost={(groupId) => void createEditor("host", groupId)}
                />
              {:else}
                <Empty
                  title="No hosts"
                  text="Add a host to save its connection settings."
                >
                  {#snippet icon()}
                    <Server size={31} aria-hidden="true" />
                  {/snippet}
                  {#snippet action()}
                    <button
                      type="button"
                      class="primary"
                      onclick={() => createEditor("host")}
                    >
                      <Plus size={17} aria-hidden="true" />Add your first host
                    </button>
                  {/snippet}
                </Empty>
              {/if}
            {/if}
            {#if page === "Keychain"}
              {#if workspace.keys.length}
                <div class="section-label">
                  Keys <span>{workspace.keys.length}</span>
                </div>
                <div class="item-list">
                  {#each workspace.keys as key (key.id)}
                    <article class="list-item">
                      <div class="host-icon">
                        <Key size={23} aria-hidden="true" />
                      </div>
                      <div class="item-body">
                        <h3>{key.name}<span class="badge">{key.kind}</span></h3>
                        <p class="mono">{key.fingerprint}</p>
                      </div>
                      <button
                        type="button"
                        onclick={() => {
                          void navigator.clipboard
                            .writeText(key.publicKey)
                            .then(() => (notice = "Public key copied."))
                            .catch((err) => (notice = String(err)));
                        }}
                      >
                        <Copy size={16} aria-hidden="true" />Public key
                      </button>
                      <button
                        type="button"
                        class="icon-button"
                        aria-label={`Delete ${key.name}`}
                        onclick={() => remove("keys", key.id)}
                      >
                        <Trash size={17} aria-hidden="true" />
                      </button>
                    </article>
                  {/each}
                </div>
              {:else}
                <Empty title="No keys" text="Generate or import an SSH key.">
                  {#snippet icon()}
                    <Key size={31} aria-hidden="true" />
                  {/snippet}
                  {#snippet action()}
                    <button
                      type="button"
                      class="primary"
                      onclick={() => (editor = { type: "key" })}
                    >
                      <Plus size={17} aria-hidden="true" />Create or import a
                      key
                    </button>
                  {/snippet}
                </Empty>
              {/if}
            {/if}
            {#if page === "Port forwarding"}
              {#if workspace.forwards.length}
                <div class="item-list">
                  {#each workspace.forwards as rule (rule.id)}
                    <article class="list-item">
                      <div class="host-icon">
                        <ArrowsLeftRight size={23} aria-hidden="true" />
                      </div>
                      <div class="item-body">
                        <h3>
                          {rule.name}<span class="badge">{rule.kind}</span>
                        </h3>
                        <p class="mono">
                          {rule.bindAddress}:{rule.bindPort}
                          {#if rule.kind !== "dynamic"}
                            {" → "}{rule.destination}:{rule.destinationPort}
                          {/if}
                        </p>
                        <p>
                          {workspace.hosts.find((h) => h.id === rule.hostId)
                            ?.name ?? "Host removed"}
                        </p>
                      </div>
                      <button
                        type="button"
                        onclick={() => {
                          const host = workspace.hosts.find(
                            (h) => h.id === rule.hostId,
                          );
                          if (host) connect(host, rule);
                        }}
                      >
                        Open tunnel<ArrowRight size={16} aria-hidden="true" />
                      </button>
                      <button
                        type="button"
                        class="icon-button"
                        aria-label="Edit forwarding"
                        onclick={() =>
                          (editor = { type: "forward", value: rule })}
                      >
                        <PencilSimple size={17} aria-hidden="true" />
                      </button>
                      <button
                        type="button"
                        class="icon-button"
                        aria-label="Delete forwarding"
                        onclick={() => remove("forwards", rule.id)}
                      >
                        <Trash size={17} aria-hidden="true" />
                      </button>
                    </article>
                  {/each}
                </div>
              {:else}
                <Empty
                  title="No forwarding rules"
                  text="Access a remote service locally, expose a local port through a server, or create a SOCKS proxy."
                >
                  {#snippet icon()}
                    <ArrowsLeftRight size={31} aria-hidden="true" />
                  {/snippet}
                  {#snippet action()}
                    <button
                      type="button"
                      class="primary"
                      onclick={() => createEditor("forward")}
                    >
                      <Plus size={17} aria-hidden="true" />Create a forwarding
                      rule
                    </button>
                  {/snippet}
                </Empty>
              {/if}
            {/if}
            {#if page === "Snippets"}
              {#if workspace.snippets.length}
                <div class="snippet-grid">
                  {#each workspace.snippets as snippet (snippet.id)}
                    <article class="snippet-card">
                      <div class="section-row">
                        <h3>
                          <Code size={18} aria-hidden="true" />{snippet.name}
                        </h3>
                        <button
                          type="button"
                          class="icon-button"
                          aria-label="Edit snippet"
                          onclick={() =>
                            (editor = { type: "snippet", value: snippet })}
                        >
                          <PencilSimple size={16} aria-hidden="true" />
                        </button>
                      </div>
                      <pre>{snippet.command}</pre>
                      <div class="section-row">
                        <button
                          type="button"
                          onclick={() => {
                            void navigator.clipboard
                              .writeText(snippet.command)
                              .then(() => (notice = "Snippet copied."))
                              .catch((err) => (notice = String(err)));
                          }}
                        >
                          <Copy size={15} aria-hidden="true" />Copy
                        </button>
                        <button
                          type="button"
                          class="icon-button"
                          aria-label="Delete snippet"
                          onclick={() => remove("snippets", snippet.id)}
                        >
                          <Trash size={16} aria-hidden="true" />
                        </button>
                      </div>
                      <label class="run-snippet"
                        >Run in session<select
                          value=""
                          onchange={(e) => {
                            const id = e.currentTarget.value;
                            if (id)
                              void native("run_snippet", {
                                id,
                                snippetId: snippet.id,
                              })
                                .then(() => {
                                  selectTab(id, false);
                                  notice =
                                    "Snippet sent to the terminal for execution.";
                                })
                                .catch((err) => (notice = String(err)));
                            e.currentTarget.value = "";
                          }}
                        >
                          <option value="">Choose an open session</option>
                          {#each sessions.filter((s) => !s.forward) as s (s.id)}
                            <option value={s.id}>{s.host.name}</option>
                          {/each}
                        </select></label
                      >
                    </article>
                  {/each}
                </div>
              {:else}
                <Empty
                  title="No snippets"
                  text="Save commands and scripts for reuse."
                >
                  {#snippet icon()}
                    <Code size={31} aria-hidden="true" />
                  {/snippet}
                  {#snippet action()}
                    <button
                      type="button"
                      class="primary"
                      onclick={() => createEditor("snippet")}
                    >
                      <Plus size={17} aria-hidden="true" />Add your first
                      snippet
                    </button>
                  {/snippet}
                </Empty>
              {/if}
            {/if}
            {#if page === "Known hosts"}
              {#if displayedKnown.length}
                <div class="item-list">
                  {#each displayedKnown as item}
                    <article class="list-item">
                      <div class="host-icon">
                        <Fingerprint size={23} aria-hidden="true" />
                      </div>
                      <div class="item-body">
                        <h3>
                          {item.host}<span class="badge">{item.kind}</span>
                        </h3>
                        <p class="mono">{item.fingerprint}</p>
                      </div>
                      {#if item.synced}
                        <span class="badge">Reference fingerprint</span>
                      {/if}
                      <button
                        type="button"
                        class="icon-button"
                        aria-label="Remove trusted fingerprint"
                        onclick={() =>
                          (confirm = {
                            message: `Remove trust for ${item.host}? You will be asked to verify this host again.`,
                            action: () => {
                              void native<Workspace>("forget_fingerprint", {
                                workspace,
                                item,
                              })
                                .then((next) => {
                                  update(next);
                                  refreshKnown();
                                })
                                .catch((err) => (notice = String(err)));
                            },
                          })}
                      >
                        <Trash size={17} aria-hidden="true" />
                      </button>
                    </article>
                  {/each}
                </div>
              {:else}
                <Empty
                  title="No known hosts"
                  text="Server identities appear here after you verify them on your first connection. Changed keys are never trusted automatically."
                >
                  {#snippet icon()}
                    <Fingerprint size={31} aria-hidden="true" />
                  {/snippet}
                </Empty>
              {/if}
            {/if}
            {#if page === "Settings"}
              <SettingsView
                settings={workspace.settings}
                {workspace}
                {syncing}
                onSyncBusy={(value) => {
                  syncing = value;
                }}
                isCurrent={(previous) => workspace === previous}
                onSyncApply={async (next) => {
                  const previous = workspace;
                  await saveChain;
                  const warnings = await native<string[]>("apply_sync", {
                    previous,
                    workspace: next,
                  });
                  workspace = next;
                  if (warnings.length) notice = warnings.join(" ");
                  refreshKnown();
                }}
                onChange={(settings) => update({ ...workspace, settings })}
              />
            {/if}
          </div>
        {/key}
      </section>
      {#if sftpVisited}
        <div class="sftp-container" hidden={active !== "sftp"}>
          <SFTP {workspace} onTrustChanged={refreshKnown} />
        </div>
      {/if}
      <div class="terminal-workspace" hidden={!activeSession}>
        <div class="terminal-deck">
          {#each sessions as session (session.id)}
            <div
              class="session-container"
              hidden={activeSession?.id !== session.id}
            >
              <TerminalPane
                {session}
                active={active === session.id}
                {focusRequest}
                settings={workspace.settings}
                onTrustChanged={refreshKnown}
                onClose={() => closeTab(session.id)}
                onOS={(id, system) => {
                  if (!workspace.settings.detectOS) return;
                  const current = workspace;
                  const clean = system
                    .replace(/[^a-zA-Z0-9 _.-]/g, "")
                    .slice(0, 60);
                  if (
                    !clean ||
                    current.hosts.find((host) => host.id === id)?.system ===
                      clean
                  )
                    return;
                  update({
                    ...current,
                    hosts: current.hosts.map((host) =>
                      host.id === id ? { ...host, system: clean } : host,
                    ),
                  });
                }}
              />
            </div>
          {/each}
        </div>
        {#if interactiveSession && toolsOpen}
          <SessionTools
            {workspace}
            sessionId={interactiveSession.id}
            onTheme={(theme) =>
              update({
                ...workspace,
                settings: { ...workspace.settings, theme },
              })}
            onNewSnippet={() => void createEditor("snippet")}
            onClose={() => (toolsOpen = false)}
          />
        {/if}
      </div>
    {/if}
  </main>
  {#if notice}
    <div class="toast" role="status">
      <CheckCircle size={19} aria-hidden="true" />{notice}<button
        type="button"
        aria-label="Dismiss notification"
        onclick={() => (notice = "")}
      >
        <X size={15} aria-hidden="true" />
      </button>
    </div>
  {/if}
  {#if choosingHost}
    <HostPicker
      {workspace}
      onClose={() => (choosingHost = false)}
      onChoose={(host) => {
        choosingHost = false;
        void connect(host);
      }}
      onNewHost={() => {
        choosingHost = false;
        void createEditor("host");
      }}
    />
  {/if}
  {#if editor}
    <EditorModal
      {editor}
      {workspace}
      onClose={() => (editor = null)}
      onSave={(next) => {
        update(next);
        editor = null;
        notice = "Saved.";
      }}
      onKnown={refreshKnown}
    />
  {/if}
  {#if importingSSH}
    <SSHConfigImport
      {workspace}
      onClose={() => (importingSSH = false)}
      onPrepare={() => saveChain}
      onSave={(next, message) => {
        workspace = next;
        importingSSH = false;
        notice = message;
      }}
    />
  {/if}
  {#if confirm}
    <Modal title="Confirm deletion" onClose={() => (confirm = null)}
      ><p>{confirm.message}</p>
      <div class="modal-actions">
        <button type="button" onclick={() => (confirm = null)}>Cancel</button>
        <button
          type="button"
          class="danger"
          onclick={() => {
            confirm?.action();
            confirm = null;
          }}
        >
          Delete
        </button>
      </div>
    </Modal>
  {/if}
</div>
