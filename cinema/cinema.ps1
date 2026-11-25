# The cinema kit's dev pak: the PowerShell spelling of cinema.py, which does the work (and has --help). Kept for
# callers that already use it.
#
#   powershell -ExecutionPolicy Bypass -File cinema\cinema.ps1 build|install|uninstall|status [-Out <pak90_dir.vpk>]
param([Parameter(Mandatory = $true)][ValidateSet("build", "install", "uninstall", "status")][string]$Action,
      [string]$Out = "")
$py = @("-I", (Join-Path $PSScriptRoot "cinema.py"), $Action)
if ($Out) { $py += @("--out", $Out) }
& python @py
exit $LASTEXITCODE
