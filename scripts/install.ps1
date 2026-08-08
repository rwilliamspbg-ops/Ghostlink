<#
.SYNOPSIS
    Ghostlink one-line installer for Windows.

.DESCRIPTION
    Downloads the prebuilt ghost-link.exe binary published on this repo's
    GitHub Releases (built by .github/workflows/release-artifacts.yml),
    verifies it against the release's published SHA256SUMS-windows-latest
    file, and installs it to a user-writable directory. No admin rights
    required, nothing is added to PATH automatically.

    This installs the ghost-link.exe binary only (CLI + OpenAI-compatible
    API server) -- it does NOT install the Go control-plane gateway or the
    built React GUI, which are not published as standalone release assets
    today (only ghost-link-<os>*, SHA256SUMS-<os>, and an SBOM are -- see
    scripts/release_bundle.sh). For the full browser GUI on Windows, clone
    the repo and use launch.bat or launch-native.ps1 -- see the README's
    Quick Start section.

    Honesty notes, read before filing a bug:
      - Only x86_64/amd64 Windows binaries are published as of this writing
        -- there is no separate ARM64 build (check the `os:` matrix in
        .github/workflows/release-artifacts.yml in case that has since
        changed). This script refuses to install on non-AMD64 hosts rather
        than handing you a binary that won't run.
      - There is currently no code-signing certificate for these binaries,
        so Windows SmartScreen may warn on first run of the downloaded
        ghost-link.exe -- that is expected, not a sign this script did
        anything wrong.

.PARAMETER Version
    Release tag to install, e.g. "v1.16.1". Defaults to $env:VERSION, or
    the latest release if neither is set.

.PARAMETER InstallDir
    Directory to install ghost-link.exe into. Defaults to
    $env:GHOSTLINK_INSTALL_DIR, or "$env:LOCALAPPDATA\Ghostlink\bin".

.EXAMPLE
    irm https://raw.githubusercontent.com/rwilliamspbg-ops/Ghostlink/main/scripts/install.ps1 | iex

.EXAMPLE
    $env:VERSION = "v1.16.1"
    irm https://raw.githubusercontent.com/rwilliamspbg-ops/Ghostlink/main/scripts/install.ps1 | iex
#>
[CmdletBinding()]
param(
    [string]$Version = $(if ($env:VERSION) { $env:VERSION } else { "latest" }),
    [string]$InstallDir = $(if ($env:GHOSTLINK_INSTALL_DIR) { $env:GHOSTLINK_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "Ghostlink\bin" })
)

$ErrorActionPreference = "Stop"

$Repo   = "rwilliamspbg-ops/Ghostlink"
$GitHub = "https://github.com/$Repo"
$Api    = "https://api.github.com/repos/$Repo"

function Write-Info([string]$Message) { Write-Host $Message }
function Write-InstallWarning([string]$Message) { Write-Host "warning: $Message" -ForegroundColor Yellow }
function Fail([string]$Message) {
    Write-Host "error: $Message" -ForegroundColor Red
    exit 1
}

# --- Arch check --------------------------------------------------------
# No ARM64 Windows build exists today (single-arch matrix, no
# cross-compilation step) -- refuse rather than install a binary that will
# not execute. PROCESSOR_ARCHITECTURE reflects the current process's
# architecture, so this is best-effort under x64 emulation on ARM64 Windows.
$Arch = $env:PROCESSOR_ARCHITECTURE
if ($Arch -ne "AMD64") {
    Fail "unsupported architecture '$Arch'. Ghostlink release binaries are x86_64/amd64-only as of this writing -- there is no ARM64 build for Windows (check .github/workflows/release-artifacts.yml's build matrix in case this has changed since). Refusing to install a binary that won't run on this machine. Build from source instead: $GitHub#quick-start"
}

$BinAsset  = "ghost-link-windows-latest.exe"
$SumsAsset = "SHA256SUMS-windows-latest"

# --- Resolve version -----------------------------------------------------
if ($Version -eq "latest") {
    Write-Info "Looking up the latest Ghostlink release..."
    try {
        $Release = Invoke-RestMethod -UseBasicParsing -Uri "$Api/releases/latest" -Headers @{ "User-Agent" = "ghostlink-install.ps1" }
    } catch {
        Fail "failed to query $Api/releases/latest -- check your network connection ($($_.Exception.Message))"
    }
    $Tag = $Release.tag_name
    if (-not $Tag) { Fail "could not determine the latest release tag from the GitHub API response" }
} else {
    $Tag = $Version
}
Write-Info "Installing Ghostlink $Tag (windows-latest, $Arch)"

$DownloadBase = "$GitHub/releases/download/$Tag"
$BinUrl  = "$DownloadBase/$BinAsset"
$SumsUrl = "$DownloadBase/$SumsAsset"

# --- Download --------------------------------------------------------------
$TmpDir = Join-Path ([System.IO.Path]::GetTempPath()) ("ghostlink-install-" + [System.Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $TmpDir -Force | Out-Null

try {
    $TmpBin  = Join-Path $TmpDir $BinAsset
    $TmpSums = Join-Path $TmpDir $SumsAsset

    Write-Info "Downloading $BinAsset..."
    try {
        Invoke-WebRequest -UseBasicParsing -Uri $BinUrl -OutFile $TmpBin
    } catch {
        Fail "failed to download $BinUrl -- does release $Tag exist and include a windows-latest build? ($($_.Exception.Message))"
    }

    Write-Info "Downloading $SumsAsset for verification..."
    try {
        Invoke-WebRequest -UseBasicParsing -Uri $SumsUrl -OutFile $TmpSums
    } catch {
        Fail "failed to download $SumsUrl ($($_.Exception.Message))"
    }

    # --- Verify checksum -----------------------------------------------
    # SHA256SUMS-windows-latest lines look like
    # "<hash> *./ghost-link-windows-latest.exe" (binary mode -- what the
    # windows-latest runner's sha256sum actually emits) but handle the
    # "<hash>  ./name" text-mode format too (seen on ubuntu-latest /
    # macos-latest) in case a future runner image changes this.
    $ExpectedHash = $null
    foreach ($line in Get-Content -Path $TmpSums) {
        if ($line -match '^([0-9a-fA-F]{64})\s+\*?\.\/?(.+)$') {
            if ($Matches[2] -eq $BinAsset) {
                $ExpectedHash = $Matches[1].ToLowerInvariant()
                break
            }
        }
    }
    if (-not $ExpectedHash) {
        Fail "could not find a checksum entry for $BinAsset in $SumsAsset -- refusing to install an unverified binary"
    }

    $ActualHash = (Get-FileHash -Path $TmpBin -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($ActualHash -ne $ExpectedHash) {
        Fail "checksum mismatch for ${BinAsset}: expected $ExpectedHash, got $ActualHash. The download may be corrupted or tampered with -- not installing."
    }
    Write-Info "Checksum verified (sha256:$ActualHash)"

    # --- Install ---------------------------------------------------------
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    $Dest = Join-Path $InstallDir "ghost-link.exe"
    Move-Item -Path $TmpBin -Destination $Dest -Force

    Write-Host ""
    Write-Host "Ghostlink $Tag installed to $Dest"
    Write-Host ""

    $PathDirs = $env:PATH -split ";"
    $OnPath = $PathDirs | Where-Object { $_.TrimEnd("\") -eq $InstallDir.TrimEnd("\") }
    if (-not $OnPath) {
        Write-InstallWarning "$InstallDir is not on your PATH."
        Write-Host "Add it permanently (current-user scope, no admin required), then open a new terminal:"
        Write-Host "  [Environment]::SetEnvironmentVariable('Path', `$env:Path + ';$InstallDir', 'User')"
        Write-Host ""
    }

    Write-Host "Next steps:"
    Write-Host "  & `"$Dest`" --help"
    Write-Host "  & `"$Dest`" doctor --strict         # sanity-check your setup"
    Write-Host "  & `"$Dest`" serve 127.0.0.1 8003    # start the OpenAI-compatible API server"
    Write-Host ""
    Write-Host "This installs the ghost-link.exe binary only (CLI + OpenAI-compatible"
    Write-Host "API server) -- it does not include the Go control-plane gateway or the"
    Write-Host "React GUI, which aren't published as standalone release assets. For the"
    Write-Host "full browser GUI, clone the repo and run 'launch.bat' or 'docker compose"
    Write-Host "up': $GitHub#quick-start"
    Write-Host ""
    Write-Host "Models load from a 'models\' directory relative to wherever you run"
    Write-Host "ghost-link.exe from. More: $GitHub/blob/main/docs/QUICKSTART.md"
}
finally {
    Remove-Item -Path $TmpDir -Recurse -Force -ErrorAction SilentlyContinue
}
