import { useState } from "react";
import type { Host, Workspace } from "./model";
import { Field, Modal } from "./ui";

export function HostPicker({
  workspace,
  onChoose,
  onNewHost,
  onClose,
}: {
  workspace: Workspace;
  onChoose: (host: Host) => void;
  onNewHost: () => void;
  onClose: () => void;
}) {
  const [query, setQuery] = useState("");
  const groupName = (host: Host) =>
    workspace.groups?.find((group) => group.id === host.groupId)?.name ?? "";
  const matches = workspace.hosts.filter((host) =>
    `${host.name} ${host.address} ${groupName(host)}`
      .toLowerCase()
      .includes(query.trim().toLowerCase()),
  );
  return (
    <Modal title="Open a session" onClose={onClose}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (matches[0]) onChoose(matches[0]);
        }}
      >
        <Field label="Find a host">
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Name, address, or group"
          />
        </Field>
        <div className="host-picker-list">
          {matches.map((host) => (
            <button type="button" key={host.id} onClick={() => onChoose(host)}>
              <span className="picker-label">
                {host.name}
                <small>{groupName(host)}</small>
              </span>
              <span className="muted picker-label">
                {host.address}:{host.port}
              </span>
            </button>
          ))}
          {!matches.length && (
            <p className="helper">
              {workspace.hosts.length
                ? "No hosts match your search."
                : "Add a host to open your first session."}
            </p>
          )}
        </div>
        <button type="button" onClick={onNewHost}>
          New host
        </button>
      </form>
    </Modal>
  );
}
