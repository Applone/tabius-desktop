export interface FileEntry {
  name: string;
  directory: boolean;
  size: number;
  modified: number;
}
export function joinPath(path: string, name: string) {
  return `${path.replace(/[\\/]+$/, "")}/${name}`;
}
export function parentPath(path: string) {
  if (/^[A-Za-z]:[\\/]*$/.test(path)) return `${path.slice(0, 2)}/`;
  const parent =
    path.replace(/[\\/]+$/, "").replace(/[\\/][^\\/]*$/, "") || "/";
  return /^[A-Za-z]:$/.test(parent) ? `${parent}/` : parent;
}
export function validName(name: string) {
  return (
    name.trim().length > 0 &&
    name !== "." &&
    name !== ".." &&
    !/[\\/]/.test(name) &&
    !Array.from(name).some((character) => character.charCodeAt(0) < 32)
  );
}
export function fileSize(size: number) {
  if (size < 1024) return `${size} B`;
  if (size < 1048576) return `${(size / 1024).toFixed(1)} KB`;
  return `${(size / 1048576).toFixed(1)} MB`;
}
