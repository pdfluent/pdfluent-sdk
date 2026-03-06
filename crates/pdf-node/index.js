// Auto-generated napi-rs loader.
// Loads the correct native binary for the current platform.

const { existsSync, readFileSync } = require('fs');
const { join } = require('path');

const { platform, arch } = process;

let nativeBinding = null;
let localFileExisted = false;
let loadError = null;

function isMusl() {
  // Detect musl libc via /etc/os-release or ldd output.
  if (platform !== 'linux') return false;
  try {
    const data = readFileSync('/etc/os-release', 'utf8');
    return data.includes('Alpine') || data.includes('musl');
  } catch {
    return false;
  }
}

const platformArch = `${platform}-${arch}`;
const musl = isMusl();

const triples = {
  'darwin-x64': 'pdf-node.darwin-x64.node',
  'darwin-arm64': 'pdf-node.darwin-arm64.node',
  'linux-x64': musl
    ? 'pdf-node.linux-x64-musl.node'
    : 'pdf-node.linux-x64-gnu.node',
  'linux-arm64': 'pdf-node.linux-arm64-gnu.node',
  'win32-x64': 'pdf-node.win32-x64-msvc.node',
};

const filename = triples[platformArch];

if (filename) {
  const localPath = join(__dirname, filename);
  try {
    if (existsSync(localPath)) {
      localFileExisted = true;
      nativeBinding = require(localPath);
    } else {
      // Try loading from node_modules (npm install flow).
      nativeBinding = require(`@xfa-engine/pdf-node-${platformArch}`);
    }
  } catch (e) {
    loadError = e;
  }
}

if (!nativeBinding) {
  if (loadError) {
    throw loadError;
  }
  throw new Error(
    `Unsupported platform: ${platformArch}. ` +
      'Please open an issue at https://github.com/jasperdew/xfa-native-rust/issues'
  );
}

module.exports = nativeBinding;
