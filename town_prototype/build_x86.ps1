$ErrorActionPreference = 'Stop'

$mingwRoot = if ($env:MSYS2_ROOT) { $env:MSYS2_ROOT } else { 'C:\msys64' }
$mingwBin = Join-Path $mingwRoot 'mingw32\bin'
$linker = Join-Path $mingwBin 'i686-w64-mingw32-gcc.exe'
if (-not (Test-Path -LiteralPath $linker)) {
    throw "32-bit MinGW linker not found at $linker. Install mingw-w64-i686-gcc or set MSYS2_ROOT."
}

$env:PATH = "$mingwBin;$mingwRoot\usr\bin;$env:PATH"
$env:CARGO_TARGET_DIR = Join-Path $env:TEMP 'ac_town_prototype_target'
$manifest = Join-Path $PSScriptRoot 'Cargo.toml'
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$outputDirectory = Join-Path $repoRoot 'outputs'

rustup target add i686-pc-windows-gnu
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

cargo build --release --manifest-path $manifest --target i686-pc-windows-gnu
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

New-Item -ItemType Directory -Force -Path $outputDirectory | Out-Null
$builtExe = Join-Path $env:CARGO_TARGET_DIR 'i686-pc-windows-gnu\release\ac_town_prototype.exe'
$outputExe = Join-Path $outputDirectory 'ac_town_prototype.exe'
Copy-Item -LiteralPath $builtExe -Destination $outputExe -Force
Write-Output "Built x86 prototype: $outputExe"
