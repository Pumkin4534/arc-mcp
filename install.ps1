# Arc MCP prebuilt installation for Windows. Run in a normal PowerShell session.
& {
$ErrorActionPreference = 'Stop'
$version = '0.2.0'
$originalPath = $env:PATH
$originalTls = [Net.ServicePointManager]::SecurityProtocol
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$work = Join-Path ([IO.Path]::GetTempPath()) ('arc-install-' + [Guid]::NewGuid().ToString('N'))

function Get-Download([string]$Url, [string]$Destination) {
    Invoke-WebRequest -UseBasicParsing -Uri $Url -OutFile $Destination
}
function Test-Checksum([string]$Path, [string]$Expected) {
    if ($Expected -notmatch '^[0-9a-fA-F]{64}$') { throw 'Invalid checksum record.' }
    if ((Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $Expected.ToLowerInvariant()) { throw "Checksum mismatch: $Path" }
}
function Test-Node {
    if (-not (Get-Command node.exe -ErrorAction SilentlyContinue) -or -not (Get-Command npm.cmd -ErrorAction SilentlyContinue)) { return $false }
    try {
        & node.exe -e 'if (Number(process.versions.node.split(".")[0]) < 20) process.exit(1)' 2>$null
        if ($LASTEXITCODE -ne 0) { return $false }
        & npm.cmd --version 1>$null 2>$null
        return $LASTEXITCODE -eq 0
    } catch { return $false }
}
try {
    if ($env:OS -ne 'Windows_NT') { throw 'Use install.sh on macOS or Ubuntu.' }
    $architecture = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
    switch ($architecture.ToUpperInvariant()) {
        'AMD64' { $arch = 'x64'; $nodeHash = 'cbc74b5e0982c9dbb41525fd8fb5dcd32fbee237863f7829072f4639a1ce5f20' }
        'ARM64' { $arch = 'arm64'; $nodeHash = 'a8b8cd79cbbb3062435f449591d98dd63730bbbf30ce2d7cc3a6376c236cb111' }
        default { throw 'Arc requires 64-bit Windows (x64 or ARM64).' }
    }
    New-Item -ItemType Directory -Path $work | Out-Null
    if (-not (Test-Node)) {
        $tools = if ($env:ARC_TOOLS_HOME) { $env:ARC_TOOLS_HOME } else { Join-Path $env:LOCALAPPDATA 'arc-mcp\tools' }
        if (-not [IO.Path]::IsPathRooted($tools)) { throw 'ARC_TOOLS_HOME must be absolute.' }
        $nodeVersion = '22.23.4'
        $nodeName = "node-v$nodeVersion-win-$arch"
        $nodeDirectory = Join-Path $tools $nodeName
        $env:PATH = "$nodeDirectory;$env:PATH"
        if (-not (Test-Node)) {
            if (Test-Path -LiteralPath $nodeDirectory) { throw "Inspect the incomplete managed Node directory: $nodeDirectory" }
            Write-Host 'Preparing Node.js for installation...'
            $nodeArchive = Join-Path $work 'node.zip'
            Get-Download "https://nodejs.org/dist/v$nodeVersion/$nodeName.zip" $nodeArchive
            Test-Checksum $nodeArchive $nodeHash
            Expand-Archive -LiteralPath $nodeArchive -DestinationPath (Join-Path $work 'node')
            New-Item -ItemType Directory -Path $tools -Force | Out-Null
            Move-Item -LiteralPath (Join-Path (Join-Path $work 'node') $nodeName) -Destination $nodeDirectory
        }
    }
    if (-not (Test-Node)) { throw 'Node.js/npm setup failed.' }
    $asset = "arc-mcp-$version-windows-$arch.zip"
    $release = "https://github.com/Pumkin4534/arc-mcp/releases/download/v$version"
    $checksumFile = Join-Path $work 'checksum'
    Write-Host "Downloading Arc MCP $version for Windows/$arch..."
    Get-Download "$release/$asset.sha256" $checksumFile
    $record = (Get-Content -LiteralPath $checksumFile -Raw).Trim()
    $pattern = '^([0-9a-fA-F]{64})\s+' + [regex]::Escape($asset) + '$'
    if ($record -notmatch $pattern) { throw 'Checksum filename/format mismatch.' }
    $expected = $Matches[1]
    $archive = Join-Path $work 'package.zip'
    Get-Download "$release/$asset" $archive
    Test-Checksum $archive $expected
    $unpacked = Join-Path $work 'package'
    Expand-Archive -LiteralPath $archive -DestinationPath $unpacked
    $package = Join-Path $unpacked "arc-mcp-$version-windows-$arch"
    Push-Location -LiteralPath $package
    try {
        & npm.cmd ci --ignore-scripts --no-audit --no-fund
        if ($LASTEXITCODE -ne 0) { throw 'Installer dependencies failed.' }
        & node.exe scripts/install.mjs --install
        if ($LASTEXITCODE -ne 0) { throw 'Arc installation failed; existing files were preserved.' }
    } finally { Pop-Location }
} finally {
    $env:PATH = $originalPath
    [Net.ServicePointManager]::SecurityProtocol = $originalTls
    if (Test-Path -LiteralPath $work) { Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue }
}
}
