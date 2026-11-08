import { expect, test } from "bun:test";
import { fileSize, joinPath, parentPath, validName } from "./SFTP";

test("file paths preserve spaces, quotes, Unicode, and root navigation", () => {
  expect(joinPath("/", "a 'quoted' 日本語 file")).toBe(
    "/a 'quoted' 日本語 file",
  );
  expect(joinPath("/srv/", "folder")).toBe("/srv/folder");
  expect(parentPath("/srv/folder/")).toBe("/srv");
  expect(parentPath("/")).toBe("/");
  expect(parentPath("/srv")).toBe("/");
  expect(parentPath("C:/Users")).toBe("C:/");
  expect(parentPath("C:/")).toBe("C:/");
  expect(joinPath("C:\\Users\\", "name")).toBe("C:\\Users/name");
});

test("file names exclude traversal and control characters", () => {
  for (const name of ["", " ", ".", "..", "a/b", "a\\b", "a\n", "a\0"])
    expect(validName(name)).toBe(false);
  for (const name of ["日本語", "a 'quoted' file", ".config", "[data]"])
    expect(validName(name)).toBe(true);
  expect(fileSize(0)).toBe("0 B");
  expect(fileSize(2048)).toBe("2.0 KB");
  expect(fileSize(1048576)).toBe("1.0 MB");
});
