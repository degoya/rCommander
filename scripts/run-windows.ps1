# The Windows half of scripts/run-windows.sh for a WSL that mounts C: read-only: copies the
# rcommander.exe WSL built to %LOCALAPPDATA%\rcommander-dev\ and starts it. run-windows.sh prints
# this command with the paths filled in when it cannot write there itself.
#
# Usage (PowerShell on Windows):
#   powershell -NoProfile -ExecutionPolicy Bypass -File <this script> -Binary <rcommander.exe> [<arguments>]
#
# A running rcommander.exe locks the file: close it first, or the copy fails.
param(
    [Parameter(Mandatory = $true)][string]$Binary,
    [Parameter(ValueFromRemainingArguments = $true)][string[]]$Arguments
)
$ErrorActionPreference = 'Stop'

if (-not (Test-Path -LiteralPath $Binary -PathType Leaf)) { throw "no such file: $Binary" }
$destination = Join-Path $env:LOCALAPPDATA 'rcommander-dev'
New-Item -ItemType Directory -Force -Path $destination | Out-Null
$exe = Join-Path $destination 'rcommander.exe'
Copy-Item -LiteralPath $Binary -Destination $exe -Force

Write-Host "==> starting $exe $Arguments"
if ($Arguments) {
    Start-Process -FilePath $exe -ArgumentList $Arguments
} else {
    Start-Process -FilePath $exe
}
Write-Host '==> started'
