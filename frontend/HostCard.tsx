import {
  LinuxLogo,
  PencilSimple,
  HardDrives as Server,
  Trash,
  WindowsLogo,
} from "@phosphor-icons/react";
import appleIcon from "./assets/tabius/apple.svg";
import type { Host } from "./model";

export function HostCard({
  host,
  onConnect,
  onEdit,
  onDelete,
}: {
  host: Host;
  onConnect: () => void;
  onEdit: () => void;
  onDelete: () => void;
}) {
  const system = host.system.toLowerCase();
  const isMac = /darwin|macos|mac os/.test(system);
  const Icon = /windows|mingw/.test(system)
    ? WindowsLogo
    : /linux|ubuntu|debian|arch|fedora|centos|alpine|redhat|rhel|rocky|alma|suse|gentoo|nixos/.test(
          system,
        )
      ? LinuxLogo
      : Server;
  return (
    <article className="host-card">
      <button
        type="button"
        className="host-open"
        aria-label={`Open session with ${host.name}`}
        title="Double-click to open a session"
        onDoubleClick={onConnect}
        onClick={(event) => {
          // Keyboard and assistive activation produce clicks without a pointer.
          if (event.detail === 0) onConnect();
        }}
      >
        <span
          className="host-icon"
          role="img"
          aria-label={host.system}
          title={host.system}
        >
          {isMac ? (
            <img src={appleIcon} alt="" width={24} height={24} />
          ) : (
            <Icon size={24} />
          )}
        </span>
        <span className="host-name">{host.name}</span>
        <span className="host-address">
          {host.address}
          <span className="host-port">:{host.port}</span>
        </span>
      </button>
      <div className="card-actions">
        <button
          type="button"
          className="icon-button"
          aria-label={`Edit ${host.name}`}
          onClick={onEdit}
        >
          <PencilSimple size={16} />
        </button>
        <button
          type="button"
          className="icon-button"
          aria-label={`Delete ${host.name}`}
          onClick={onDelete}
        >
          <Trash size={16} />
        </button>
      </div>
    </article>
  );
}
