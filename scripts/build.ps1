#!/usr/bin/env pwsh
# Нативная сборка КуМир на Windows.
#
#   pwsh -File scripts/build.ps1 windows   # .exe + zip в dist\windows\
#   pwsh -File scripts/build.ps1 test
#   pwsh -File scripts/build.ps1 help
#
# Кросс-сборка APK выполняется через bash-скрипт (Linux, WSL или CI).

[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [string]$Command = "help",

    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$Rest
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

$VersionMatch = Select-String -Path (Join-Path $Root "Cargo.toml") `
    -Pattern '^version = "([^"]+)"' | Select-Object -First 1
if (-not $VersionMatch) { throw "Не удалось прочитать версию из Cargo.toml" }
$Version = $VersionMatch.Matches[0].Groups[1].Value

$HostTarget = ((rustc -vV | Select-String '^host:').Line -split '\s+')[1]
$Arch = switch -Regex ($HostTarget) {
    '^x86_64-'  { 'x86_64' }
    '^aarch64-' { 'aarch64' }
    '^i686-'    { 'x86' }
    default     { $HostTarget.Split('-')[0] }
}

function Fail([string]$Message) {
    Write-Host "error: $Message" -ForegroundColor Red
    exit 1
}

function Invoke-Cargo([string[]]$CargoArgs) {
    & cargo @CargoArgs
    if ($LASTEXITCODE -ne 0) { throw "cargo завершился с кодом $LASTEXITCODE" }
}

function Build-Windows {
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        Fail "cargo не найден. Установите Rust: https://rustup.rs"
    }

    $stage = Join-Path $Root "dist\windows\kumir-ide-$Version-windows-$Arch"
    $archive = "$stage.zip"

    Write-Host "==> сборка Windows ($HostTarget)"
    Invoke-Cargo @("build", "--release", "--bin", "kumir-ide", "--bin", "kumir-run")

    if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
    New-Item -ItemType Directory -Force -Path (Join-Path $stage "bin") | Out-Null
    Copy-Item "target\release\kumir-ide.exe", "target\release\kumir-run.exe" `
        -Destination (Join-Path $stage "bin")
    Copy-Item README.md, LICENSE.md -Destination $stage
    Copy-Item examples -Destination $stage -Recurse
    Copy-Item docs -Destination $stage -Recurse

    if (Test-Path $archive) { Remove-Item -Force $archive }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [System.IO.Compression.ZipFile]::CreateFromDirectory(
        $stage, $archive,
        [System.IO.Compression.CompressionLevel]::Fastest,
        $true
    )

    Remove-Item -Recurse -Force $stage
    Write-Host "==> готово: $archive"
}

function Show-Help {
    @"
КуМир на Rust — сборка $Version (Windows)

Использование:
  pwsh -File scripts/build.ps1 <команда>

Команды:
  windows     собрать .exe и zip в dist\windows\
  test        cargo test --workspace
  check       cargo check --workspace --all-targets
  fmt         отформатировать код
  clippy      запустить clippy
  run         запустить IDE
  cli         запустить kumir-run (аргументы передаются программе)
  clean       удалить target\ и android\build\
  distclean   clean + удалить dist\
  doctor      показать версии Rust и инструментов
  help        эта справка

APK собирается bash-скриптом: ./scripts/build.sh apk (Linux, WSL или CI).
"@
}

function Show-Doctor {
    Write-Host "Версия проекта: $Version"
    rustc -V
    cargo -V
    Write-Host "Host target:    $HostTarget"
    Write-Host "Установлены:    $((rustup target list --installed) -join ' ')"
}

switch ($Command.ToLowerInvariant()) {
    "help"      { Show-Help }
    "windows"   { Build-Windows }
    "test"      { Invoke-Cargo (@("test", "--workspace") + $Rest) }
    "check"     { Invoke-Cargo @("check", "--workspace", "--all-targets") }
    "fmt"       { Invoke-Cargo @("fmt", "--all") }
    "clippy"    { Invoke-Cargo @("clippy", "--workspace", "--all-targets") }
    "run"       { Invoke-Cargo (@("run", "-p", "kumir-gui", "--") + $Rest) }
    "cli"       { Invoke-Cargo (@("run", "-p", "kumir-cli", "--") + $Rest) }
    "clean" {
        Remove-Item -Recurse -Force "target", "android\build" -ErrorAction SilentlyContinue
        Write-Host "==> target\ и android\build\ удалены"
    }
    "distclean" {
        Remove-Item -Recurse -Force "target", "android\build", "dist" -ErrorAction SilentlyContinue
        Write-Host "==> target\, android\build\ и dist\ удалены"
    }
    "doctor"    { Show-Doctor }
    default     { Fail "неизвестная команда «$Command». Запустите help." }
}
