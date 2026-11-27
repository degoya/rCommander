# The Windows half of scripts/run-windows.sh for a WSL that mounts C: read-only: copies the
# rcommander.exe WSL built to %LOCALAPPDATA%\rcommander-dev\ and starts it. run-windows.sh prints
# this command with the paths filled in, every word single-quoted, when it cannot write there itself.
#
# Usage (PowerShell on Windows):
#   powershell -NoProfile -ExecutionPolicy Bypass -File <this script> -Binary <rcommander.exe> [<arguments>]
#
# Every word after the path of -Binary goes to rcommander.exe unchanged (RC-110-52), also one that
# looks like a PowerShell parameter (`--out`, `--screenshots`) and a value with spaces. Hence no
# param() block: a [Parameter()] attribute makes this an advanced script, whose common parameters
# (-OutVariable, -OutBuffer, ...) take `--out` for an abbreviation of theirs. The arguments are
# read from $args and quoted here for the Windows command line (the rules CommandLineToArgvW and
# Rust's std::env::args split it by), then handed to Start-Process as one string: an array there is
# joined with spaces, unquoted. A lone `--` is PowerShell's end of parameters and does not arrive.
#
# A running rcommander.exe locks the file: close it first, or the copy fails.
$ErrorActionPreference = 'Stop'
$words = @($args)

# One argument for a Windows command line, as Python's subprocess.list2cmdline writes it: in
# double quotes when it is empty or has a blank, a quote inside escaped with a backslash and the
# backslashes before a quote (or before the closing one) doubled.
function ConvertTo-CommandLineArgument([string]$Value) {
    $quote = $Value.Length -eq 0 -or $Value -match '[ \t]'
    $result = ''
    $backslashes = 0
    foreach ($char in $Value.ToCharArray()) {
        if ($char -eq '\') { $backslashes++; continue }
        if ($char -eq '"') {
            $result += ('\' * (2 * $backslashes + 1)) + '"'
        } else {
            $result += ('\' * $backslashes) + $char
        }
        $backslashes = 0
    }
    if ($quote) { '"' + $result + ('\' * (2 * $backslashes)) + '"' } else { $result + ('\' * $backslashes) }
}

if ($words.Count -lt 2 -or $words[0] -ne '-Binary') {
    throw 'usage: run-windows.ps1 -Binary <rcommander.exe> [<arguments>]'
}
$Binary = [string]$words[1]
$line = (@($words | Select-Object -Skip 2) | ForEach-Object { ConvertTo-CommandLineArgument ([string]$_) }) -join ' '

if (-not (Test-Path -LiteralPath $Binary -PathType Leaf)) { throw "no such file: $Binary" }
$destination = Join-Path $env:LOCALAPPDATA 'rcommander-dev'
New-Item -ItemType Directory -Force -Path $destination | Out-Null
$exe = Join-Path $destination 'rcommander.exe'
Copy-Item -LiteralPath $Binary -Destination $exe -Force

Write-Host "==> starting $exe $line"
if ($line) {
    Start-Process -FilePath $exe -ArgumentList $line
} else {
    Start-Process -FilePath $exe
}
Write-Host '==> started'
