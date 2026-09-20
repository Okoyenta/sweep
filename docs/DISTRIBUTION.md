# Distribution

How a tagged release reaches users, and the one-time setup needed to make
`winget install`, `scoop install` and `npm install -g` work.

## What happens on a `v*` tag

Pushing a tag like `v0.9.0` runs `.github/workflows/release.yml`:

| Job | Result |
| --- | --- |
| `build-linux` | builds on `ubuntu-latest`, attaches `sweep-linux-x64` |
| `build-windows` | builds on `windows-latest`, attaches `sweep-windows-x64.exe` |
| `manifests` | generates winget + scoop manifests, attaches them, uploads them as a workflow artifact |
| `publish-winget` | opens a PR against `microsoft/winget-pkgs` — **only if `WINGET_TOKEN` is set** |
| `publish-scoop` | commits the manifest to the scoop bucket repo — **only if `SCOOP_BUCKET_TOKEN` is set** |
| `publish-npm` | publishes the launcher + two platform packages to npm — **only if `NPM_TOKEN` is set** |

Without the three secrets, the release still publishes both binaries and both
manifests; the publish jobs skip with a notice in the run summary. Nothing
fails. This is deliberate — attaching working binaries is the critical path,
and package-manager submission is best-effort on top of it.

> Re-running a release job for a tag that already published will fail with a
> "version already exists" error from npm. That is caught by the same
> `continue-on-error` policy and does not fail the release.

## One-time setup

No publish job can work until these exist. They require account access, so
they have to be done by hand.

### 1. winget (`WINGET_TOKEN`)

1. Fork `microsoft/winget-pkgs` to your account. `wingetcreate` pushes its
   branch to that fork and opens the PR from there.
2. Create a classic PAT with the **`public_repo`** scope.
3. Add it to this repo as the secret `WINGET_TOKEN`
   (Settings → Secrets and variables → Actions).

That is the only setup step. The workflow runs `wingetcreate submit` against
the manifests it already generated, which works whether or not `Okoyenta.Sweep`
exists in winget-pkgs yet — so the first release needs no manual submission.

Expect the first PR to take a few days — winget-pkgs runs automated validation
and a human review. An unsigned portable exe is accepted, but SmartScreen may
warn users until the binary builds reputation. Later tags open a new PR each
time, automatically.

### 2. scoop (`SCOOP_BUCKET_TOKEN`)

1. Create a bucket repo. The workflow defaults to `Okoyenta/sweep-bucket`; set
   the repository **variable** `SCOOP_BUCKET_REPO` to override
   (e.g. `Okoyenta/scoop-bucket`).
2. Give it a `bucket/` directory — that layout is what `scoop bucket add`
   expects.
3. Create a PAT with **`repo`** scope that can push to it, and add it here as
   the secret `SCOOP_BUCKET_TOKEN`.

Users then install with:

```console
scoop bucket add sweep https://github.com/Okoyenta/sweep-bucket
scoop install sweep
```

Unlike winget, there is no review queue — the first tagged release after setup
publishes immediately.

### 3. npm (`NPM_TOKEN`)

The packages live under the `@okoyenta` scope, so publishing needs an account
that owns it. The unscoped name `sweep` is **taken** by an unrelated package
(`bredele/sweep`, v0.1.0) — hence the scope.

1. `npm login` as an account named `okoyenta`, or one that is a member of an
   `okoyenta` org with write access.
2. Create an **Automation** token (npmjs.com → Access Tokens). Automation
   tokens are the ones that bypass 2FA prompts, which is what CI needs.
3. Add it here as the secret `NPM_TOKEN`.

Users then install with:

```console
npm install -g @okoyenta/sweep
```

## The npm packages

Three packages, all published from `npm/`:

| Directory | Package | Contents |
| --- | --- | --- |
| `npm/sweep` | `@okoyenta/sweep` | the launcher `bin/sweep.js` — this is what users install |
| `npm/sweep-win32-x64` | `@okoyenta/sweep-win32-x64` | `sweep.exe` |
| `npm/sweep-linux-x64` | `@okoyenta/sweep-linux-x64` | `sweep` |

The launcher is a short Node script that resolves the platform-matching package
and runs its binary, forwarding stdout/stderr and the exit code. The binary
arrives as an **optional dependency** gated by each package's `os` and `cpu`
fields, so npm fetches only the matching one and **there is no install-time
download script** — the layout esbuild and biome use. That matters because a
download script breaks behind strict proxies and in offline installs; here
`npm install` only ever talks to the registry.

No binary is committed: the `publish-npm` job downloads both release assets into
the platform directories before packing, so what npm ships is byte-identical to
what GitHub ships.

**Versions are stamped from the tag.** All three `package.json` files carry
`0.0.0` placeholders, and the job rewrites them — plus the launcher's
`optionalDependencies` — from the tag name. Keeping a second version number in
sync with `Cargo.toml` by hand is a thing that gets forgotten.

`engines.node` is `>=18`, and Node is needed only to run the launcher; `sweep`
itself is a static native binary.

**Not covered:** there is no ARM build, so `win32-arm64` and `linux-arm64`
install the launcher and then exit with a message pointing at building from
source.

## Does it end up on PATH?

**Yes — both package managers handle `PATH` for you.** This is why sweep ships
as a portable package rather than an installer.

**winget.** A `portable` package is unpacked to
`%LOCALAPPDATA%\Microsoft\WinGet\Packages\...` and a shim is created in
`%LOCALAPPDATA%\Microsoft\WinGet\Links\`, which winget itself adds to your
**user** `PATH`. So this works with no manual step:

```console
winget install Okoyenta.Sweep
sweep doctor
```

The command is `sweep`, not `sweep-windows-x64`, because the installer manifest
sets `Commands: [sweep]`. For a single-file portable winget resolves the alias
from `Commands`, then `--rename`, then the downloaded file's name.
`PortableCommandAlias` does **not** apply here — winget only reads that field
from `NestedInstallerFiles`, i.e. for archive-based portables, and its validator
reports it as an unknown field on a single-file portable. Worth remembering if
the release asset is ever renamed.

**scoop.** Shims go in `~/scoop/shims`, which is on `PATH` from the moment
scoop is installed. The `bin` entry in the manifest maps the exe to `sweep`.

Two caveats either way:

- **Open a new terminal after installing.** An already-running shell keeps its
  old `PATH`; the change is not picked up retroactively.
- Uninstalling (`winget uninstall Okoyenta.Sweep` / `scoop uninstall sweep`)
  removes the shim, so `PATH` stays clean.

## Current limitations

- **No installer.** Both package managers install in portable mode. `PATH` is
  handled (above), but there is no Start Menu entry, no Add/Remove Programs
  entry, and no Group Policy / Intune deployment. Those are the only reasons to
  add a real MSI (WiX) or Inno Setup installer, and none of them apply to
  installing a CLI tool for yourself.
- **No code signing.** The binary is unsigned, so Windows SmartScreen may warn
  on direct download until it accrues reputation. Signing needs a certificate.
- **No uninstaller.** `sweep self-uninstall` is still in the ROADMAP backlog;
  removal is `scoop uninstall sweep`, `winget uninstall Okoyenta.Sweep`, or
  deleting the exe.
- **No Linux packaging.** `sweep-linux-x64` is a bare binary — no `.deb`,
  `.rpm`, or AUR package. Users download it and `chmod +x`.

## Manual install (works today, no setup required)

```console
# Windows
curl -L -o sweep.exe https://github.com/Okoyenta/sweep/releases/latest/download/sweep-windows-x64.exe

# Linux
curl -L -o sweep https://github.com/Okoyenta/sweep/releases/latest/download/sweep-linux-x64
chmod +x sweep
```

Then move it somewhere on your `PATH`.
