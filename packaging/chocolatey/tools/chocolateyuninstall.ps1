$ErrorActionPreference = 'Stop'

# The ProductCode leads the silent arguments because that is where
# Uninstall-ChocolateyPackage puts them: it runs `msiexec /x <silentArgs>`.
# Uninstalling by display name instead would remove whatever Apps & Features
# happens to match, and the name is not ours to make unique.
$packageArgs = @{
  packageName    = 'pdfluent'
  fileType       = 'msi'
  silentArgs     = '{AEE448FB-9AC3-4519-956E-60B7A232CDA8} /qn /norestart'
  validExitCodes = @(0, 3010, 1605, 1614, 1641)
  file           = ''
}

Uninstall-ChocolateyPackage @packageArgs
