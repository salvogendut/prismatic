$ErrorActionPreference = 'Stop'
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vs) { throw 'Visual Studio x64 build tools were not found' }
$setup = Join-Path $vs 'Common7\Tools\VsDevCmd.bat'
$lines = & cmd.exe /s /c "`"$setup`" -arch=x64 -host_arch=x64 >nul && set"
if ($LASTEXITCODE -ne 0) { throw 'Cannot initialize the MSVC environment' }
foreach ($line in $lines) {
    if ($line -match '^(PATH|LIB|INCLUDE|LIBPATH|WindowsSdkDir|WindowsSDKVersion|VCToolsInstallDir|VCToolsRedistDir)=(.*)$') {
        Add-Content -Path $env:GITHUB_ENV -Value "$($Matches[1])=$($Matches[2])"
    }
}
