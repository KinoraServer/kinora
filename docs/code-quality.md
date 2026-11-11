# Code quality checks

Every push and pull request runs free, automatic checks that look for bugs, risky patterns and code that will be hard
to maintain. They sit next to the existing checks in `ci.yml` (rustfmt, Clippy, `tsc`, tests, Prettier, commit
messages) and Android Lint in `android.yml`.

| Check | What it looks at | Workflow | Fails the build on |
| --- | --- | --- | --- |
| [SonarQube Cloud](https://sonarcloud.io) | Bugs, code smells, duplication, complexity and test coverage across Rust, TypeScript, Kotlin, Docker and shell, with a quality gate on new code | `sonarqube.yml` | A failed quality gate shows as its own check on the pull request |
| [Oxlint](https://oxc.rs/docs/guide/usage/linter) | `web/`: bug-prone code, React hook rules, accessibility, imports | `static-analysis.yml` | Errors. Warnings are shown as annotations |
| [Knip](https://knip.dev) | `web/`: unused files and dependencies, missing dependencies | `static-analysis.yml` | Unused files or dependencies. Unused exports are only listed |
| [cargo-deny](https://embarkstudios.github.io/cargo-deny/) | Rust crates: known vulnerabilities (RustSec), licences, crate sources | `static-analysis.yml` | Any advisory, a licence not in `deny.toml`, an unknown registry |
| [actionlint](https://github.com/rhysd/actionlint) | The GitHub workflows, including the shell inside them | `static-analysis.yml` | Any finding |
| [ShellCheck](https://www.shellcheck.net) | `scripts/*.sh` and the git hooks | `static-analysis.yml` | Warnings and errors |
| [Hadolint](https://github.com/hadolint/hadolint) | The `Dockerfile` | `static-analysis.yml` | Warnings and errors (exceptions in `.hadolint.yaml`) |

## Run them locally

```sh
cd web
pnpm lint        # Oxlint
pnpm knip        # unused files, dependencies and exports

cd ..
cargo install --locked cargo-deny   # once
cargo deny check
```

ShellCheck, Hadolint and actionlint are single binaries. Install them from your package manager (for example
`brew install shellcheck hadolint actionlint`) and run `shellcheck scripts/*.sh`, `hadolint Dockerfile` and
`actionlint`.

## SonarQube Cloud setup (once)

SonarQube Cloud is free for public repositories, so it needs no server. The self-hosted SonarQube Community Build would
need its own server and database, which a public repository doesn't need.

1. Sign in at [sonarcloud.io](https://sonarcloud.io) with GitHub and import the `KinoraServer` organization (this
   installs the SonarQube Cloud GitHub App on it). Choose the free plan.
2. Analyze new project → pick `kinora`. Check that the organization key and project key it shows match
   `sonar.organization` and `sonar.projectKey` in `sonar-project.properties`, and change the file if they don't.
3. In the project, go to **Administration → Analysis Method** and turn **Automatic Analysis off**. The workflow does
   the analysis, because only CI analysis can import Clippy findings and test coverage.
4. **My Account → Security** → generate a token. In GitHub, add it as the repository secret `SONAR_TOKEN`
   (Settings → Secrets and variables → Actions).
5. Optional: under **Administration → New Code**, choose "Previous version" or "Number of days". The quality gate
   only judges new code, so old findings never block a pull request.

Until `SONAR_TOKEN` exists, the SonarQube job skips the scan and passes. Pull requests from forks can't read secrets,
so they skip it too.

SonarQube Cloud adds a summary comment and a check to each pull request. The AI reviewer on the repo keeps reviewing as
before; SonarQube only adds a check and does not review or change code.

## Tuning

- **Oxlint**: rules are in `web/.oxlintrc.json`. Correctness rules are errors; "suspicious" and "perf" rules, and a few
  rules the existing code doesn't follow yet (React refs during render, `setState` in effects, some accessibility
  rules), are warnings. Turn a warning into an error once the code is clean of it.
- **Knip**: `web/knip.json`. Make `exports` and `types` errors once the unused exports are cleaned up.
- **cargo-deny**: `deny.toml`. A new licence or an accepted advisory goes in there with a reason.
- **SonarQube**: what's analyzed and the coverage reports are in `sonar-project.properties`. Rules and the quality gate
  are set in the SonarQube Cloud UI.
