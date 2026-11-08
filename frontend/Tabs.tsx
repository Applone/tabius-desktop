import {
  ArrowsLeftRight,
  Books,
  Plus,
  SidebarSimple,
  Terminal,
  X,
} from "@phosphor-icons/react";
import { useRef, useState } from "react";
import type { Session } from "./Terminal";

export function reorderTabs<T extends { id: string }>(
  items: T[],
  source: string,
  target: string,
  after: boolean,
): T[] {
  if (
    source === target ||
    !items.some((item) => item.id === source) ||
    !items.some((item) => item.id === target)
  )
    return items;
  const next = items.filter((item) => item.id !== source);
  const index = next.findIndex((item) => item.id === target) + Number(after);
  const moved = items.find((item) => item.id === source);
  if (moved) next.splice(index, 0, moved);
  return next;
}
export function adjacentTab(items: { id: string }[], closing: string): string {
  const index = items.findIndex((item) => item.id === closing);
  return (items[index + 1] ?? items[index - 1])?.id ?? "library";
}
export function cycleTab(
  items: { id: string }[],
  active: string,
  direction: number,
): string {
  const ids = ["library", "sftp", ...items.map((item) => item.id)];
  const index = Math.max(0, ids.indexOf(active));
  return ids[(index + direction + ids.length) % ids.length];
}

export function Tabs({
  sessions,
  active,
  disabled,
  onSelect,
  onClose,
  onNew,
  onReorder,
  onTools,
  toolsOpen,
}: {
  onTools: () => void;
  toolsOpen: boolean;
  sessions: Session[];
  active: string;
  disabled: boolean;
  onSelect: (id: string, focusTerminal?: boolean) => void;
  onClose: (id: string) => void;
  onNew: () => void;
  onReorder: (source: string, target: string, after: boolean) => void;
}) {
  const dragged = useRef<string | undefined>(undefined);
  const [drop, setDrop] = useState<{ id: string; after: boolean }>();
  const items = [
    { id: "library", label: "Library" },
    { id: "sftp", label: "SFTP" },
    ...sessions.map((session) => ({
      id: session.id,
      label: `${session.host.name}${session.forward ? " · Tunnel" : ""}`,
    })),
  ];
  return (
    <div className="tabbar" inert={disabled}>
      <div className="tab-list" role="tablist" aria-label="Workspace tabs">
        {[
          { kind: "navigation", items: items.slice(0, 2), offset: 0 },
          { kind: "session", items: items.slice(2), offset: 2 },
        ].map((group) => (
          <div key={group.kind} className={`${group.kind}-tabs`}>
            {group.items.map((item, groupIndex) => (
              <div
                key={item.id}
                role="tab"
                aria-selected={active === item.id}
                aria-label={item.label}
                title={item.label}
                tabIndex={active === item.id ? 0 : -1}
                className={`session-tab ${item.id === "library" || item.id === "sftp" ? "navigation-tab" : ""} ${active === item.id ? "active" : ""} ${drop?.id === item.id ? (drop.after ? "drop-after" : "drop-before") : ""}`}
                draggable={item.id !== "library" && item.id !== "sftp"}
                onClick={() => onSelect(item.id)}
                onKeyDown={(event) => {
                  if (event.target !== event.currentTarget) return;
                  const index = group.offset + groupIndex;
                  let next: number | undefined;
                  if (event.key === "ArrowRight")
                    next = (index + 1) % items.length;
                  if (event.key === "ArrowLeft")
                    next = (index - 1 + items.length) % items.length;
                  if (event.key === "Home") next = 0;
                  if (event.key === "End") next = items.length - 1;
                  if (event.key === "Enter" || event.key === " ") {
                    event.preventDefault();
                    onSelect(item.id);
                  }
                  if (
                    event.key === "Delete" &&
                    item.id !== "library" &&
                    item.id !== "sftp"
                  ) {
                    event.preventDefault();
                    onClose(item.id);
                  }
                  if (next !== undefined) {
                    event.preventDefault();
                    const target = items[next];
                    onSelect(target.id, false);
                    const elements = event.currentTarget
                      .closest('[role="tablist"]')
                      ?.querySelectorAll<HTMLElement>('[role="tab"]');
                    elements?.[next]?.focus();
                  }
                }}
                onDragStart={(event) => {
                  if (
                    item.id === "library" ||
                    item.id === "sftp" ||
                    (event.target instanceof window.Element &&
                      event.target.closest("button"))
                  ) {
                    event.preventDefault();
                    return;
                  }
                  dragged.current = item.id;
                  event.dataTransfer.effectAllowed = "move";
                  event.dataTransfer.setData(
                    "application/x-tabius-tab",
                    item.id,
                  );
                }}
                onDragOver={(event) => {
                  if (
                    !dragged.current ||
                    item.id === "library" ||
                    item.id === "sftp" ||
                    dragged.current === item.id
                  )
                    return;
                  event.preventDefault();
                  event.dataTransfer.dropEffect = "move";
                  const rect = event.currentTarget.getBoundingClientRect();
                  setDrop({
                    id: item.id,
                    after: event.clientX > rect.left + rect.width / 2,
                  });
                }}
                onDrop={(event) => {
                  event.preventDefault();
                  if (
                    dragged.current &&
                    item.id !== "library" &&
                    item.id !== "sftp"
                  ) {
                    const rect = event.currentTarget.getBoundingClientRect();
                    onReorder(
                      dragged.current,
                      item.id,
                      event.clientX > rect.left + rect.width / 2,
                    );
                  }
                  dragged.current = undefined;
                  setDrop(undefined);
                }}
                onDragEnd={() => {
                  dragged.current = undefined;
                  setDrop(undefined);
                }}
              >
                {item.id === "library" ? (
                  <Books size={17} />
                ) : item.id === "sftp" ? (
                  <ArrowsLeftRight size={17} />
                ) : (
                  <Terminal size={15} />
                )}
                <span className="tab-title">{item.label}</span>
                {item.id !== "library" && item.id !== "sftp" && (
                  <button
                    type="button"
                    className="icon-button"
                    aria-label={`Close ${item.label}`}
                    onPointerDown={(event) => event.stopPropagation()}
                    onClick={(event) => {
                      event.stopPropagation();
                      onClose(item.id);
                    }}
                  >
                    <X size={13} />
                  </button>
                )}
              </div>
            ))}
          </div>
        ))}
        <button
          type="button"
          className="icon-button new-tab"
          aria-label="New tab"
          title="New tab"
          onClick={onNew}
        >
          <Plus size={17} />
        </button>
      </div>
      <button
        type="button"
        className="icon-button tools-toggle"
        aria-label="Toggle session tools"
        aria-pressed={toolsOpen}
        onClick={onTools}
      >
        <SidebarSimple size={19} />
      </button>
    </div>
  );
}
