# Installs the slab compiler (slab.exe) and language server (slab-lsp.exe) from the latest
# GitHub release, without a clone or a Rust toolchain. In PowerShell:
#
#   irm https://raw.githubusercontent.com/HeavyLvy/slab/main/install.ps1 | iex
#
# Windows (x64). Elsewhere see install.sh, or `cargo install slab-lang slab-lsp`.
#
# Environment:
#   SLAB_VERSION      install this version (0.1.0 or v0.1.0) instead of the latest
#   SLAB_INSTALL_DIR  where to put the programs (default: %LOCALAPPDATA%\slab\bin)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$repo = 'HeavyLvy/slab'
$dir = if ($env:SLAB_INSTALL_DIR) { $env:SLAB_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'slab\bin' }

if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64') {
    throw "no prebuilt binaries for $env:PROCESSOR_ARCHITECTURE; build from source with: cargo install slab-lang slab-lsp"
}

if ($env:SLAB_VERSION) {
    $version = $env:SLAB_VERSION.TrimStart('v')
} else {
    # the latest release is a redirect to .../releases/tag/vX.Y.Z
    # (the final address of the request is where the response object keeps it: Windows
    # PowerShell and PowerShell 7 name it differently)
    $response = Invoke-WebRequest -Uri "https://github.com/$repo/releases/latest" -Method Head -UseBasicParsing
    $final = if ($response.BaseResponse.ResponseUri) { $response.BaseResponse.ResponseUri.AbsoluteUri } else { $response.BaseResponse.RequestMessage.RequestUri.AbsoluteUri }
    if ($final -notmatch '/tag/v(.+)$') { throw 'could not look up the latest release' }
    $version = $Matches[1]
}

$name = "slab-$version-windows-x86_64"
$base = "https://github.com/$repo/releases/download/v$version"
$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ([System.IO.Path]::GetRandomFileName())
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    Write-Host "Downloading slab $version for Windows..."
    $zip = Join-Path $tmp "$name.zip"
    Invoke-WebRequest -Uri "$base/$name.zip" -OutFile $zip -UseBasicParsing

    # check the archive against the release's checksums when it has them
    $sums = $null
    try { $sums = (Invoke-WebRequest -Uri "$base/SHA256SUMS" -UseBasicParsing).Content } catch { }
    if ($sums) {
        $line = ($sums -split "`n") | Where-Object { $_ -match [regex]::Escape("$name.zip") } | Select-Object -First 1
        if ($line) {
            $expected = ($line -split '\s+')[0]
            $actual = (Get-FileHash -Algorithm SHA256 $zip).Hash.ToLower()
            if ($expected -ne $actual) { throw "checksum mismatch for $name.zip" }
        }
    }

    Expand-Archive -Path $zip -DestinationPath $tmp -Force
    New-Item -ItemType Directory -Path $dir -Force | Out-Null
    foreach ($bin in 'slab.exe', 'slab-lsp.exe') {
        Copy-Item (Join-Path $tmp "$name\$bin") (Join-Path $dir $bin) -Force
    }
} finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
Write-Host "Installed slab $version to $dir (slab.exe, slab-lsp.exe)"

$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if (($userPath -split ';') -notcontains $dir) {
    [Environment]::SetEnvironmentVariable('Path', "$userPath;$dir", 'User')
    Write-Host "Added $dir to your PATH. Open a new terminal to use it."
}
Write-Host 'Try: slab --version'
Write-Host 'Editor support: install the "slab" extension in VS Code (it finds slab-lsp there).'
