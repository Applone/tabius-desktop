<script lang="ts" module>
  import type { Forward, Host, HostGroup, Snippet } from "./model";
  export type Editor =
    | { type: "host"; value: Host }
    | { type: "forward"; value: Forward }
    | { type: "snippet"; value: Snippet }
    | { type: "group"; value: HostGroup }
    | { type: "key" }
    | { type: "known" };
</script>

<script lang="ts">
  import {
    Check,
    CircleNotch,
    Desktop,
    LockKey,
    HardDrives as Server,
    ShieldCheck,
    UploadSimple,
    Warning,
  } from "phosphor-svelte";
  import { untrack } from "svelte";
  import { native } from "./api";
  import Field from "./Field.svelte";
  import Modal from "./Modal.svelte";
  import type { KeyEntry, Workspace } from "./model";
  import { hostChain } from "./model";
  import Toggle from "./Toggle.svelte";

  let {
    editor,
    workspace,
    onClose,
    onSave,
    onKnown,
  }: {
    editor: Editor;
    workspace: Workspace;
    onClose: () => void;
    onSave: (next: Workspace) => void;
    onKnown: () => void;
  } = $props();

  let host = $state.raw(
    untrack(() => (editor.type === "host" ? editor.value : ({} as Host))),
  );
  let rule = $state.raw(
    untrack(() => (editor.type === "forward" ? editor.value : ({} as Forward))),
  );
  let snippet = $state.raw(
    untrack(() =>
      editor.type === "snippet"
        ? editor.value
        : { id: "", name: "", command: "" },
    ),
  );
  let keyMode = $state("generate");
  let group = $state.raw(
    untrack(() =>
      editor.type === "group" ? editor.value : { id: "", name: "" },
    ),
  );
  let publicKey = $state("");
  let keyName = $state("");
  let kind = $state("ed25519");
  let passphrase = $state("");
  let text = $state("");
  let error = $state("");
  let busy = $state(false);
  const titles = {
    host: "Host details",
    forward: "Port forwarding",
    snippet: "Snippet",
    group: "Host group",
    key: "Add a key",
    known: "Import known hosts",
  };
  const setH = <K extends keyof Host>(key: K, value: Host[K]) =>
    (host = { ...host, [key]: value });
  const setF = <K extends keyof Forward>(key: K, value: Forward[K]) =>
    (rule = { ...rule, [key]: value });
  let chainHosts = $derived(workspace.hosts.filter((h) => h.id !== host.id));
  async function submit(e: SubmitEvent) {
    e.preventDefault();
    error = "";
    busy = true;
    try {
      if (
        editor.type === "host" ||
        editor.type === "forward" ||
        editor.type === "snippet" ||
        editor.type === "group"
      ) {
        onSave(
          await native("edit_workspace", {
            workspace,
            kind: editor.type,
            item:
              editor.type === "host"
                ? host
                : editor.type === "forward"
                  ? rule
                  : editor.type === "snippet"
                    ? snippet
                    : group,
            remove: false,
          }),
        );
      }
      if (editor.type === "key") {
        const key = await native<KeyEntry>(
          keyMode === "generate" ? "generate_key" : "import_key",
          { name: keyName, kind, passphrase, publicKey, content: text },
        );
        onSave(
          await native("edit_workspace", {
            workspace,
            kind: "key",
            item: key,
            remove: false,
          }),
        );
      }
      if (editor.type === "known") {
        await native("import_known_hosts", { content: text });
        onKnown();
        onClose();
      }
    } catch (err) {
      error = String(err).replace(/^Error: /, "");
    } finally {
      busy = false;
    }
  }
  async function readFile(file: File | undefined) {
    if (!file) return;
    if (file.size > 1024 * 1024) {
      error = "Choose a file smaller than 1 MB.";
      return;
    }
    try {
      text = await file.text();
    } catch (err) {
      error = String(err);
    }
  }
</script>

<Modal title={titles[editor.type]} {onClose} wide={editor.type === "host"}>
  <form
    onsubmit={(e) => {
      void submit(e);
    }}
  >
    <fieldset disabled={busy}>
      {#if editor.type === "group"}
        <Field label="Group name">
          {#snippet control(id, describedBy)}
            <input
              {id}
              aria-describedby={describedBy}
              required
              maxLength={100}
              value={group.name}
              oninput={(event) =>
                (group = { ...group, name: event.currentTarget.value })}
            >
          {/snippet}
        </Field>
      {/if}
      {#if editor.type === "host"}
        <div class="form-section">
          <h3>Connection</h3>
          <Field label="Label">
            {#snippet control(id, describedBy)}
              <input
                {id}
                aria-describedby={describedBy}
                required
                value={host.name}
                oninput={(e) => setH("name", e.currentTarget.value)}
                placeholder="e.g. Production server"
              >
            {/snippet}
          </Field>
          <Field label="Hostname or IP address">
            {#snippet control(id, describedBy)}
              <input
                {id}
                aria-describedby={describedBy}
                required
                value={host.address}
                oninput={(e) => setH("address", e.currentTarget.value)}
                placeholder="192.168.1.10"
              >
            {/snippet}
          </Field>
          <Field label="Group">
            {#snippet control(id, describedBy)}
              <select
                {id}
                aria-describedby={describedBy}
                value={host.groupId ?? ""}
                onchange={(event) => setH("groupId", event.currentTarget.value)}
              >
                <option value="">Ungrouped</option>
                {#each workspace.groups ?? [] as g (g.id)}
                  <option value={g.id}>{g.name}</option>
                {/each}
              </select>
            {/snippet}
          </Field>
          <div class="form-columns">
            <Field label="Username">
              {#snippet control(id, describedBy)}
                <input
                  {id}
                  aria-describedby={describedBy}
                  autoCapitalize="none"
                  autoCorrect="off"
                  spellcheck={false}
                  value={host.username}
                  oninput={(e) => setH("username", e.currentTarget.value)}
                  placeholder="System default"
                >
              {/snippet}
            </Field>
            <Field label="SSH port">
              {#snippet control(id, describedBy)}
                <input
                  {id}
                  aria-describedby={describedBy}
                  type="number"
                  min="1"
                  max="65535"
                  required
                  value={host.port}
                  oninput={(e) => setH("port", Number(e.currentTarget.value))}
                >
              {/snippet}
            </Field>
          </div>
          <Field
            label="Authentication"
            hint="Password and key passphrase prompts appear securely inside the terminal."
          >
            {#snippet control(id, describedBy)}
              <select
                {id}
                aria-describedby={describedBy}
                value={host.keyId}
                onchange={(e) => setH("keyId", e.currentTarget.value)}
              >
                <option value="">SSH agent / interactive password</option>
                {#each workspace.keys as k (k.id)}
                  <option value={k.id}>{k.name}</option>
                {/each}
              </select>
            {/snippet}
          </Field>
        </div>
        <div class="form-section">
          <h3>Session options</h3>
          <Toggle
            label="Agent forwarding"
            description="Allow the remote host to use your local SSH agent."
            value={host.agentForwarding}
            onChange={(v) => setH("agentForwarding", v)}
          />
          <Field label="Startup snippet">
            {#snippet control(id, describedBy)}
              <select
                {id}
                aria-describedby={describedBy}
                value={host.startup}
                onchange={(e) => setH("startup", e.currentTarget.value)}
              >
                <option value="">None</option>
                {#each workspace.snippets as s (s.id)}
                  <option value={s.id}>{s.name}</option>
                {/each}
              </select>
            {/snippet}
          </Field>
          <Field
            label="Host chaining"
            hint="Copy a saved host's SSH route as the jump route. Authentication uses your SSH agent or default keys."
          >
            {#snippet control(id, describedBy)}
              <select
                {id}
                aria-describedby={describedBy}
                value={host.jump}
                onchange={(e) => setH("jump", e.currentTarget.value)}
              >
                <option value="">None</option>
                {#if host.jump &&
                  !chainHosts.some((h) => hostChain(h) === host.jump)}
                  <option value={host.jump}>
                    Existing route ({host.jump})
                  </option>
                {/if}
                {#each chainHosts as h (h.id)}
                  <option value={hostChain(h)}>
                    {`${h.name} (${h.address}:${h.port})`}
                  </option>
                {/each}
              </select>
            {/snippet}
          </Field>
          <Field
            label="SSH proxy"
            hint="An additional SSH proxy hop, placed before the host chain."
          >
            {#snippet control(id, describedBy)}
              <input
                {id}
                aria-describedby={describedBy}
                value={host.proxy}
                oninput={(e) => setH("proxy", e.currentTarget.value)}
                placeholder="user@proxy.example.com:22"
              >
            {/snippet}
          </Field>
          <Field
            label="Environment variables"
            hint="One NAME=value per line. The server must allow these through AcceptEnv."
          >
            {#snippet control(id, describedBy)}
              <textarea
                {id}
                aria-describedby={describedBy}
                rows={3}
                value={host.env}
                oninput={(e) => setH("env", e.currentTarget.value)}
                placeholder="LANG=en_US.UTF-8"
              ></textarea>
            {/snippet}
          </Field>
          <Toggle
            label="UTF-8"
            description="Send a UTF-8 locale to the remote session."
            value={host.utf8}
            onChange={(v) => setH("utf8", v)}
          /><Toggle
            label="Mosh"
            description="Requires mosh locally and mosh-server on the host. SSH forwarding uses OpenSSH."
            value={host.mosh}
            onChange={(v) => setH("mosh", v)}
          />
        </div>
      {/if}
      {#if editor.type === "forward"}
        <div class="segmented">
          {#each ["local", "remote", "dynamic"] as const as value (value)}
            <button
              type="button"
              class={rule.kind === value ? "chosen" : ""}
              onclick={() => setF("kind", value)}
            >
              {value[0].toUpperCase() + value.slice(1)}
            </button>
          {/each}
        </div>
        <div class="forward-diagram">
          <Desktop size={30} aria-hidden="true" /><span>········</span>
          <ShieldCheck size={33} aria-hidden="true" /><span>········</span>
          <Server size={30} aria-hidden="true" />
        </div>
        <p class="helper">
          {rule.kind === "local"
            ? "Access a remote service from a port on your computer."
            : rule.kind === "remote"
              ? "Expose a local service through a listening port on your SSH server."
              : "Create a local SOCKS5 proxy through an SSH host."}
        </p>
        <Field label="Rule name">
          {#snippet control(id, describedBy)}
            <input
              {id}
              aria-describedby={describedBy}
              required
              value={rule.name}
              oninput={(e) => setF("name", e.currentTarget.value)}
              placeholder="e.g. Database tunnel"
            >
          {/snippet}
        </Field>
        <Field label="SSH host">
          {#snippet control(id, describedBy)}
            <select
              {id}
              aria-describedby={describedBy}
              required
              value={rule.hostId}
              onchange={(e) => setF("hostId", e.currentTarget.value)}
            >
              <option value="">Select a host</option>
              {#each workspace.hosts as h (h.id)}
                <option value={h.id}>{h.name}</option>
              {/each}
            </select>
          {/snippet}
        </Field>
        <div class="form-columns">
          <Field label="Bind address">
            {#snippet control(id, describedBy)}
              <select
                {id}
                aria-describedby={describedBy}
                value={rule.bindAddress}
                onchange={(e) => setF("bindAddress", e.currentTarget.value)}
              >
                <option>127.0.0.1</option>
                <option>::1</option>
              </select>
            {/snippet}
          </Field>
          <Field label="Listening port">
            {#snippet control(id, describedBy)}
              <input
                {id}
                aria-describedby={describedBy}
                required
                type="number"
                min="1"
                max="65535"
                value={rule.bindPort}
                oninput={(e) => setF("bindPort", Number(e.currentTarget.value))}
              >
            {/snippet}
          </Field>
        </div>
        {#if rule.kind !== "dynamic"}
          <div class="form-columns">
            <Field label="Destination host">
              {#snippet control(id, describedBy)}
                <input
                  {id}
                  aria-describedby={describedBy}
                  required
                  value={rule.destination}
                  oninput={(e) => setF("destination", e.currentTarget.value)}
                >
              {/snippet}
            </Field>
            <Field label="Destination port">
              {#snippet control(id, describedBy)}
                <input
                  {id}
                  aria-describedby={describedBy}
                  required
                  type="number"
                  min="1"
                  max="65535"
                  value={rule.destinationPort}
                  oninput={(e) =>
                    setF("destinationPort", Number(e.currentTarget.value))}
                >
              {/snippet}
            </Field>
          </div>
        {/if}
        <p class="helper">
          Listeners are restricted to loopback. Remote forwarding also depends
          on the server’s GatewayPorts policy. Close the tunnel tab to stop it.
        </p>
      {/if}
      {#if editor.type === "snippet"}
        <Field label="Label">
          {#snippet control(id, describedBy)}
            <input
              {id}
              aria-describedby={describedBy}
              required
              value={snippet.name}
              oninput={(e) =>
                (snippet = { ...snippet, name: e.currentTarget.value })}
              placeholder="e.g. Check disk usage"
            >
          {/snippet}
        </Field>
        <Field label="Script">
          {#snippet control(id, describedBy)}
            <textarea
              {id}
              aria-describedby={describedBy}
              required
              class="code-input"
              rows={9}
              value={snippet.command}
              oninput={(e) =>
                (snippet = { ...snippet, command: e.currentTarget.value })}
              placeholder="df -h"
            ></textarea>
          {/snippet}
        </Field>
        <p class="helper">
          Insert snippets into an open terminal, or use one as a host’s startup
          snippet. Startup snippets execute after authentication.
        </p>
      {/if}
      {#if editor.type === "key"}
        <div class="segmented">
          <button
            type="button"
            class={keyMode === "generate" ? "chosen" : ""}
            onclick={() => (keyMode = "generate")}
          >
            Generate key
          </button>
          <button
            type="button"
            class={keyMode === "import" ? "chosen" : ""}
            onclick={() => (keyMode = "import")}
          >
            Import key
          </button>
        </div>
        <Field label="Key label">
          {#snippet control(id, describedBy)}
            <input
              {id}
              aria-describedby={describedBy}
              required
              value={keyName}
              oninput={(e) => (keyName = e.currentTarget.value)}
              placeholder="e.g. Personal laptop"
            >
          {/snippet}
        </Field>
        {#if keyMode === "generate"}
          <Field label="Algorithm">
            {#snippet control(id, describedBy)}
              <select
                {id}
                aria-describedby={describedBy}
                value={kind}
                onchange={(e) => (kind = e.currentTarget.value)}
              >
                <option value="ed25519">Ed25519 · Recommended</option>
                <option value="rsa">RSA · 4096 bits</option>
              </select>
            {/snippet}
          </Field>
          <Field
            label="Passphrase"
            hint="Optional. A passphrase encrypts your private key at rest."
          >
            {#snippet control(id, describedBy)}
              <input
                {id}
                aria-describedby={describedBy}
                type="password"
                autocomplete="new-password"
                value={passphrase}
                oninput={(e) => (passphrase = e.currentTarget.value)}
              >
            {/snippet}
          </Field>
        {:else}
          <Field label="Private key">
            {#snippet control(id, describedBy)}
              <textarea
                {id}
                aria-describedby={describedBy}
                required
                class="code-input"
                rows={7}
                value={text}
                oninput={(e) => (text = e.currentTarget.value)}
                placeholder="-----BEGIN OPENSSH PRIVATE KEY-----"
              ></textarea>
            {/snippet}
          </Field>
          <label class="file-input"
            ><UploadSimple size={17} aria-hidden="true" />Choose private key
            file<input
              type="file"
              onchange={(e) => {
                void readFile(e.currentTarget.files?.[0]);
              }}
            ></label
          >
          <p class="helper">
            For encrypted keys, include the matching .pub key below to avoid
            needing to decrypt it during import.
          </p>
          <Field label="Public key (optional)">
            {#snippet control(id, describedBy)}
              <input
                {id}
                aria-describedby={describedBy}
                value={publicKey}
                oninput={(e) => (publicKey = e.currentTarget.value)}
                placeholder="ssh-ed25519 AAAA…"
              >
            {/snippet}
          </Field>
        {/if}
        <div class="security-note">
          <LockKey size={19} aria-hidden="true" />
          <p>
            Private key files are saved in Tabius’s application data directory
            with owner-only permissions on Linux and macOS. They are never saved
            in browser storage.
          </p>
        </div>
      {/if}
      {#if editor.type === "known"}
        <p class="helper">
          Paste entries from an OpenSSH known_hosts file. Only import identities
          you have verified independently.
        </p>
        <Field label="Known hosts entries">
          {#snippet control(id, describedBy)}
            <textarea
              {id}
              aria-describedby={describedBy}
              class="code-input"
              required
              rows={10}
              value={text}
              oninput={(e) => (text = e.currentTarget.value)}
            ></textarea>
          {/snippet}
        </Field>
        <label class="file-input"
          ><UploadSimple size={17} aria-hidden="true" />Choose file<input
            type="file"
            onchange={(e) => {
              void readFile(e.currentTarget.files?.[0]);
            }}
          ></label
        >
      {/if}
      {#if error}
        <div class="inline-error" role="alert">
          <Warning size={17} aria-hidden="true" />{error}
        </div>
      {/if}
      <div class="modal-actions">
        <button type="button" onclick={onClose}>Cancel</button>
        <button class="primary" type="submit">
          {#if busy}
            <CircleNotch class="spin" size={17} aria-hidden="true" />
          {:else}
            <Check size={17} aria-hidden="true" />
          {/if}
          {" "}{busy
            ? "Working…"
            : editor.type === "key"
              ? keyMode === "generate"
                ? "Generate key"
                : "Import key"
              : editor.type === "known"
                ? "Import"
                : "Save changes"}
        </button>
      </div>
    </fieldset>
  </form>
</Modal>
