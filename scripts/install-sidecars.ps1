# Загрузка yt-dlp и ffmpeg в ./sidecars. Использовать перед `cargo tauri dev`/`build`.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$dst = Join-Path $root 'sidecars'
New-Item -ItemType Directory -Force -Path $dst | Out-Null

function Download($url, $out) {
    Write-Host "→ $url"
    Invoke-WebRequest -Uri $url -OutFile $out -UseBasicParsing
}

# yt-dlp (windows x64, последний релиз)
$yt = Join-Path $dst 'yt-dlp.exe'
if (-not (Test-Path $yt)) {
    Download 'https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe' $yt
}

# ffmpeg (essentials build)
$ff = Join-Path $dst 'ffmpeg.exe'
if (-not (Test-Path $ff)) {
    $zip = Join-Path $dst 'ffmpeg.zip'
    Download 'https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip' $zip
    Expand-Archive -Path $zip -DestinationPath $dst -Force
    $found = Get-ChildItem -Path $dst -Recurse -Filter 'ffmpeg.exe' | Where-Object { $_.DirectoryName -notlike '*\doc\*' } | Select-Object -First 1
    if ($null -ne $found) { Move-Item -Force $found.FullName $ff }
    Remove-Item -Force $zip -ErrorAction SilentlyContinue
    Get-ChildItem -Path $dst -Recurse -Directory -Filter 'ffmpeg-*' | Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host "✔ Done. Sidecars at $dst"
Get-Item $yt, $ff | Format-Table Name, Length
