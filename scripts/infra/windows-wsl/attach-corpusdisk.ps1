# Koppelt de externe corpus-schijf door naar WSL en zorgt dat /mnt/storagebox
# bestaat, zodat systemd-services (GitLab-runner) met een werkend corpus starten.
#
# Draait bij het opstarten van Windows EN periodiek. Dat tweede is nodig: een
# `wsl --shutdown` laat de doorgekoppelde schijf vallen, en dan is /mnt/storagebox
# weg terwijl Windows helemaal niet herstart is. Het script is idempotent, dus
# vaak draaien is gratis.
#
# De schijf wordt op MODEL gezocht, niet op nummer: PHYSICALDRIVE-nummers
# verschuiven zodra er een schijf bij komt of weggaat.
#
# WAAROM DE CONTROLE OP UUID GAAT, NIET OP APPARAATLETTER
#
# De vorige versie testte `test -b /dev/sdd` om te zien of de schijf al gekoppeld
# was. Apparaatletters in WSL verschuiven echter: na een herstart was /dev/sdd de
# INTERNE WSLg-schijf, waardoor de test slaagde, de mount werd overgeslagen, en
# /mnt/storagebox stilzwijgend leeg bleef. De CI draaide door en zou tegen een
# niet-bestaand corpus meten. Een UUID verschuift niet.
#
# WAAROM ALLE WSL-UITVOER VIA EEN BESTAND GAAT
#
# wsl.exe blokkeert wanneer zijn uitvoer in een pipe zonder console loopt (zoals
# in een SSH-sessie). Deze taak draait normaal onder de takenplanner, waar dat
# niet speelt -- maar dan is het script alleen in productie testbaar, en dat is
# hoe de vorige fout maanden onopgemerkt bleef. Met bestandsomleiding werkt het
# in beide gevallen.
$ErrorActionPreference = "Continue"

$CORPUS_UUID = "1ac86527-872e-4759-9902-b9c6a46fbb93"
$DISK_MODEL  = "*WD Ext HDD*"
$log         = "C:\Users\Gebruiker\wsl-boot.log"
$tmp         = "$env:TEMP\attach-corpusdisk.out"

function Log($m) { "$(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')  $m" | Out-File -Append -Encoding utf8 $log }

# Voert een bash-regel uit in WSL en geeft de uitvoer terug; exitcode in $script:LastWslExit.
function Invoke-Wsl($bashLine) {
    cmd /c "wsl -d Ubuntu -u root bash -lc `"$bashLine`" > `"$tmp`" 2>&1"
    $script:LastWslExit = $LASTEXITCODE
    if (Test-Path $tmp) { return (Get-Content $tmp -Raw) } else { return "" }
}

Log "start"

# Is de schijf al binnen WSL zichtbaar op zijn UUID?
Invoke-Wsl "blkid -U $CORPUS_UUID" | Out-Null
if ($script:LastWslExit -eq 0) {
    Log "schijf al doorgekoppeld (UUID gevonden)"
} else {
    $disk = Get-CimInstance Win32_DiskDrive | Where-Object { $_.Model -like $DISK_MODEL }
    if (-not $disk)        { Log "FOUT: corpus-schijf niet gevonden - niets gekoppeld"; exit 1 }
    if ($disk -is [array]) { Log "FOUT: meerdere schijven passen op het model; handmatig nakijken"; exit 1 }

    Log "koppelen: $($disk.Model) als $($disk.DeviceID)"
    cmd /c "wsl --mount $($disk.DeviceID) --bare > `"$tmp`" 2>&1"
    if (Test-Path $tmp) { (Get-Content $tmp -Raw).Trim() -split "`r?`n" | Where-Object { $_ } | ForEach-Object { Log "  wsl --mount: $_" } }

    Invoke-Wsl "blkid -U $CORPUS_UUID" | Out-Null
    if ($script:LastWslExit -ne 0) {
        Log "FOUT: na --mount is de UUID nog niet zichtbaar; schijf niet bruikbaar"
        exit 1
    }
}

# fstab verwerken (mount op UUID, met nofail) en het resultaat hard verifieren.
#
# Met herhaling, want vlak na `wsl --mount --bare` is de schijf zelf al wel
# zichtbaar terwijl de kernel de partitie erop nog niet heeft doorgenomen. Een
# enkele `mount -a` valt dan net te vroeg en faalt stil: de schijf is gekoppeld,
# /mnt/storagebox blijft leeg, en de CI zou tegen een leeg corpus meten.
# WAAROM HET WACHTVENSTER RUIM IS
#
# Op 21-08-2026 duurde het na een `wsl --shutdown` plus `wsl --unmount` 85
# seconden voor de kernel de partitie had doorgenomen: vijf keer
# `/dev/sde1: Can't open blockdev` en pas daarna de EXT4-mount. Het venster
# stond toen op 6 x 5s = 30s, dus het script gaf op terwijl er niets mis was,
# meldde FOUT, en liet de machine zonder corpus achter. Twee minuten kost niets
# -- dit draait bij het opstarten en periodiek -- en dekt de trage kant ruim.
$mounted = $false
for ($poging = 1; $poging -le 24; $poging++) {
    Invoke-Wsl "mount -a" | Out-Null
    $df = (Invoke-Wsl "mountpoint -q /mnt/storagebox && df -h /mnt/storagebox | tail -1 || echo NIET-GEKOPPELD").Trim()
    if ($df -notmatch "NIET-GEKOPPELD" -and $df) {
        Log "GEKOPPELD (poging $poging): $df"
        $mounted = $true
        break
    }
    Log "  poging ${poging}/24: partitie nog niet klaar, 5s wachten ($($poging * 5)s verstreken)"
    Start-Sleep -Seconds 5
}

if (-not $mounted) {
    Log "FOUT: /mnt/storagebox is na 24 pogingen (2 minuten) geen mountpoint"
    Invoke-Wsl "lsblk -o NAME,SIZE,FSTYPE,UUID" | ForEach-Object { Log "  lsblk: $_" }
    exit 1
}
exit 0
