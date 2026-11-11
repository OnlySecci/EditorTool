$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $projectRoot
try {
    $buildTarget=$env:CARGO_BUILD_TARGET
    $buildArgs=@('build','--release','--locked')
    # Build MSVC libraries with the same CRT mode as Rust instead of mixing a
    # dynamic-CRT prebuilt FLTK archive with the static-CRT release executable.
    if($buildTarget -like '*msvc') {
        $buildArgs += @('--no-default-features','--features','fltk/static-msvcrt')
        $env:CFLTK_TOOLCHAIN=Join-Path $projectRoot 'packaging/msvc-static.cmake'
    } elseif($buildTarget -like 'aarch64*') { $buildArgs += '--no-default-features' }
    cargo @buildArgs
    if ($LASTEXITCODE -ne 0) { throw 'Rust-Build fehlgeschlagen.' }
    New-Item -ItemType Directory -Force -Path 'dist/windows' | Out-Null
    $binary=if($buildTarget){"target/$buildTarget/release/editortool.exe"}else{'target/release/editortool.exe'}
    try {
        Copy-Item -LiteralPath $binary -Destination 'dist/windows/EditorTool.exe'
    } catch {
        # Windows permits renaming a running executable while keeping its process alive.
        # Keep the running version intact and put the new build at the normal launch path.
        $previousName = 'EditorTool.previous.' + [DateTime]::Now.ToString('yyyyMMdd-HHmmss-fff') + '.exe'
        Rename-Item -LiteralPath 'dist/windows/EditorTool.exe' -NewName $previousName
        Copy-Item -LiteralPath $binary -Destination 'dist/windows/EditorTool.exe'
        Write-Host 'Die laufende Version bleibt geöffnet; beim nächsten Start wird die neue Fassung verwendet.'
    }
    Copy-Item -LiteralPath 'README.md','LICENSE' -Destination 'dist/windows'
    # GNU builds can depend on MinGW runtimes. Copy the ones present in PATH.
    $isGnu=($buildTarget -like '*gnu*') -or (-not $buildTarget -and ((rustc -vV | Select-String '^host:') -match 'gnu'))
    if($isGnu){
    foreach ($dll in @('libstdc++-6.dll', 'libgcc_s_seh-1.dll', 'libwinpthread-1.dll')) {
        $candidate = $env:PATH.Split(';') | ForEach-Object { Join-Path $_ $dll } | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
        if ($candidate) { Copy-Item -LiteralPath $candidate -Destination "dist/windows/$dll" }
    }
    }
    Write-Host "Erstellt: $projectRoot\dist\windows\EditorTool.exe"
} finally { Pop-Location }
