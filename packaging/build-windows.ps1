# SPDX-License-Identifier: MIT
# Copyright (c) 2026 insideTheFlux
# Builds the Windows 11 program into dist\: rfcps.exe and a .zip with the license. Run it in PowerShell from the
# project folder, on a Windows computer with Rust (https://rustup.rs) and the "Desktop development with C++" tools
# (Visual Studio Build Tools):
#   powershell -ExecutionPolicy Bypass -File packaging\build-windows.ps1
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")
$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.*)"' | Select-Object -First 1).Matches.Groups[1].Value
cargo build --release -p rfcps
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
New-Item -ItemType Directory -Force dist | Out-Null
$stage = "dist\rfcps-$version-windows-x86_64"
Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item target\release\rfcps.exe $stage
Copy-Item LICENSE, NOTICE.md, README.md $stage
$zip = "$stage.zip"
Remove-Item -Force $zip -ErrorAction SilentlyContinue
Compress-Archive -Path "$stage\*" -DestinationPath $zip
Write-Host $zip
