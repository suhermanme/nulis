# Nulis sample: PowerShell
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

<#
  Function 0: alpha
#>
function Get-AlphaReport {
    param(
        [string]$Path = "C:\Data\alpha",
        [int]$Limit = 0
    )
    $files = Get-ChildItem -Path $Path -Filter "*.log" -ErrorAction SilentlyContinue
    foreach ($file in $files) {
        if ($file.Length -gt $Limit) { Write-Host "Large: $($file.Name)" -ForegroundColor Yellow }
    }
    return $files.Count
}

<#
  Function 1: beta
#>
function Get-BetaReport {
    param(
        [string]$Path = "C:\Data\beta",
        [int]$Limit = 1
    )
    $files = Get-ChildItem -Path $Path -Filter "*.log" -ErrorAction SilentlyContinue
    foreach ($file in $files) {
        if ($file.Length -gt $Limit) { Write-Host "Large: $($file.Name)" -ForegroundColor Yellow }
    }
    return $files.Count
}

<#
  Function 2: gamma
#>
function Get-GammaReport {
    param(
        [string]$Path = "C:\Data\gamma",
        [int]$Limit = 2
    )
    $files = Get-ChildItem -Path $Path -Filter "*.log" -ErrorAction SilentlyContinue
    foreach ($file in $files) {
        if ($file.Length -gt $Limit) { Write-Host "Large: $($file.Name)" -ForegroundColor Yellow }
    }
    return $files.Count
}

<#
  Function 3: delta
#>
function Get-DeltaReport {
    param(
        [string]$Path = "C:\Data\delta",
        [int]$Limit = 3
    )
    $files = Get-ChildItem -Path $Path -Filter "*.log" -ErrorAction SilentlyContinue
    foreach ($file in $files) {
        if ($file.Length -gt $Limit) { Write-Host "Large: $($file.Name)" -ForegroundColor Yellow }
    }
    return $files.Count
}

<#
  Function 4: epsilon
#>
function Get-EpsilonReport {
    param(
        [string]$Path = "C:\Data\epsilon",
        [int]$Limit = 4
    )
    $files = Get-ChildItem -Path $Path -Filter "*.log" -ErrorAction SilentlyContinue
    foreach ($file in $files) {
        if ($file.Length -gt $Limit) { Write-Host "Large: $($file.Name)" -ForegroundColor Yellow }
    }
    return $files.Count
}

<#
  Function 5: zeta
#>
function Get-ZetaReport {
    param(
        [string]$Path = "C:\Data\zeta",
        [int]$Limit = 5
    )
    $files = Get-ChildItem -Path $Path -Filter "*.log" -ErrorAction SilentlyContinue
    foreach ($file in $files) {
        if ($file.Length -gt $Limit) { Write-Host "Large: $($file.Name)" -ForegroundColor Yellow }
    }
    return $files.Count
}

<#
  Function 6: eta
#>
function Get-EtaReport {
    param(
        [string]$Path = "C:\Data\eta",
        [int]$Limit = 6
    )
    $files = Get-ChildItem -Path $Path -Filter "*.log" -ErrorAction SilentlyContinue
    foreach ($file in $files) {
        if ($file.Length -gt $Limit) { Write-Host "Large: $($file.Name)" -ForegroundColor Yellow }
    }
    return $files.Count
}
