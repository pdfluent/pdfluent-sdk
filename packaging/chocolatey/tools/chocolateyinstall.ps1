$ErrorActionPreference = 'Stop'

$packageArgs = @{
  packageName    = 'pdfluent'
  fileType       = 'msi'
  url64bit       = 'https://pdfluent.com/releases/1.0.0-beta.21/PDFluent_1.0.0-beta.21_x64_en-US.msi'
  checksum64     = 'F6262AC4EF35920F0B6D2992DD9BF4701A21751309A85B1E938533AF10364F12'
  checksumType64 = 'sha256'
  # The MSI shows its interface from InstallUISequence only, and the one custom
  # action that opens a window (LaunchApplication) is conditioned on
  # AUTOLAUNCHAPP, which nothing but the finish-page checkbox sets. /quiet is
  # therefore silent by construction rather than by hope.
  silentArgs     = '/quiet /norestart'
  validExitCodes = @(0, 3010, 1641)
}

# The installer fetches the Microsoft Edge WebView2 runtime from
# go.microsoft.com when WVRTINSTALLED is not already set, so this needs a
# network connection even though the MSI itself is on disk by now.
Install-ChocolateyPackage @packageArgs
