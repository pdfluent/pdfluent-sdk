# Houdt een WSL-sessie open zodat de virtuele machine blijft draaien.
#
# vmIdleTimeout=-1 in .wslconfig hoort dit al te regelen, maar deze taak is de
# tweede lijn: WSL legt de machine stil op basis van AANGEKOPPELDE CLIENTS, niet
# op basis van de processen erin, en dat is precies de aanname waar de CI eerder
# op stukliep. Een openstaande sessie is de enige garantie die niet afhangt van
# hoe een toekomstige WSL-versie die instelling uitlegt.
#
# De taak start bij het opstarten van Windows en herstart zichzelf als hij
# stopt. Het commando doet niets en kost niets.
$ErrorActionPreference = "Continue"
$log = "C:\Users\Gebruiker\wsl-keepalive.log"
"$(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')  keepalive gestart" | Out-File -Append -Encoding utf8 $log

while ($true) {
    # Blokkeert zolang de distro leeft; komt terug als die alsnog stopt.
    #
    # De omleiding staat BINNEN cmd en gaat naar NUL. Met PowerShells `*> $null`
    # eromheen stopt wsl.exe onmiddellijk in plaats van te blokkeren -- gemeten:
    # de lus draaide dan elke 10 seconden rond zonder ooit een sessie te
    # houden, terwijl exact hetzelfde commando met bestandsomleiding netjes
    # bleef staan. wsl.exe is kieskeurig over waar zijn uitvoer heen gaat.
    cmd /c "wsl -d Ubuntu -u root -- sleep infinity > NUL 2>&1"
    "$(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')  sessie viel weg, opnieuw verbinden" | Out-File -Append -Encoding utf8 $log
    Start-Sleep -Seconds 10
}
