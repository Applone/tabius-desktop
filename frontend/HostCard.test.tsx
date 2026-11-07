import { expect, mock, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";
import { HostCard } from "./HostCard";
import type { Host } from "./model";

const host: Host = {
  id: "production",
  name: "Production",
  address: "prod.example.com",
  port: 2222,
  username: "alice",
  keyId: "",
  agentForwarding: false,
  startup: "",
  jump: "",
  proxy: "",
  env: "",
  utf8: true,
  mosh: false,
  system: "Server",
};

function card() {
  const onConnect = mock(() => {});
  const onEdit = mock(() => {});
  const onDelete = mock(() => {});
  const element = HostCard({ host, onConnect, onEdit, onDelete });
  const [open, actions] = element.props.children;
  return { element, open, actions, onConnect, onEdit, onDelete };
}

test("a pointer double-click opens exactly one session", () => {
  const { open, onConnect } = card();
  open.props.onClick({ detail: 1 });
  expect(onConnect).not.toHaveBeenCalled();
  open.props.onClick({ detail: 2 });
  open.props.onDoubleClick();
  expect(onConnect).toHaveBeenCalledTimes(1);
});

test("keyboard and assistive activation opens a session", () => {
  const { open, onConnect } = card();
  expect(open.type).toBe("button");
  open.props.onClick({ detail: 0 });
  expect(onConnect).toHaveBeenCalledTimes(1);
});

test("edit and delete controls are outside the session button", () => {
  const { element, actions, onConnect, onEdit, onDelete } = card();
  const [edit, remove] = actions.props.children;
  edit.props.onClick();
  remove.props.onClick();
  expect(onEdit).toHaveBeenCalledTimes(1);
  expect(onDelete).toHaveBeenCalledTimes(1);
  expect(onConnect).not.toHaveBeenCalled();
  expect(element.props.onDoubleClick).toBeUndefined();
});

test("host cards show connection details without the old footer", () => {
  const markup = renderToStaticMarkup(card().element);
  expect(markup).toContain("Production");
  expect(markup).toContain("prod.example.com");
  expect(markup).toContain(":2222");
  expect(markup).toContain('aria-label="Open session with Production"');
  expect(markup).not.toContain("Connect");
  expect(markup).not.toContain(">Server<");
  expect(markup).not.toContain("card-bottom");
});
