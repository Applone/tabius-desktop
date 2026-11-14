import { expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";
import type { Workspace } from "./model";
import { SettingsView } from "./Settings";

const workspace: Workspace = {
  hosts: [],
  keys: [],
  snippets: [],
  forwards: [],
  history: [],
  settings: {
    autoReconnect: false,
    bell: false,
    optionAsMeta: true,
    emulator: "xterm-256color",
    font: "monospace",
    fontSize: 14,
    lineHeight: 1.2,
    theme: "Graphite",
    highlighting: true,
    detectOS: true,
    postQuantum: true,
  },
};

test("settings group terminal, session, and security on the left", () => {
  const view = SettingsView({
    workspace,
    settings: workspace.settings,
    syncing: true,
    onChange: () => {},
    onSyncApply: async () => {},
    onSyncBusy: () => {},
    isCurrent: () => true,
  });
  const [left, right] = view.props.children;
  const leftMarkup = renderToStaticMarkup(left);
  const rightMarkup = renderToStaticMarkup(right);
  for (const heading of [
    "Terminal",
    "Session behavior",
    "Connection security",
  ]) {
    expect(leftMarkup).toContain(`<h3>${heading}</h3>`);
    expect(rightMarkup).not.toContain(`<h3>${heading}</h3>`);
  }
  for (const heading of ["Configuration sync", "Migrate from Tabby-Web"]) {
    expect(rightMarkup).toContain(`<h3>${heading}</h3>`);
    expect(leftMarkup).not.toContain(`<h3>${heading}</h3>`);
  }
  expect(leftMarkup.match(/inert=""/g)).toHaveLength(3);
});

test("font preview recognizes system JetBrains Nerd Fonts and keeps the saved family", () => {
  const markup = renderToStaticMarkup(
    <SettingsView
      workspace={workspace}
      settings={{ ...workspace.settings, font: "JetBrains Mono" }}
      syncing={false}
      onChange={() => {}}
      onSyncApply={async () => {}}
      onSyncBusy={() => {}}
      isCurrent={() => true}
    />,
  );
  expect(markup).toContain('<option selected="">JetBrains Mono</option>');
  expect(markup).toContain(
    "font-family:&quot;JetBrains Mono&quot;, &quot;JetBrainsMono Nerd Font Mono&quot;, &quot;JetBrainsMono Nerd Font&quot;, monospace;",
  );
});
