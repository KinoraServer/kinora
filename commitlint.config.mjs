// Conventional Commits: https://www.conventionalcommits.org
//   feat(web): add a library icon picker
//   fix(server): stop transcodes when the client disconnects
//   chore(deps): bump vite to 7.1
// The scope is optional. Use a crate name, `web`, or one of the extra scopes below.
import { readdirSync } from "node:fs";

const crates = readdirSync(new URL("./crates/", import.meta.url), { withFileTypes: true })
  .filter((d) => d.isDirectory())
  .map((d) => d.name);

export default {
  extends: ["@commitlint/config-conventional"],
  rules: {
    // Warn (not block) on unknown scopes, so a commit that adds a new crate or area still goes through.
    "scope-enum": [
      1,
      "always",
      [...crates, "web", "player", "docker", "packaging", "unraid", "docs", "ci", "deps", "hooks", "repo", "android"],
    ],
    // Bodies often hold pasted logs or URLs.
    "body-max-line-length": [1, "always", 100],
    "footer-max-line-length": [1, "always", 100],
  },
};
