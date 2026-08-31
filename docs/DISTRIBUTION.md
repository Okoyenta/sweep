# Distribution

How a tagged release reaches users, and the one-time setup needed to make
`winget install` and `scoop install` work.

## What happens on a `v*` tag

Pushing a tag like `v0.9.0` runs `.github/workflows/release.yml`:

| Job | Result |
| --- | --- |
| `build-linux` | builds on `ubuntu-latest`, attaches `sweep-linux-x64` |
| `build-windows` | builds on `windows-latest`, attaches `sweep-windows-x64.exe` |
| `manifests` | generates winget + scoop manifests, attaches them, uploads them as a workflow artifact |
| `publish-winget` | opens a PR against `microsoft/winget-pkgs` — **only if `WINGET_TOKEN` is set** |
| `publish-scoop` | commits the manifest to the scoop bucket repo — **only if `SCOOP_BUCKET_TOKEN` is set** |

Without the two secrets, the release still publishes both binaries and both
manifests; the publish jobs skip with a notice in the run summary. Nothing
fails. This is deliberate — attaching working binaries is the critical path,
and package-manager submission is best-effort on top of it.

## One-time setup

Neither publish job can work until these exist. They require account access, so
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
sets `PortableCommandAlias: sweep`. Without that field winget derives the
command name from the downloaded file name — worth remembering if the release
asset is ever renamed.

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
