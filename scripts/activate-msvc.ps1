# Process-local build environment. Does not alter global PATH or toolchain configuration.
$projectRoot = Split-Path $PSScriptRoot -Parent
$env:TEMP = Join-Path $projectRoot '.tools/test-tmp'
$env:TMP = $env:TEMP
New-Item -ItemType Directory -Force $env:TEMP | Out-Null
$vsInstall = & 'C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe' -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (!$vsInstall) { throw 'Install Visual Studio x64 C++ Build Tools first.' }
$vsDevCmd = Join-Path $vsInstall 'Common7\Tools\VsDevCmd.bat'
$seenBuildVariables = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
cmd /d /c "`"$vsDevCmd`" -no_logo -arch=x64 -host_arch=x64 && set" | ForEach-Object {
    # Some sandbox launchers supply both PATH and Path. Preserve VsDevCmd's first value.
    if ($_ -match '^([^=]+)=(.*)$' -and $seenBuildVariables.Add($matches[1])) { [Environment]::SetEnvironmentVariable($matches[1], $matches[2], 'Process') }
}
$projectRoot = Split-Path $PSScriptRoot -Parent
$env:CARGO_HOME = Join-Path $projectRoot '.tools/cargo-home'
$env:CARGO_BUILD_JOBS = '4'
$env:RUSTUP_TOOLCHAIN = 'stable-x86_64-pc-windows-msvc'
$env:RUSTFLAGS = ''
$env:PATH = (Join-Path $env:USERPROFILE '.cargo/bin') + ';' + $env:PATH
$env:TEMP = Join-Path $projectRoot '.tools/test-tmp'
$env:TMP = $env:TEMP
New-Item -ItemType Directory -Force $env:TEMP | Out-Null
Get-Command link.exe | Select-Object -ExpandProperty Source
