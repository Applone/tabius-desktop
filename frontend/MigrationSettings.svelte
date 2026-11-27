<script lang="ts">
  import { CircleNotch, CloudArrowUp } from "phosphor-svelte";
  import { native } from "./api";
  import Field from "./Field.svelte";
  import ServerFields from "./ServerFields.svelte";
  import type { RemoteConfiguration } from "./sync";

  interface Connection {
    server: string;
    token: string;
    configurationId?: number;
    configurations: RemoteConfiguration[];
  }
  interface Preview {
    fingerprint: string;
    profiles: number;
    secrets: number;
    fingerprints: number;
    destinationHasContent: boolean;
  }

  let {
    disabled,
    onBusy,
  }: {
    disabled: boolean;
    onBusy: (busy: boolean) => void;
  } = $props();

  let source = $state.raw<Connection>({
    server: "",
    token: "",
    configurations: [],
  });
  let destination = $state.raw<Connection>({
    server: "",
    token: "",
    configurations: [],
  });
  let sourcePassword = $state("");
  let password = $state("");
  let preview = $state.raw<Preview | null>(null);
  let busy = $state(false);
  let error = $state("");
  let status = $state("");
  function reset() {
    preview = null;
    error = "";
    status = "";
  }
  async function run(action: () => Promise<void>) {
    busy = true;
    onBusy(true);
    error = "";
    status = "";
    try {
      await action();
    } catch (err) {
      preview = null;
      error = String(err).replace(/^Error: /, "");
    } finally {
      busy = false;
      onBusy(false);
    }
  }
  async function load(which: "source" | "destination") {
    const connection = which === "source" ? source : destination;
    const configurations = await native<RemoteConfiguration[]>(
      "list_configurations",
      { server: connection.server, token: connection.token },
    );
    const updated = {
      ...connection,
      configurations,
      configurationId:
        configurations.length === 1 ? configurations[0].id : undefined,
    };
    if (which === "source") source = updated;
    else destination = updated;
    preview = null;
    if (!configurations.length)
      status = `No ${which} configurations found. Create one on that server first.`;
  }
  async function submit() {
    const result = await native<Preview | { migrated: true }>(
      "migrate_configuration",
      {
        source: {
          server: source.server,
          token: source.token,
          configurationId: source.configurationId,
        },
        destination: {
          server: destination.server,
          token: destination.token,
          configurationId: destination.configurationId,
        },
        sourcePassword,
        password,
        fingerprint: preview?.fingerprint ?? null,
      },
    );
    if ("migrated" in result) {
      preview = null;
      sourcePassword = "";
      password = "";
      source = { ...source, token: "" };
      destination = { ...destination, token: "" };
      status =
        "Migration complete. The encrypted copy was verified on the Tabius sync server. Connect Configuration sync to that server using the new vault password.";
    } else {
      preview = result;
    }
  }
</script>

<section class="settings-section sync-section migration-section">
  <div class="settings-section-heading">
    <CloudArrowUp size={20} aria-hidden="true" />
    <h3>Migrate from Tabby-Web</h3>
  </div>
  <p class="helper">
    Transfer your configuration from Tabby-Web to a Tabius sync server. Your old
    vault is decrypted on this device, then the complete copy is encrypted with
    Argon2id and XChaCha20-Poly1305 before upload. Profiles, keys, saved
    secrets, and fingerprints are preserved. Tabby-Web stays unchanged. Create a
    configuration on the Tabius server before starting.
  </p>
  <fieldset disabled={busy || disabled}>
    <ServerFields
      label="Tabby-Web server"
      connection={source}
      onChange={(value) => {
        source = value;
        reset();
      }}
      onLoad={() => void run(() => load("source"))}
    />
    <Field
      label="Old Tabby-Web vault password"
      hint="Leave empty if the legacy configuration is unencrypted."
    >
      {#snippet control(id, describedBy)}
        <input
          {id}
          aria-describedby={describedBy}
          type="password"
          autocomplete="off"
          value={sourcePassword}
          oninput={(e) => {
            sourcePassword = e.currentTarget.value;
            reset();
          }}
        >
      {/snippet}
    </Field>
    <ServerFields
      label="Tabius sync server"
      connection={destination}
      onChange={(value) => {
        destination = value;
        reset();
      }}
      onLoad={() => void run(() => load("destination"))}
    />
    <Field
      label="New Tabius vault password"
      hint="Required. Encrypts the full copy; tokens and passwords are not saved."
    >
      {#snippet control(id, describedBy)}
        <input
          {id}
          aria-describedby={describedBy}
          type="password"
          autocomplete="new-password"
          value={password}
          oninput={(e) => {
            password = e.currentTarget.value;
            reset();
          }}
        >
      {/snippet}
    </Field>
    {#if preview}
      <div class="form-section helper" role="status">
        <p class="helper">
          Ready to copy {preview.profiles} profiles, {preview.secrets} vault
          secrets, and {preview.fingerprints} host fingerprints.
        </p>
        <p>
          {preview.destinationHasContent
            ? "This replaces all data in the selected destination configuration."
            : "This writes to the selected empty destination configuration."}{" "}Clients
          using the destination must support Tabius version 2 encryption. Legacy
          Tabby clients cannot read the new copy.
        </p>
      </div>
    {/if}
    <button
      type="button"
      class="primary"
      disabled={!source.configurationId ||
        !destination.configurationId ||
        !source.token.trim() ||
        !destination.token.trim() ||
        !password}
      onclick={() => void run(submit)}
    >
      {#if busy}
        <CircleNotch class="spin" size={17} aria-hidden="true" />
      {/if}
      {preview
        ? preview.destinationHasContent
          ? "Migrate and replace destination"
          : "Migrate to Tabius"
        : "Preview migration"}
    </button>
  </fieldset>
  {#if busy}
    <p role="status">Contacting sync servers…</p>
  {/if}
  {#if error}
    <p class="inline-error" role="alert">{error}</p>
  {/if}
  {#if status}
    <p role="status">{status}</p>
  {/if}
</section>
