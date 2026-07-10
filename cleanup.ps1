# Cleanup script for Ghostlink Studio
# Kills all running services and frees up ports

Write-Host "Cleaning up Ghostlink Studio processes..." -ForegroundColor Cyan

# Kill Ollama
Get-Process | Where-Object { $_.ProcessName -eq 'ollama' } | Stop-Process -Force -ErrorAction SilentlyContinue
Write-Host "[✓] Ollama stopped" -ForegroundColor Green

# Kill ghost-link backend
Get-Process | Where-Object { $_.CommandLine -like '*ghost-link*' } | Stop-Process -Force -ErrorAction SilentlyContinue
Write-Host "[✓] Ghostlink backend stopped" -ForegroundColor Green

# Kill Vite processes
Get-Process | Where-Object { $_.ProcessName -like '*vite*' -or $_.CommandLine -like '*vite*' } | Stop-Process -Force -ErrorAction SilentlyContinue
Write-Host "[✓] Vite processes stopped" -ForegroundColor Green

# Force kill any remaining processes on port 5173
Get-NetTCPConnection -LocalPort 5173 -ErrorAction SilentlyContinue | ForEach-Object {
    Stop-Process -Id $_.OwningProcess -Force
}
Write-Host "[✓] Port 5173 cleared" -ForegroundColor Green

# Force kill any remaining processes on port 8003
Get-NetTCPConnection -LocalPort 8003 -ErrorAction SilentlyContinue | ForEach-Object {
    Stop-Process -Id $_.OwningProcess -Force
}
Write-Host "[✓] Port 8003 cleared" -ForegroundColor Green

# Force kill any remaining processes on port 11434
Get-NetTCPConnection -LocalPort 11434 -ErrorAction SilentlyContinue | ForEach-Object {
    Stop-Process -Id $_.OwningProcess -Force
}
Write-Host "[✓] Port 11434 cleared" -ForegroundColor Green

Write-Host ""
Write-Host "All services stopped successfully!" -ForegroundColor Green
Write-Host "You can now restart Ghostlink Studio with: bash launch-complete.sh" -ForegroundColor Yellow
