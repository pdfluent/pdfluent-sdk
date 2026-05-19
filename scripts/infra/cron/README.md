# PDFluent — scheduled cleanup specs

Operator-installable cron/launchd specs that wire the dry-run cleanup
scripts in `scripts/infra/*` to a regular schedule. Nothing here runs
automatically until the operator explicitly installs the files.

## Files

| File | Target | Purpose |
| --- | --- | --- |
| `pdfluent-vps-cleanup.cron` | `/etc/cron.d/pdfluent-vps-cleanup` on the Hetzner VPS runner | Daily disk-guard + storage report, weekly runner-cache cleanup (dry-run by default) |
| `pdfluent-cleanup.logrotate` | `/etc/logrotate.d/pdfluent-cleanup` on the same VPS | Rotate the cleanup logs weekly, keep 8 weeks |
| `pdfluent-mac-cleanup.plist` | `~/Library/LaunchAgents/com.pdfluent.cleanup.plist` on the coordinator's Mac | Daily storage report + worktree + artifact cleanup (dry-run by default) |

## Install on the VPS (root)

```bash
# 1. Copy scripts into a stable location the cron expects.
ssh root@<vps>
mkdir -p /opt/pdfluent-cleanup /var/log/pdfluent-cleanup
git clone --branch enterprise/ga-hardening https://github.com/pdfluent/PDFluent-project.git /tmp/pdfluent-repo
cp /tmp/pdfluent-repo/scripts/infra/_lib.sh \
   /tmp/pdfluent-repo/scripts/infra/cleanup_vps_runner_cache.sh \
   /tmp/pdfluent-repo/scripts/infra/disk_guard.sh \
   /tmp/pdfluent-repo/scripts/infra/storage_report.sh \
   /opt/pdfluent-cleanup/
chmod +x /opt/pdfluent-cleanup/*.sh

# 2. Adjust the cron file's script paths if you used a different prefix:
#    `sed -i 's|/tmp/pdfluent-cleanup/|/opt/pdfluent-cleanup/|g' \
#     /tmp/pdfluent-repo/scripts/infra/cron/pdfluent-vps-cleanup.cron`

# 3. Install the cron + logrotate.
install -m 0644 /tmp/pdfluent-repo/scripts/infra/cron/pdfluent-vps-cleanup.cron \
        /etc/cron.d/pdfluent-vps-cleanup
install -m 0644 /tmp/pdfluent-repo/scripts/infra/cron/pdfluent-cleanup.logrotate \
        /etc/logrotate.d/pdfluent-cleanup

# 4. Reload.
systemctl reload cron

# 5. Confirm:
grep -R "pdfluent" /etc/cron.d/ /etc/logrotate.d/
ls -la /var/log/pdfluent-cleanup/
```

Flip `--apply` on `cleanup_vps_runner_cache.sh` AFTER a week of clean
dry-run output (the dry-run line shows exactly what would be removed
and the reclaimable bytes; nothing is touched until `--apply` is
passed).

## Install on the Mac (coordinator)

```bash
cp scripts/infra/cron/pdfluent-mac-cleanup.plist \
   ~/Library/LaunchAgents/com.pdfluent.cleanup.plist
launchctl load ~/Library/LaunchAgents/com.pdfluent.cleanup.plist
```

Logs land in `~/Library/Logs/pdfluent-cleanup.log`. The plist runs at
03:30 local; tweak `StartCalendarInterval` to taste.

## Un-install

```bash
# VPS
sudo rm /etc/cron.d/pdfluent-vps-cleanup /etc/logrotate.d/pdfluent-cleanup
sudo systemctl reload cron

# Mac
launchctl unload ~/Library/LaunchAgents/com.pdfluent.cleanup.plist
rm ~/Library/LaunchAgents/com.pdfluent.cleanup.plist
```

## Safety contract

The shell scripts these specs invoke (`cleanup_vps_runner_cache.sh`,
`cleanup_agent_worktrees.sh`, `cleanup_local_artifacts.sh`) carry the
hard-rule discipline:

- **Dry-run is the default.** No file is removed unless `--apply` is
  passed.
- **Denylist enforced** in `scripts/infra/_lib.sh`. Corpus, oracle
  renders, signed-license fixtures, and any UNKNOWN path are
  fail-closed (never deleted).
- **Pushed-to-gitlab gate** on worktree removal — never wipes a
  worktree whose branch isn't safely on the remote.
- **`storagebox` corpus** is explicitly listed as never-touch.

Nothing in the cron specs above bypasses these guards.
