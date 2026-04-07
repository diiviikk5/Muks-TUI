# Muks

Muks is a Windows desktop customization control plane for Windows 11.

It installs, detects, themes, and orchestrates the best existing desktop customization engines behind one config root:

- Lively Wallpaper
- Rainmeter
- YASB
- Komorebi
- Windhawk

## Current foundation

- `%USERPROFILE%\\.muks` config and generated state
- adapter registry for all five engines
- install planning and health detection
- theme token generation and output rendering
- snapshot and rollback primitives
- `muks` CLI
- `muks tui` terminal UI
- `mukss` interactive command shell with a branded startup banner
- live target sync for all adapters into managed paths under `%USERPROFILE%\\.muks\\live` (or detected tool paths)

## Quick start

```powershell
cargo run -p muks-cli -- status
cargo run -p muks-cli -- doctor
cargo run -p muks-cli -- tui
cargo run -p muks-cli --bin mukss
cargo run -p muks-cli -- theme apply graphite --best-effort
```

## TUI controls

Inside `muks tui`:

- `r` refresh adapter + profile state
- `d` doctor summary in activity log
- `i` generate install plan
- `a` apply full adapter sync (all five adapters)
- `x` run reinstall guidance for selected adapter
- `j/k` or arrow keys move adapter selection
- `q` quit

## Installer mode

```powershell
cargo run -p muks-cli -- install
cargo run -p muks-cli -- install --apply
```

## Local install

```powershell
.\install.ps1
```

This installs `muks.exe` and `mukss.exe` into `%USERPROFILE%\.muks\bin`.
