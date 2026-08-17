# Geeft ruimte die binnen WSL is vrijgemaakt terug aan de C:-schijf.
#
# WAAROM DIT NODIG IS
#
# De virtuele schijf van WSL (ext4.vhdx) groeit wel maar krimpt nooit. De
# dagelijkse opruiming binnen WSL maakt dus wel plaats IN de virtuele schijf,
# maar Windows ziet daar niets van terug: het vhdx-bestand blijft op zijn
# hoogste waarde staan. Zonder deze stap ziet opruimen effectief uit terwijl C:
# alleen maar verder volloopt.
#
# WSL kan dit ook automatisch (`--set-sparse`), maar Microsoft heeft die functie
# uitgezet wegens mogelijke datacorruptie en vraagt `--allow-unsafe`. Op een
# machine die betrouwbare corpusmetingen moet leveren is dat de verkeerde ruil,
# dus doen we het expliciet en op een rustig moment.
#
# ORDE VAN HANDELINGEN, EN WAAROM
#
#   1. fstrim binnen WSL      - zonder dit zijn vrijgekomen blokken voor de
#                               virtuele schijf niet als vrij te herkennen en
#                               levert compacteren bijna niets op.
#   2. wsl --shutdown         - compacteren kan niet op een schijf in gebruik.
#   3. diskpart compact vdisk - geeft de blokken terug aan C:.
#   4. distro + schijf terug  - anders staat de CI stil tot iemand het merkt.
$ErrorActionPreference = "Continue"

$VHDX = "C:\Users\Gebruiker\AppData\Local\wsl\{d5de22f3-a70a-4aa0-978f-9c24064063c3}\ext4.vhdx"
$log  = "C:\Users\Gebruiker\wsl-compact.log"
$tmp  = "$env:TEMP\compact-wsl.out"

function Log($m) { "$(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')  $m" | Out-File -Append -Encoding utf8 $log }
function VhdxGB { [math]::Round((Get-Item $VHDX).Length / 1GB, 2) }

function Invoke-Wsl($bashLine) {
    cmd /c "wsl -d Ubuntu -u root bash -lc `"$bashLine`" > `"$tmp`" 2>&1"
    $script:LastWslExit = $LASTEXITCODE
    if (Test-Path $tmp) { return (Get-Content $tmp -Raw) } else { return "" }
}

Log "start (vhdx nu $(VhdxGB) GB)"

# Nooit lopend werk afbreken. Een halve corpusrun kost meer dan een week
# wachten op de volgende compactie.
#
# rsync staat er expliciet bij: de compactie valt op zondag 04:30, midden in
# het nachtelijke overdrachtvenster (23:00-07:00). Zonder deze regel legt de
# compactie WSL stil terwijl er een overdracht van honderden GB's loopt.
# De eerste letter tussen blokhaken is geen sierlijkheid: `pgrep -f` kijkt naar
# hele commandoregels, en de shell die dit patroon uitvoert heeft het patroon
# zelf in zijn commandoregel staan. Zonder de blokhaken vindt pgrep dus altijd
# zichzelf en slaat het compacteren voor eeuwig over.
$busy = (Invoke-Wsl "pgrep -f '[g]itlab-runner-helper|[c]argo build|[c]argo test|[r]ustc |[r]sync |[n]ight_corpus_sync' >/dev/null && echo BEZIG || echo VRIJ").Trim()
if ($busy -match "BEZIG") {
    Log "AFGEBROKEN: er loopt CI-werk; compacteren overgeslagen tot de volgende ronde"
    exit 0
}

$before = VhdxGB

Log "stap 1/4: fstrim binnen WSL"
$trim = (Invoke-Wsl "fstrim -av 2>&1 | head -5").Trim()
foreach ($line in ($trim -split "`r?`n" | Where-Object { $_ })) { Log "  $line" }

Log "stap 2/4: WSL stilleggen"
cmd /c "wsl --shutdown > `"$tmp`" 2>&1"
Start-Sleep -Seconds 10

Log "stap 3/4: virtuele schijf compacteren"
$script = @"
select vdisk file="$VHDX"
attach vdisk readonly
compact vdisk
detach vdisk
exit
"@
$scriptFile = "$env:TEMP\compact-vdisk.txt"
$script | Out-File -FilePath $scriptFile -Encoding ASCII
cmd /c "diskpart /s `"$scriptFile`" > `"$tmp`" 2>&1"
if (Test-Path $tmp) {
    foreach ($line in ((Get-Content $tmp -Raw) -split "`r?`n" | Where-Object { $_ -match "\S" })) { Log "  diskpart: $line" }
}
Remove-Item $scriptFile -ErrorAction SilentlyContinue

$after = VhdxGB
Log "vhdx: $before GB -> $after GB (teruggegeven aan C:: $([math]::Round($before - $after, 2)) GB)"

Log "stap 4/4: distro en corpusschijf terugbrengen"
& powershell.exe -NoProfile -ExecutionPolicy Bypass -File "C:\Users\Gebruiker\attach-corpusdisk.ps1"

# Hard verifieren dat de CI weer staat. Stil falen hier betekent een runner die
# offline is zonder dat iemand het ziet -- precies wat we willen voorkomen.
$state = (Invoke-Wsl "systemctl is-active gitlab-runner; mountpoint -q /mnt/storagebox && echo CORPUS-OK || echo CORPUS-WEG").Trim()
Log "eindtoestand: $($state -replace "`r?`n", ' | ')"
if ($state -notmatch "active" -or $state -match "CORPUS-WEG") {
    Log "FOUT: CI niet volledig terug na compacteren - handmatig nakijken"
    exit 1
}
Log "klaar"
exit 0
