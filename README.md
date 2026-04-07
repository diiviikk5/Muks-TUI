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
- install planning persists JSON reports at `%USERPROFILE%\\.muks\\logs\\install-report.json`
- `muks install --apply` now attempts official upstream installers even when `winget` is unavailable
- theme token generation and output rendering
- snapshot and rollback primitives with adapter backup/restore
- `muks` CLI
- `muks tui` terminal UI
- `mukss` interactive command shell with a branded startup banner
- live target sync for all adapters into managed paths under `%USERPROFILE%\\.muks\\live` (or detected tool paths)
- adapter enable flags respected during apply/render pipelines
- preset files in `%USERPROFILE%\\.muks\\presets\\*.toml` (graphite, forest, rose, cyber, nebula)

## Quick start

```powershell
cargo run -p muks-cli -- status
cargo run -p muks-cli -- doctor
cargo run -p muks-cli -- doctor --repair
cargo run -p muks-cli -- tui
cargo run -p muks-cli --bin mukss
cargo run -p muks-cli -- theme apply graphite --best-effort
cargo run -p muks-cli -- theme apply cyber --best-effort
```

## TUI controls

Inside `muks tui`:

- `r` refresh adapter + profile state
- `d` doctor summary in activity log
- `f` run repair flow (doctor + install apply)
- `i` generate install plan
- `I` attempt installer execution (`winget` + official guidance fallback)
- `a` apply full adapter sync (all five adapters)
- `1-5` quick-apply built-in presets (`graphite`, `forest`, `rose`, `cyber`, `nebula`)
- `p` apply selected adapter only
- `e` toggle selected adapter enabled/disabled
- `x` run reinstall guidance for selected adapter
- `s` create snapshot
- `u` rollback latest snapshot and re-apply adapters
- `j/k` or arrow keys move adapter selection
- `q` quit

## Installer mode

```powershell
cargo run -p muks-cli -- install
cargo run -p muks-cli -- install --apply
cargo run -p muks-cli -- bar reload
cargo run -p muks-cli -- widgets reload
cargo run -p muks-cli -- tile start
cargo run -p muks-cli -- mod apply curated
cargo run -p muks-cli -- watch --iterations 100 --interval-ms 500
cargo run -p muks-cli -- adapter configure yasb --enabled false
```

`install --apply` may still require elevated permissions on some systems because upstream installers can enforce admin scope.

## Local install

```powershell
.\install.ps1
```

This installs `muks.exe` and `mukss.exe` into `%USERPROFILE%\.muks\bin`.
