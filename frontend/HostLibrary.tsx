import { Folder, PencilSimple, Plus, Trash } from "@phosphor-icons/react";
import { HostCard } from "./HostCard";
import type { Host, HostGroup, Workspace } from "./model";

export function HostLibrary({
  workspace,
  onConnect,
  onEdit,
  onDelete,
  onEditGroup,
  onDeleteGroup,
  onNewHost,
}: {
  workspace: Workspace;
  onConnect: (host: Host) => void;
  onEdit: (host: Host) => void;
  onDelete: (host: Host) => void;
  onEditGroup: (group: HostGroup) => void;
  onDeleteGroup: (group: HostGroup) => void;
  onNewHost: (groupId: string) => void;
}) {
  const groups = workspace.groups ?? [];
  const sections = [
    ...groups,
    { id: "", name: groups.length ? "Ungrouped" : "Hosts" },
  ];
  return (
    <>
      {sections.map((group) => {
        const hosts = workspace.hosts.filter((host) =>
          group.id
            ? host.groupId === group.id
            : !groups.some((g) => g.id === host.groupId),
        );
        if (!group.id && !hosts.length && groups.length) return null;
        return (
          <section className="host-group" key={group.id}>
            <div className="group-heading">
              <h2>
                {group.id && <Folder size={17} />}
                {group.name}
                <span className="group-count">{hosts.length}</span>
              </h2>
              {group.id && (
                <div className="group-actions">
                  <button
                    type="button"
                    className="icon-button"
                    aria-label={`Add host to ${group.name}`}
                    onClick={() => onNewHost(group.id)}
                  >
                    <Plus size={16} />
                  </button>
                  <button
                    type="button"
                    className="icon-button"
                    aria-label={`Rename ${group.name}`}
                    onClick={() => onEditGroup(group)}
                  >
                    <PencilSimple size={16} />
                  </button>
                  <button
                    type="button"
                    className="icon-button"
                    aria-label={`Delete group ${group.name}`}
                    onClick={() => onDeleteGroup(group)}
                  >
                    <Trash size={16} />
                  </button>
                </div>
              )}
            </div>
            {hosts.length ? (
              <div className="host-grid">
                {hosts.map((host) => (
                  <HostCard
                    key={host.id}
                    host={host}
                    onConnect={() => onConnect(host)}
                    onEdit={() => onEdit(host)}
                    onDelete={() => onDelete(host)}
                  />
                ))}
              </div>
            ) : (
              <p className="helper">
                No hosts in this group. Add a host or assign one in host
                details.
              </p>
            )}
          </section>
        );
      })}
    </>
  );
}
