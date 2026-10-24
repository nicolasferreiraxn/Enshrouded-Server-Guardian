# Prepara a VPS para o deploy por SSH com chave (sem senha).
# Rodar na VPS, no PowerShell como administrador:
#   powershell -ExecutionPolicy Bypass -File configurar-ssh.ps1
# Para desfazer depois do deploy:  Stop-Service sshd; Set-Service sshd -StartupType Disabled
$ErrorActionPreference = "Stop"
$Chave = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILNnSNeZva5hiBrydh5IRBF+JeMdrMB31Hd4cEyrfbiD deploy-enshrouded-vps"

$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $admin) { throw "Rode este script como administrador." }

Write-Host "== Servidor OpenSSH"
if (-not (Get-Service sshd -ErrorAction SilentlyContinue)) {
    # instalador oficial da Microsoft (PowerShell/Win32-OpenSSH); não depende do Windows Update
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    $rel = Invoke-RestMethod "https://api.github.com/repos/PowerShell/Win32-OpenSSH/releases/latest" -Headers @{ "User-Agent" = "deploy" }
    $msi = $rel.assets | Where-Object { $_.name -like "OpenSSH-Win64-*.msi" } | Select-Object -First 1
    if (-not $msi) { throw "Não achei o instalador OpenSSH-Win64 no GitHub." }
    $arq = Join-Path $env:TEMP $msi.name
    Write-Host "baixando $($msi.name)"
    Invoke-WebRequest $msi.browser_download_url -OutFile $arq
    Start-Process msiexec.exe -ArgumentList "/i", "`"$arq`"", "/qn", "ADDLOCAL=Server" -Wait
    if (-not (Get-Service sshd -ErrorAction SilentlyContinue)) { throw "A instalação do OpenSSH não criou o serviço sshd." }
}
Start-Service sshd
Set-Service sshd -StartupType Automatic

Write-Host "== Chave autorizada (só a do deploy)"
$k = "C:\ProgramData\ssh\administrators_authorized_keys"
Set-Content $k $Chave -Encoding ascii
# SIDs em vez de nomes: "Administradores"/"Administrators" muda com o idioma do Windows
icacls $k /inheritance:r /grant "*S-1-5-32-544:F" /grant "*S-1-5-18:F" | Out-Null

Write-Host "== Sem login por senha; PowerShell como shell"
$c = "C:\ProgramData\ssh\sshd_config"
$cfg = Get-Content $c
if ($cfg -match '^#?PasswordAuthentication') {
    $cfg = $cfg -replace '^#?PasswordAuthentication .*', 'PasswordAuthentication no'
} else { $cfg += 'PasswordAuthentication no' }
Set-Content $c $cfg -Encoding ascii
New-ItemProperty -Path "HKLM:\SOFTWARE\OpenSSH" -Name DefaultShell `
    -Value "C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe" -PropertyType String -Force | Out-Null
Restart-Service sshd

Write-Host "== Firewall do Windows: TCP 22"
if (-not (Get-NetFirewallRule -Name "OpenSSH-Server-In-TCP" -ErrorAction SilentlyContinue)) {
    New-NetFirewallRule -Name "OpenSSH-Server-In-TCP" -DisplayName "OpenSSH (22)" -Direction Inbound `
        -Protocol TCP -LocalPort 22 -Action Allow | Out-Null
}

Write-Host "`n== Pronto. Mande estes dados no chat:"
Write-Host "usuario: $(whoami)"
$ip = try { (Invoke-RestMethod "https://api.ipify.org" -TimeoutSec 10) } catch { "(veja no painel do provedor)" }
Write-Host "ip publico: $ip"
Write-Host "Lembre de liberar no painel do provedor: TCP 22 (de preferência só o seu IP) e UDP 9021 (jogo)."
