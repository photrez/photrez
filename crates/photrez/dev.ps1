<#
.SYNOPSIS
  Build and run the native Photrez app in debug -- the dev loop.

.DESCRIPTION
  "cargo run" plus the two environment facts this box needs:

    1. msys64\mingw64\bin on PATH. The gnu target links through ld.exe and the
       windows-sys build script shells out to dlltool.exe by bare name; cargo
       can name a linker in .cargo/config.toml but cannot put a directory on
       PATH for a *build script* -- hence this script.
    2. The gnu toolchain itself: the crate-local rust-toolchain.toml pins it,
       so a plain `cargo` call inside the crate resolves gnu even without the
       machine's directory override.

  The window is left open. -NoBuild relaunches the last artefact.

.EXAMPLE
  .\dev.ps1
.EXAMPLE
  .\dev.ps1 -NoBuild
#>
param(
  [switch]$NoBuild
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version 2.0

$Root = $PSScriptRoot
$Exe = Join-Path $Root "target\debug\photrez.exe"

function Say([string]$msg) { Write-Host $msg -ForegroundColor Cyan }

function Find-MingwBin {
  # msys64 first, then Git for Windows' own copy -- both ship the same gnu
  # binutils. A missing linker is the most common way this port refuses to
  # build on a fresh machine, so the search is what makes the script portable.
  $candidates = @(
    "C:\msys64\mingw64\bin",
    "C:\Program Files\Git\mingw64\bin",
    (Join-Path $env:USERPROFILE "scoop\apps\msys2\current\mingw64\bin")
  )
  foreach ($c in $candidates) {
    if (Test-Path (Join-Path $c "gcc.exe")) { return $c }
  }
  return $null
}

$mingw = Find-MingwBin
if (-not $mingw) {
  throw "No gnu toolchain found. Tried C:\msys64\mingw64\bin, Git's mingw64\bin, and scoop's msys2. Install MSYS2, then: pacman -S mingw-w64-x86_64-gcc"
}
$env:Path = "$mingw;$env:USERPROFILE\.cargo\bin;$env:Path"
Say "toolchain : $mingw"

if ($NoBuild) {
  if (-not (Test-Path $Exe)) {
    throw "Nothing built at $Exe -- run without -NoBuild first."
  }
} else {
  Say "building (the first gpui link takes a while)..."
  # Cargo writes warnings to stderr; with EAP=Stop the `2>&1` merge turns them
  # into throwing NativeCommandError records. Temporarily Continue so the
  # build finishes, then restore Stop for the rest of the script.
  $prevEAP = $ErrorActionPreference
  $ErrorActionPreference = "Continue"
  cargo build --manifest-path "$Root\Cargo.toml" 2>&1 |
    Select-String -Pattern "^error|^error\[|^warning:|Finished|Compiling photrez " |
    ForEach-Object { $_.Line }
  $ErrorActionPreference = $prevEAP
  if (-not (Test-Path $Exe)) { throw "Build did not produce $Exe" }
}

Stop-Process -Name "photrez" -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 400

$proc = Start-Process -FilePath $Exe -PassThru
Say "running pid $($proc.Id)"

# The window handle arrives a few hundred ms after the process on a debug
# build, and the first frame can be a blank clear rather than the element tree.
# Polling the client rect is the honest wait.
$deadline = (Get-Date).AddSeconds(30)
$handle = 0
do {
  Start-Sleep -Milliseconds 300
  $proc.Refresh()
  $handle = $proc.MainWindowHandle
} until ($handle -ne 0 -or $proc.HasExited -or (Get-Date) -gt $deadline)

if ($proc.HasExited) {
  throw "photrez exited immediately (code $($proc.ExitCode))."
}
if ($handle -eq 0) {
  Say "no window handle after 30s; the process is up, so look for it on the taskbar."
} else {
  Say "ready -- window '$((Get-Process -Id $proc.Id).MainWindowTitle)'."
}
