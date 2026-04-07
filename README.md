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

## Quick start

```powershell
cargo run -p muks-cli -- status
cargo run -p muks-cli -- doctor
cargo run -p muks-cli -- tui
cargo run -p muks-cli --bin mukss
```

## Local install

```powershell
.\install.ps1
```

This installs `muks.exe` and `mukss.exe` into `%USERPROFILE%\.muks\bin`.
