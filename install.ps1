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
    Get-Process muks,mukss -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 300
    Copy-Item (Join-Path $binaryDir "muks.exe") (Join-Path $installRoot "muks.exe") -Force
    Copy-Item (Join-Path $binaryDir "mukss.exe") (Join-Path $installRoot "mukss.exe") -Force

    $currentUserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $pathParts = @()
    if ($currentUserPath) {
        $pathParts = $currentUserPath.Split(';', [System.StringSplitOptions]::RemoveEmptyEntries)
    }

    if ($pathParts -notcontains $installRoot) {
        $updatedPath = if ([string]::IsNullOrWhiteSpace($currentUserPath)) {
            $installRoot
        } else {
            "$currentUserPath;$installRoot"
        }
        [Environment]::SetEnvironmentVariable("Path", $updatedPath, "User")
    }

    if (($env:Path.Split(';', [System.StringSplitOptions]::RemoveEmptyEntries)) -notcontains $installRoot) {
        $env:Path = "$installRoot;$env:Path"
    }

    Write-Host "Installed muks.exe and mukss.exe to $installRoot"
    Write-Host "Added $installRoot to the current user PATH."
    Write-Host "Open a new terminal and run: muks"
} finally {
    Pop-Location
}
