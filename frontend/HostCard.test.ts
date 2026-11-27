import { expect, mock, test } from "bun:test";
import { flushSync, mount, unmount } from "svelte";
import appleIcon from "./assets/tabius/apple.svg";
import HostCard from "./HostCard.svelte";
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

function card(system = host.system) {
  const onConnect = mock(() => {});
  const onEdit = mock(() => {});
  const onDelete = mock(() => {});
  const target = document.createElement("div");
  const component = mount(HostCard, {
    target,
    props: { host: { ...host, system }, onConnect, onEdit, onDelete },
  });
  flushSync();
  const article = target.querySelector("article");
  const open = article?.querySelector<HTMLButtonElement>(".host-open");
  if (!article || !open)
    throw new Error("Host card is missing its session button");
  return {
    article,
    open,
    onConnect,
    onEdit,
    onDelete,
    dispose: () => unmount(component),
  };
}

test("a pointer double-click opens exactly one session", async () => {
  const { open, onConnect, dispose } = card();
  open.dispatchEvent(
    new window.MouseEvent("click", { detail: 1, bubbles: true }),
  );
  open.dispatchEvent(
    new window.MouseEvent("click", { detail: 2, bubbles: true }),
  );
  expect(onConnect).not.toHaveBeenCalled();
  open.dispatchEvent(
    new window.MouseEvent("dblclick", { detail: 2, bubbles: true }),
  );
  expect(onConnect).toHaveBeenCalledTimes(1);
  await dispose();
});

test("keyboard and assistive activation opens a session", async () => {
  const { open, onConnect, dispose } = card();
  open.dispatchEvent(
    new window.MouseEvent("click", { detail: 0, bubbles: true }),
  );
  expect(onConnect).toHaveBeenCalledTimes(1);
  await dispose();
});

test("edit and delete controls are outside the session button", async () => {
  const { article, open, onConnect, onEdit, onDelete, dispose } = card();
  const edit = article.querySelector<HTMLButtonElement>(
    '[aria-label="Edit Production"]',
  );
  const remove = article.querySelector<HTMLButtonElement>(
    '[aria-label="Delete Production"]',
  );
  expect(open.contains(edit)).toBe(false);
  expect(open.contains(remove)).toBe(false);
  edit?.click();
  remove?.click();
  expect(onEdit).toHaveBeenCalledTimes(1);
  expect(onDelete).toHaveBeenCalledTimes(1);
  expect(onConnect).not.toHaveBeenCalled();
  article.dispatchEvent(new window.MouseEvent("dblclick", { bubbles: true }));
  expect(onConnect).not.toHaveBeenCalled();
  await dispose();
});

test("host cards show connection details without the old footer", async () => {
  const { article, open, dispose } = card();
  expect(article.textContent).toContain("Production");
  expect(article.textContent).toContain("prod.example.com");
  expect(article.textContent).toContain(":2222");
  expect(open.getAttribute("aria-label")).toBe("Open session with Production");
  expect(article.textContent).not.toContain("Connect");
  expect(article.textContent).not.toContain("Server");
  expect(article.querySelector(".card-bottom")).toBeNull();
  await dispose();
});

test("detected host systems choose their OS logo", async () => {
  const icons = new Set<string>();
  for (const system of ["ubuntu", "Darwin", "Windows", "Server"]) {
    const { article, dispose } = card(system);
    const icon = article.querySelector(".host-icon");
    expect(icon?.getAttribute("aria-label")).toBe(system);
    if (icon) icons.add(icon.innerHTML);
    await dispose();
  }
  expect(icons.size).toBe(4);
});

test("macOS host cards display the Apple mark with an accessible system label", async () => {
  const { article, dispose } = card("macOS");
  const img = article.querySelector<HTMLImageElement>(
    '[role="img"][aria-label="macOS"] img',
  );
  expect(img?.getAttribute("src")).toBe(appleIcon);
  expect(img?.alt).toBe("");
  await dispose();
});
