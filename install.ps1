param(
    [string]$Profile = "release"
)

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $MyInvocation.MyCommand.Path
$cargoArgs = @("build", "-p", "muks-cli")

if ($Profile -eq "release") {
    $cargoArgs += "--release"
}

Push-Location $workspace
try {
    cargo @cargoArgs

    $binaryDir = if ($Profile -eq "release") {
        Join-Path $workspace "target\release"
    } else {
        Join-Path $workspace "target\debug"
    }

    $installRoot = Join-Path $env:USERPROFILE ".muks\bin"
    New-Item -ItemType Directory -Force -Path $installRoot | Out-Null
    Copy-Item (Join-Path $binaryDir "muks.exe") (Join-Path $installRoot "muks.exe") -Force
    Copy-Item (Join-Path $binaryDir "mukss.exe") (Join-Path $installRoot "mukss.exe") -Force

    Write-Host "Installed muks.exe and mukss.exe to $installRoot"
    Write-Host "Add $installRoot to PATH if it is not already available."
} finally {
    Pop-Location
}
