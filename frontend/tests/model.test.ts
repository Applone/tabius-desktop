import { describe, expect, test } from "bun:test";
import {
  emptyWorkspace,
  historyToSnippets,
  newHost,
  upsert,
  validateHost,
} from "../model";

describe("host validation", () => {
  test("accepts DNS, IPv4 and IPv6 addresses", () => {
    for (const address of ["server.example.com", "127.0.0.1", "2001:db8::1"])
      expect(
        validateHost({ ...newHost(), name: "Server", address }),
      ).toBeUndefined();
  });
  test("rejects invalid ports and option injection", () => {
    for (const address of [
      "-oProxyCommand=bad",
      "ssh://host",
      "user@host",
      "host;id",
      "host\nother",
    ])
      expect(
        validateHost({ ...newHost(), name: "Server", address }),
      ).toBeDefined();
    for (const port of [0, 65536, 22.5, NaN])
      expect(
        validateHost({
          ...newHost(),
          name: "Server",
          address: "localhost",
          port,
        }),
      ).toBeDefined();
  });
});
describe("history import", () => {
  test("understands Bash and extended Zsh history", () => {
    expect(
      historyToSnippets("#1710000000\nls -la\n: 1710000001:0;pwd\nls -la\n"),
    ).toEqual(["pwd", "ls -la"]);
  });
  test("bounds imported history and ignores empty lines", () => {
    expect(historyToSnippets("\n \n# comment")).toEqual([]);
    expect(
      historyToSnippets(
        Array.from({ length: 700 }, (_, i) => `echo ${i}`).join("\n"),
      ),
    ).toHaveLength(500);
  });
});
test("upsert preserves other items and replaces existing IDs", () => {
  expect(
    upsert(
      [
        { id: "a", name: "first" },
        { id: "b", name: "second" },
      ],
      { id: "a", name: "updated" },
    ),
  ).toEqual([
    { id: "a", name: "updated" },
    { id: "b", name: "second" },
  ]);
});
test("new workspaces have no fabricated connections and independent defaults", () => {
  const a = emptyWorkspace();
  const b = emptyWorkspace();
  a.settings.bell = true;
  expect(b.settings.bell).toBe(false);
  expect(a.hosts).toEqual([]);
});
