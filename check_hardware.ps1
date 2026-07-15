Write-Host "=== Ghostlink Hardware Detection Check ===" -ForegroundColor Cyan
Write-Host ""

Write-Host "--- GPU Detection ---" -ForegroundColor Yellow
$gpus = Get-CimInstance Win32_VideoController
foreach ($gpu in $gpus) {
    $name = $gpu.Name
    $ram = if ($gpu.AdapterRAM -and $gpu.AdapterRAM -gt 0) { "{0:N1} GB" -f ($gpu.AdapterRAM / 1GB) } else { "Unknown" }
    Write-Host "  GPU: $name" -ForegroundColor Green
    Write-Host "  VRAM: $ram"
    Write-Host ""
}

Write-Host "--- NPU Detection ---" -ForegroundColor Yellow
$npus = Get-CimInstance -Namespace "root\cimv2" -ClassName Win32_PnPEntity -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -match '(NPU|Neural|AI Accelerator|XDNA|Ryzen AI)' }
if ($npus) {
    foreach ($npu in $npus) {
        Write-Host "  NPU: $($npu.Name)" -ForegroundColor Green
        Write-Host "  Status: $($npu.Status)"
    }
} else {
    Write-Host "  No NPU detected" -ForegroundColor Yellow
}
Write-Host ""

Write-Host "--- Backend Binary ---" -ForegroundColor Yellow
if (Test-Path "target\release\ghost-link.exe") {
    Write-Host "  Release build: Found" -ForegroundColor Green
} else {
    Write-Host "  Release build: Not found (run: cargo build --release -p ghost-link)" -ForegroundColor Red
}
if (Test-Path "target\debug\ghost-link.exe") {
    Write-Host "  Debug build: Found" -ForegroundColor Green
} else {
    Write-Host "  Debug build: Not found" -ForegroundColor Yellow
}

Write-Host ""
Write-Host "--- llama-server ---" -ForegroundColor Yellow
$llamaPaths = @(
    "third_party\llama.cpp\build\bin\Release\llama-server.exe",
    "third_party\llama.cpp\build\bin\llama-server.exe"
)
$found = $false
foreach ($p in $llamaPaths) {
    if (Test-Path $p) {
        Write-Host "  $p : Found" -ForegroundColor Green
        $found = $true
    }
}
if (-not $found) {
    Write-Host "  llama-server: Not built" -ForegroundColor Red
}

Write-Host ""
Write-Host "--- Model ---" -ForegroundColor Yellow
if (Test-Path "models\*.gguf") {
    Get-ChildItem "models\*.gguf" | ForEach-Object {
        Write-Host "  $($_.Name) ($( '{0:N1} MB' -f ($_.Length / 1MB) ))" -ForegroundColor Green
    }
} else {
    Write-Host "  No GGUF models found in models/" -ForegroundColor Yellow
}

Write-Host ""
Write-Host "=== Summary ===" -ForegroundColor Cyan
if ($gpus) {
    Write-Host "  GPU: Detected - DirectML/Vulkan backend recommended" -ForegroundColor Green
} else {
    Write-Host "  GPU: Not detected - CPU mode" -ForegroundColor Yellow
}
if ($npus) {
    Write-Host "  NPU: Detected - AMD Ryzen AI acceleration available" -ForegroundColor Green
}
Write-Host ""
Write-Host "To launch: .\launch-fast.bat" -ForegroundColor White
