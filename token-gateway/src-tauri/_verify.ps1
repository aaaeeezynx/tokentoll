$ErrorActionPreference = 'Continue'
$tauri = 'D:\token counter\token-gateway\src-tauri'
$src = "$tauri\src"
Set-Location $tauri

function Get-Conflicts() {
    $c = @()
    foreach ($f in Get-ChildItem -File -Path $src -Filter *.rs) {
        $m = Join-Path $src "$($f.BaseName)\mod.rs"
        if (Test-Path $m) { $c += $f.Name }
    }
    return , $c
}

$ok = $false
for ($i = 1; $i -le 60; $i++) {
    $cf = Get-Conflicts
    if ($cf.Count -gt 0) {
        Write-Output "wait #$i : module file conflicts: $($cf -join ', ')"
        Start-Sleep -Seconds 15
        continue
    }
    Write-Output "attempt #$i : cargo check --offline --all-targets"
    $out = (& cargo check --offline --all-targets 2>&1 | Out-String)
    $errs = ([regex]::Matches($out, '(?m)^error(\[|:)')).Count
    if ($errs -eq 0 -and $out -match 'Finished') {
        Write-Output 'CHECK_OK'
        $ok = $true
        break
    }
    Write-Output "check not clean ($errs errors). excerpt:"
    ($out -split "`n" | Select-String -Pattern '^(error|warning)' | Select-Object -First 10) | ForEach-Object { Write-Output "  $_" }
    Start-Sleep -Seconds 15
}

if (-not $ok) { Write-Output 'VERIFY_GAVE_UP'; exit 2 }

Write-Output '=== TEST ==='
$t = (& cargo test --offline 2>&1 | Out-String)
($t -split "`n" | Select-String -Pattern 'test result:|^error|running \d+ tests') | ForEach-Object { Write-Output $_ }

Write-Output '=== CLIPPY ==='
$c = (& cargo clippy --offline --all-targets 2>&1 | Out-String)
$warnLines = ($c -split "`n" | Select-String -Pattern '^(warning|error)' )
Write-Output "clippy warning/error line count: $($warnLines.Count)"
$warnLines | ForEach-Object { Write-Output $_ }
Write-Output '--- clippy file locations ---'
($c -split "`n" | Select-String -Pattern '--> src\\') | ForEach-Object { Write-Output $_ }
Write-Output 'DONE'
