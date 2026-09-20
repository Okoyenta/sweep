# sweep

A lightweight, cross-platform (Windows + Linux) TUI/CLI utility that monitors
RAM and disk usage, indexes files and installed apps in the background, detects
unused items, and helps you free space and memory on demand.

```console
npm install -g @okoyenta/sweep
sweep doctor
```

This package is a small launcher. The binary itself ships in a platform-specific
optional dependency (`@okoyenta/sweep-win32-x64` or `@okoyenta/sweep-linux-x64`),
so npm downloads only the one matching your machine and there is no install-time
download script.

Requires Node 18 or newer — only to run the launcher; `sweep` itself is a native
binary.

Full command reference, safety model and architecture:
**<https://github.com/Okoyenta/sweep>**

## Supported platforms

| Platform | Package |
| --- | --- |
| Windows x64 | `@okoyenta/sweep-win32-x64` |
| Linux x64 | `@okoyenta/sweep-linux-x64` |

Other targets install the launcher but exit with a clear message pointing you at
building from source. There is no ARM build yet.

## Uninstall

```console
npm uninstall -g @okoyenta/sweep
```

## A note on `sweep --version`

The version printed is the native binary's, which always matches the package
version — both are stamped from the same release tag.
