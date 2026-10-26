# Installs the slab compiler (slab.exe), the language server (slab-lsp.exe) and, when VS Code is
# there, its extension, from the latest GitHub release, without a clone or a Rust toolchain. In PowerShell:
#
#   irm https://raw.githubusercontent.com/HeavyLvy/slab/main/install.ps1 | iex
#
# Windows (x64). Elsewhere see install.sh, or `cargo install slab-lang slab-lsp`.
#
# Environment:
#   SLAB_VERSION      install this version (0.1.0 or v0.1.0) instead of the latest
#   SLAB_INSTALL_DIR  where to put the programs (default: %LOCALAPPDATA%\slab\bin)
#   SLAB_NO_EXTENSION set to anything to skip installing the VS Code extension
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$repo = 'HeavyLvy/slab'
$dir = if ($env:SLAB_INSTALL_DIR) { $env:SLAB_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'slab\bin' }

if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64') {
    throw "no prebuilt binaries for $env:PROCESSOR_ARCHITECTURE; build from source: install Rust from https://rustup.rs, then run: cargo install slab-lang slab-lsp"
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

# the release's checksums, when it has them
$sums = $null
try { $sums = (Invoke-WebRequest -Uri "$base/SHA256SUMS" -UseBasicParsing).Content } catch { }

# downloads one file of the release into $tmp and checks it against the checksums
function Get-ReleaseFile($file) {
    $path = Join-Path $tmp $file
    Invoke-WebRequest -Uri "$base/$file" -OutFile $path -UseBasicParsing
    if ($sums) {
        $line = ($sums -split "`n") | Where-Object { $_ -match [regex]::Escape($file) } | Select-Object -First 1
        if ($line) {
            $expected = ($line -split '\s+')[0]
            $actual = (Get-FileHash -Algorithm SHA256 $path).Hash.ToLower()
            if ($expected -ne $actual) { throw "checksum mismatch for $file" }
        }
    }
    return $path
}

try {
    Write-Host "Downloading slab $version for Windows..."
    $zip = Get-ReleaseFile "$name.zip"
    Expand-Archive -Path $zip -DestinationPath $tmp -Force
    New-Item -ItemType Directory -Path $dir -Force | Out-Null
    foreach ($bin in 'slab.exe', 'slab-lsp.exe') {
        Copy-Item (Join-Path $tmp "$name\$bin") (Join-Path $dir $bin) -Force
    }
    Write-Host "Installed slab $version to $dir (slab.exe, slab-lsp.exe)"

    # the VS Code extension, from the same release, when VS Code (or VSCodium) is installed
    if (-not $env:SLAB_NO_EXTENSION) {
        $code = Get-Command code, codium -ErrorAction SilentlyContinue | Select-Object -First 1
        if (-not $code) {
            Write-Host 'VS Code was not found, so the editor extension was not installed (see the release page for the .vsix).'
        } else {
            # the package for Windows has the language server inside; the universal one finds it on PATH
            $installed = $false
            foreach ($vsix in "slab-vscode-win32-x64-$version.vsix", "slab-vscode-$version.vsix") {
                try { $path = Get-ReleaseFile $vsix } catch { continue }
                # (Windows PowerShell 5.1 turns a native program's stderr into an error under 'Stop')
                $ErrorActionPreference = 'Continue'
                & $code.Source --install-extension $path --force *> $null
                $ErrorActionPreference = 'Stop'
                if ($LASTEXITCODE -eq 0) {
                    Write-Host 'Installed the VS Code extension (reload open VS Code windows).'
                } else {
                    Write-Host "Could not install the extension with '$($code.Name)'; install $vsix from the release page by hand."
                }
                $installed = $true
                break
            }
            if (-not $installed) { Write-Host 'This release has no VS Code extension package.' }
        }
    }
} finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}

$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if (($userPath -split ';') -notcontains $dir) {
    [Environment]::SetEnvironmentVariable('Path', "$userPath;$dir", 'User')
    Write-Host "Added $dir to your PATH. Open a new terminal to use it."
}
Write-Host 'Try: slab --version'
