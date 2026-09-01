# Windows-side setup for the WSL desktop CI runner

These four files keep the WSL2 CI runner on the Windows LAN box alive and
usable. Its hostname is deliberately not written here: this tree becomes public
under #222, and a machine name plus the setup below is a map to a box on a home
network. The name is in the private infrastructure notes.
They run on the **Windows** side, because everything they do is impossible from
inside the distro: attaching a physical disk, holding the VM up, and compacting
the virtual disk all require the Windows host.

If the desktop is ever rebuilt, this directory is the recovery path.

## Install

Copy the three `.ps1` files to `C:\Users\Gebruiker\`, and `wslconfig.example`
to `C:\Users\Gebruiker\.wslconfig`. Then register the tasks:

| Task | Trigger | Script |
|---|---|---|
| `PDFluent-WSL-KeepAlive` | at startup, restarts itself | `wsl-keepalive.ps1` |
| `PDFluent-WSL-CorpusDisk` | at startup **and every 15 min** | `attach-corpusdisk.ps1` |
| `PDFluent-WSL-CompactDisk` | Sunday 04:30 | `compact-wsl-disk.ps1` |

All three must run as **`Gebruiker` with `LogonType S4U`** and `RunLevel Highest`.

## Three things that will bite you

Each of these cost real debugging time, and each looked healthy while broken.

### 1. `wsl.exe` does not run as LOCAL SYSTEM

It fails with `WSL_E_LOCAL_SYSTEM_NOT_SUPPORTED`. Registering a task as SYSTEM
appears to succeed, the task reports as `Ready`, it runs on schedule — and does
nothing at all. The original corpus-disk task was registered this way and had
therefore never worked, which is why the disk did not come back after a reboot.

S4U is the right logon type: it runs whether or not the user is logged on, and
needs no stored password.

### 2. WSL shuts the VM down when the last session closes

It counts **attached clients**, not the processes inside. A systemd service such
as the GitLab runner does not keep the VM up. Measured before the fix: 8 boots
in 30 minutes, each lasting 1–3 minutes — exactly the moments someone happened
to run a command.

The symptom is nasty because everything else reads healthy: GitLab calls the
runner "online", `gitlab-runner verify` says "is valid", and the network tests
clean. Only a waiting job shows it — one sat pending for 149 seconds with no
runner assigned. After the fix, pickup was 1.6 seconds.

`vmIdleTimeout=-1` in `.wslconfig` handles it, and the keepalive task is the
second line of defence: the timeout setting governs behaviour that is defined in
terms of clients, so a future WSL version could interpret it differently. An
open session cannot be misinterpreted.

### 3. `df /` lies inside WSL, and the vhdx never shrinks

The root filesystem is a sparse `ext4.vhdx` with a 1 TB default maximum, so
`df /` reports ~945 GB free while the Windows volume hosting it has ~102 GB.
Anything guarding on `df /` reads healthy until Windows itself runs out. Measure
`/mnt/c`.

And freeing space inside the distro does not hand it back to Windows — the vhdx
only grows. That is what `compact-wsl-disk.ps1` is for. WSL can do this
automatically via `--set-sparse`, but Microsoft has disabled that pending a
data-corruption issue and requires `--allow-unsafe`; on a machine whose whole
purpose is trustworthy corpus measurements, that is the wrong trade.

## A note on reading the logs

All three scripts write UTF-8 explicitly. They previously used `Out-File`'s
PowerShell 5.1 default (UTF-16LE), and runs under different accounts produced a
mixed-encoding file that could no longer be read at all — a log you cannot read
is not a log.
