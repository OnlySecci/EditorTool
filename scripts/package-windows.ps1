param([switch]$PortableOnly)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $projectRoot
try {
    $version = (Select-String -LiteralPath Cargo.toml -Pattern '^version = "([^"]+)"').Matches[0].Groups[1].Value
    New-Item -ItemType Directory -Force -Path 'dist/packages' | Out-Null
    $arch=if($env:PACKAGE_ARCH){$env:PACKAGE_ARCH}else{'x64'}
    if(-not $PortableOnly){
    $nsis = Get-Command makensis.exe -ErrorAction SilentlyContinue
    $compiler = if($env:NSIS_COMPILER) {$env:NSIS_COMPILER} elseif($nsis){$nsis.Source}else{Join-Path ${env:ProgramFiles(x86)} 'NSIS/makensis.exe'}
    if (-not (Test-Path -LiteralPath $compiler)) { throw 'Set NSIS_COMPILER to an installed NSIS compiler: https://nsis.sourceforge.io/Download' }
    & $compiler "/DVERSION=$version" "/DARCH=$arch" 'packaging/windows.nsi'
    if ($LASTEXITCODE -ne 0) { throw 'Installer-Build fehlgeschlagen.' }
    }
    $portable = "dist/packages/EditorTool-$version-windows-$arch-portable.zip"
    $files=@('dist/windows/EditorTool.exe','dist/windows/README.md','dist/windows/LICENSE','third-party-licenses','packaging/portable.txt')
    $files += @(Get-ChildItem -LiteralPath dist/windows -Filter '*.dll' | Select-Object -ExpandProperty FullName)
    Compress-Archive -Path $files -DestinationPath $portable -Force
    Write-Host "Created: $projectRoot\dist\packages"
} finally { Pop-Location }
