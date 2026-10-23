param(
    [Parameter(Mandatory = $true)]
    [string]$Path
)

$resolvedPath = (Resolve-Path -LiteralPath $Path -ErrorAction Stop).Path
$bytes = [System.IO.File]::ReadAllBytes($resolvedPath)

if ($bytes.Length -lt 64) {
    throw "PE file is too short: $resolvedPath"
}

$peOffset = [BitConverter]::ToInt32($bytes, 0x3c)
$subsystemOffset = $peOffset + 24 + 68
if ($peOffset -lt 0 -or $subsystemOffset + 2 -gt $bytes.Length) {
    throw "PE header points outside the file: $resolvedPath"
}

$signature = [System.Text.Encoding]::ASCII.GetString($bytes, $peOffset, 4)
if ($signature -ne "PE`0`0") {
    throw "Invalid PE signature in $resolvedPath"
}

$optionalHeaderMagic = [BitConverter]::ToUInt16($bytes, $peOffset + 24)
if ($optionalHeaderMagic -notin @(0x10b, 0x20b)) {
    throw "Unsupported PE optional header magic 0x$($optionalHeaderMagic.ToString('x')) in $resolvedPath"
}

$subsystem = [BitConverter]::ToUInt16($bytes, $subsystemOffset)
if ($subsystem -ne 2) {
    throw "Expected IMAGE_SUBSYSTEM_WINDOWS_GUI (2), found $subsystem in $resolvedPath"
}

Write-Host "Verified IMAGE_SUBSYSTEM_WINDOWS_GUI (2): $resolvedPath"
