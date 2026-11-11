/** The folder that holds `path` (POSIX or Windows style), or "" when there is none. */
export function parentFolder(path: string): string {
  const trimmed = path.replace(/[/\\]+$/, "");
  const i = Math.max(trimmed.lastIndexOf("/"), trimmed.lastIndexOf("\\"));
  if (i < 0) return "";
  if (i === 0) return "/";
  const parent = trimmed.slice(0, i);
  // C:\Movies -> C:\ rather than C: (which means the current folder on that drive).
  return parent.endsWith(":") ? `${parent}\\` : parent;
}
