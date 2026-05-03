# Jamboree — Security Setup Addendum (v5)

**Companion to:** proposal-v5.md
**Status:** Implementation-ready
**Scope:** WSL2 (Ubuntu/Debian) — also applies to native Linux
**Posture:** Convenience-first multi-user isolation. No Docker dependency.

This addendum sets up the Linux user accounts that Jamboree's substrate (`maestro`) and Pickers (`picker`) run as, and the sudoers rules that let the human Manager (`caleb`) drive both without password prompts.

---

## 0. What this addendum covers

The v5 spec assumes multi-user isolation is in place but does not document the setup. This addendum fills that gap. It describes:

- The threat model the multi-user setup defends against.
- The filesystem and permission layout.
- The bootstrap script (`bootstrap-users.sh`) that establishes the layout.
- The manual GPG/`pass` initialization that follows.
- Operational patterns for day-to-day use.
- What changes for the v5 implementation (paths, spawn logic, code references).
- Migration from a single-user setup.
- Recovery, gotchas, and `jam doctor` integration.

The model is deliberately convenience-first: NOPASSWD sudo between users, no per-task user creation, no Docker required. This is appropriate for a single-developer machine where the human user's session is already trusted. It buys substantial defense against the most realistic attacks (prompt-injection-driven exfiltration, worker-running-rogue) at low operational cost. Hardening to a per-task or per-worktree user model is possible later by enabling the Docker backend (§6.2 in v5) without changing this baseline.

---

## 1. Threat model

The orchestrator runs many sandboxed coding agents in parallel. Realistic threats by likelihood:

**1. Prompt injection driving secret exfiltration.** A CodeRabbit comment, MCP response, web-search result, or PR description contains text that induces the worker LLM to do something it wasn't asked. "Exfiltrate creds" is a known attack pattern: read SSH keys, AWS credentials, browser session tokens, and post them somewhere. **High likelihood.**

**2. Worker going off-rails on its own code.** Worker writes garbage to its worktree, deletes files, runs unexpected `cargo install` of dependencies. Bounded by worktree creation protocol; minor recovery cost. **Medium likelihood.**

**3. Worker pushing malicious code.** Worker pushes a hidden change to `main`, or to an unrelated repo it has push access to. Doesn't require malice — induced by injection. **Medium likelihood.**

**4. Worker burning money.** Worker calls expensive APIs in a loop. Bounded by quota tracker for instrumented harnesses; budget-cap escapes from this protection are possible if the worker calls non-orchestrated APIs. **Low likelihood given Caleb's setup.**

The multi-user model targets threat #1 directly, threats #2 and #3 partially, and is orthogonal to #4 (which is handled by quota tracking and per-session budgets in v5).

What this model does **not** defend against:

- An attacker with shell access as the human user. NOPASSWD sudo to `maestro` means anyone in your terminal can become `maestro`. That's fine — if they have your shell, you have bigger problems.
- Kernel-level escapes. WSL shares a kernel with Windows; a kernel-level container escape would bypass user separation. Acceptable risk for solo dev workstation.
- Malicious software you install yourself. If you `pip install` a package that exfiltrates data, no amount of user separation helps.
- Network-level attacks on outbound traffic. The orchestrator's hardened profile (Docker backend, network namespace) addresses this; the convenience-first model does not.

---

## 2. Filesystem layout

```
/home/caleb/                                    mode 751   caleb:caleb
├── .ssh/                                       mode 700   caleb:caleb        (maestro cannot read)
├── .gnupg/                                     mode 700   caleb:caleb        (maestro cannot read)
├── .password-store/                            mode 700   caleb:caleb        (caleb's personal pass — separate from maestro's)
├── .config/                                    mode 700   caleb:caleb
├── .jam-cli/                                  mode 750   caleb:caleb        (caleb's CLI client config; preferences, UI tokens)
└── code/                                       mode 755   caleb:caleb        (Caleb's project workspace)
    ├── blueberry/                              mode 755   caleb:caleb        (PRISTINE — maestro never writes here)
    ├── blueberry-tempyr-live/                  mode 2770  caleb:maestro     (canonical Tempyr worktree, shared)
    │   ├── tempyr/nodes/                       mode 2770                     (caleb writes; maestro reads)
    │   ├── tempyr/specs/                       mode 2770                     (caleb writes; maestro reads)
    │   └── tempyr/tasks/                       mode 2770                     (maestro writes; caleb reads)
    └── jam-skills/                            mode 2770  caleb:maestro     (skills git repo, shared)

/home/maestro/                                 mode 750   maestro:maestro
├── .gnupg/                                     mode 700   maestro:maestro  (maestro's GPG keyring)
├── .password-store/                            mode 700   maestro:maestro  (maestro's pass — orchestrator secrets)
└── .jam/                                      mode 750   maestro:maestro
    ├── config/                                                                (TOML configs, harness lockfiles)
    ├── journal/                                                               (rotated JSONL journal files)
    ├── session-store.db                                                       (SQLite + FTS5 derived view)
    ├── research/                                                              (research output dirs per task)
    ├── incidents/                                                             (patch agent failure dumps)
    ├── conductor-aborted-sessions/                                            (hard-abort session dumps)
    ├── skills-evolution-candidates/                                           (pending skill diffs)
    ├── staging/                                                               (binaries staged for hot-patching)
    ├── nats-data/                                                             (JetStream durable state)
    ├── tempyr-update-queue.jsonl                                              (Tempyr edit candidates)
    └── harness-update-queue.jsonl                                             (harness version updates)

/home/picker/                              mode 750   picker:picker
└── workers/                                    mode 750   picker:picker
    └── <task-id>/                              mode 700   picker:picker  (per-worker worktree, isolated even from other workers)

/etc/sudoers.d/jam-users                       mode 440   root:root
/etc/jam/bootstrap.log                   mode 644   root:root          (audit record of bootstrap)
```

**Key permission decisions:**

- `/home/caleb` is `751` rather than `750`. This lets `maestro` traverse to known shared subdirs (e.g., `~caleb/code/blueberry-tempyr-live/`) without being able to enumerate the rest of caleb's home. `.ssh`, `.gnupg`, and `.password-store` remain `700` so the contents are still protected.

- Shared dirs use group `maestro` with mode `2770`. The leading `2` is the **setgid** bit: new files created in these directories inherit the directory's group, so caleb-created files in `tempyr/nodes/` are still group-readable by maestro without manual `chgrp`. Both `caleb` (as owner) and `maestro` (as group) have full rwx; others have nothing.

- Per-worker worktrees are `700`. Even though all workers share UID `picker`, the worktree directory permissions prevent one worker from reading another's mid-task state. Combined with sandbox `cwd` enforcement, this is "soft per-worker isolation" without requiring per-task user creation.

- `/home/maestro` is `750` with no group share. Caleb cannot directly read maestro's home. To access maestro's files, caleb sudos in (`sudo -u maestro -i`).

---

## 3. Sudoers configuration

The bootstrap script installs `/etc/sudoers.d/jam-users`:

```
caleb    ALL=(maestro)    NOPASSWD: ALL
caleb    ALL=(maestro)    SETENV: ALL
caleb    ALL=(picker) NOPASSWD: ALL
caleb    ALL=(picker) SETENV: ALL

maestro ALL=(picker) NOPASSWD: ALL
maestro ALL=(picker) SETENV: ALL

Defaults!/usr/bin/* setenv
```

**What each line allows:**

- `caleb -> maestro`: caleb runs ops commands without password (start/stop services, inspect journal, run `jam doctor`).
- `caleb -> picker`: caleb inspects worker state without password (debug a stuck worker, look at worktree contents).
- `maestro -> picker`: the orchestrator spawns workers via `sudo -u picker` from its harness adapters. Required for the worker process to run under the unprivileged identity.
- `SETENV` on each transition: required for the harness adapter to pass `JAM_TRACE_ID`, `JAM_PARENT_TRACE_ID`, secrets, etc., into the worker's environment via `sudo --preserve-env=KEY1,KEY2`.

**What it deliberately does not allow:**

- `picker` cannot sudo to anything. The lowest-privilege identity stays that way.
- `maestro` cannot sudo to root or to `caleb`. The orchestrator never needs root; if it does, that's a bug.
- No commands matching `/bin/sh` or `/bin/bash` directly — these aren't restricted, but the model relies on the user-target restriction rather than command restriction.

The "convenience over airtight" cost: anyone who gets a shell as `caleb` (e.g., via stolen SSH key) immediately becomes `maestro` with no further auth. For solo dev this is acceptable; tightening to require a passphrase would mean typing it many times per day.

---

## 4. The bootstrap script

`bootstrap-users.sh` is the prerequisite to running `jam setup`. It is idempotent — safe to re-run.

### 4.1 Usage

```bash
# One-time, from a checkout of the orchestrator repo:
sudo ./bootstrap-users.sh                  # interactive, uses $SUDO_USER
sudo ./bootstrap-users.sh --user caleb     # explicit user
sudo ./bootstrap-users.sh --dry-run        # show what would happen
sudo ./bootstrap-users.sh --verify-only    # check existing setup
```

### 4.2 What it does

In order, with each step's success/failure surfaced:

1. **Preflight checks.** Verifies the script is running as root, on Linux, with a valid human user account whose home is on a native filesystem (refuses Windows mounts per §2.14 of v5).

2. **Creates service users.** `useradd` for `maestro` (UID 2000) and `picker` (UID 2001). Skips if users already exist with the right UID; warns if UIDs differ.

3. **Adds human user to maestro group.** Required for the human to access shared dirs (skills, canonical Tempyr worktree). Group changes don't apply to existing shells — log out and back in, or run `newgrp maestro`.

4. **Normalizes home directory permissions.** Sets `/home/caleb` to `751` (traversable, not enumerable). Re-confirms `.ssh` and `.gnupg` are `700`.

5. **Writes sudoers config.** Generates the rules above; validates with `visudo -c` before installing; refuses to install if validation fails.

6. **Prepares maestro and picker home directory scaffolding.** Creates the standard `.jam/` subdirectory structure under `/home/maestro/`, prepares `/home/picker/workers/`. Does not initialize GPG/pass (manual step).

7. **Writes audit log.** `/etc/jam/bootstrap.log` records the setup metadata (UIDs, timestamps, script version) for `jam doctor` to verify against later.

8. **Verification phase.** Runs the same checks as `--verify-only`; flags anything still wrong.

### 4.3 What it does not do

- Initialize GPG keyrings or `pass` stores. These are manual one-time steps documented in §5 below.
- Move existing data from a single-user `~/.jam/` setup to `~maestro/.jam/`. See §8 for migration.
- Configure NATS or any orchestrator service. That's `jam setup`'s job.
- Install harness binaries. Done as `caleb` via the harness installers, then verified into `harnesses.lock`.

### 4.4 Failure modes

Each preflight check fails with a specific error and remediation hint, matching the pattern from v5 §11.4 (`jam setup`). Examples:

- "User '<name>' home is on a Windows mount" → tells you to `usermod -m -d /home/<name>`.
- "Sudoers config failed validation" → shows the `visudo -c` errors so you can fix and retry.
- "Group membership applied but not active in this shell" → tells you to log out and back in.

If you suspect partial state, run `--verify-only` for a non-destructive audit.

---

## 5. Manual setup: GPG and pass per user

The bootstrap script intentionally does not initialize GPG or `pass`, because GPG key generation involves interactive choices and the right behavior depends on the user's preferences (key type, passphrase policy, comment).

### 5.1 Initialize maestro's GPG keyring

```bash
sudo -u maestro -i

# inside maestro's shell:
gpg --batch --pinentry-mode loopback --gen-key <<'KEY_PARAMS'
%no-protection
Key-Type: ed25519
Key-Usage: sign
Subkey-Type: cv25519
Subkey-Usage: encrypt
Name-Real: Orch Service
Name-Email: maestro@localhost
Expire-Date: 0
%commit
KEY_PARAMS
```

`%no-protection` produces a passphrase-less key. This is the convenience-first choice: the orchestrator runs unattended overnight; pinentry prompts on every secret access defeat the orchestration model. The key file is at `~maestro/.gnupg/private-keys-v1.d/`, mode `700`, only `maestro` can read it. If `maestro`'s home is compromised, the key is compromised; that's the same risk profile as any application that holds a long-lived secret.

If you want passphrase protection, omit `%no-protection`, set up `gpg-agent` with appropriate cache TTLs, and ensure `pinentry-curses` is installed (WSL has no GUI pinentry by default).

### 5.2 Initialize pass

```bash
# still in maestro's shell:
pass init maestro@localhost
# This creates ~maestro/.password-store/ with a .gpg-id pointing at the key.
exit
```

### 5.3 Add the orchestrator's secrets

```bash
# As caleb, populate maestro's pass via sudo:
sudo -u maestro -i pass insert jam/conductor/openai-api-key
sudo -u maestro -i pass insert jam/conductor/anthropic-api-key
sudo -u maestro -i pass insert jam/workers/deepseek-api-key
sudo -u maestro -i pass insert jam/workers/github-app-id
sudo -u maestro -i pass insert -m jam/workers/github-app-key   # multiline for PEM key
sudo -u maestro -i pass insert jam/search/brave
sudo -u maestro -i pass insert jam/search/firecrawl
# ... etc, per the v5 §11.3.1 key list
```

Each `pass insert` prompts for the value (or paste, for `-m` multiline). Keys are stored encrypted under `~maestro/.password-store/`, only decryptable with `maestro`'s GPG key.

### 5.4 Caleb's personal pass stays separate

Caleb's existing `~caleb/.password-store/` (if any) is unaffected. The orchestrator's secrets and your personal secrets are encrypted with different GPG keys and live in different stores. A compromise of one doesn't expose the other.

### 5.5 Verify

```bash
sudo -u maestro -i pass list
# Should show the keys you added under jam/.

sudo -u maestro -i pass show jam/conductor/openai-api-key
# Should print the value.
```

---

## 6. Operational patterns

Day-to-day use after the setup is complete.

### 6.1 Starting / stopping the orchestrator

The orchestrator runs as `maestro`. Start it via:

```bash
sudo -u maestro -i jam start    # starts process-compose with all services
sudo -u maestro -i jam stop
sudo -u maestro -i jam status
```

A future systemd unit (out of scope for this addendum) can wrap these as `systemctl --user start jam.service` running under maestro.

### 6.2 CLI commands as caleb

The `jam` CLI runs as caleb but talks to NATS (which is owned by maestro):

```bash
jam task spawn 'Refactor canyon generator'
jam task list
jam trace replay 01HXKJ...
jam quota show
jam doctor
```

NATS is bound to localhost; access control is via NATS auth tokens stored in `pass`. The CLI as `caleb` reads maestro's NATS token via:

```bash
sudo -n -u maestro -i pass show jam/nats/token
```

This works without password thanks to the sudoers rule. Behind the scenes the CLI does this transparently.

If you'd rather not have the CLI shell out to sudo on every command, an alternative is to keep a duplicate of the NATS token in caleb's pass (`pass insert jam/nats/token` as caleb). Less clean (duplicate secret) but avoids the sudo round-trip.

### 6.3 Inspecting orchestrator state

Journal:

```bash
sudo -u maestro -i tail -f /home/maestro/.jam/journal/$(date -u +%Y-%m-%d)/journal.worker.jsonl
sudo -u maestro -i ls /home/maestro/.jam/journal/
```

Or via the UI, which renders these for you.

### 6.4 Editing skills

Skills live in the shared dir at `~caleb/code/jam-skills/`, owned `caleb:maestro` mode `2770`. Caleb edits them as caleb in his normal editor:

```bash
cd ~/code/jam-skills
nvim projects/blueberry/hot-paths.md
git add . && git commit -m "Update hot-path guidance"
```

The orchestrator's inotify watcher (running as maestro) fires; the conductor invalidates the skill cache; next session reads the updated content. No restart, no reload command.

### 6.5 Editing Tempyr nodes

Same pattern, in `~caleb/code/blueberry-tempyr-live/tempyr/nodes/`. Setgid ensures any new file caleb creates is group-readable by maestro.

`tempyr/tasks/` files are written by the orchestrator (maestro); caleb can read them but should not write them. They're journal-derived and would be overwritten anyway.

### 6.6 Inspecting a stuck worker

```bash
sudo -u picker -i ls /home/picker/workers/
sudo -u picker -i cd /home/picker/workers/<task-id>/ && git log --oneline -20
sudo -u picker -i cat /home/picker/workers/<task-id>/.killed-at-*
```

For destructive cleanup of orphaned worktrees, the `jam task cleanup` CLI handles it as maestro, which sudos to picker as needed.

### 6.7 Emergency stop

If the orchestrator is doing something bad and you want everything to halt:

```bash
sudo -u maestro jam pause-dispatch --reason "manual intervention"
sudo -u picker pkill -TERM -u picker      # kill all worker processes
sudo systemctl stop jam                            # if running under systemd
```

The pause-dispatch flag persists in NATS KV; new wakes will refuse to spawn. Restart with `jam resume-dispatch` after you've fixed whatever was wrong.

---

## 7. Code changes implied for the v5 implementation

This addendum changes assumptions in several v5 sections. The implementing AI agent should update accordingly:

### 7.1 Path defaults (§11.1)

The v5 directory layout assumes `~/.jam/` resolves to caleb's home. With the multi-user model, orchestrator state lives under `maestro`'s home:

| v5 path (assumes single-user) | Multi-user equivalent |
|---|---|
| `~/.jam/config/` | `/home/maestro/.jam/config/` |
| `~/.jam/journal/` | `/home/maestro/.jam/journal/` |
| `~/.jam/session-store.db` | `/home/maestro/.jam/session-store.db` |
| `~/.jam/research/` | `/home/maestro/.jam/research/` |
| `~/.jam/skills/` | `/home/caleb/code/jam-skills/` (shared) |
| `~/.jam/worktrees/` | `/home/picker/workers/` |

Implementation: define `JAM_HOME` env var that defaults to `/home/maestro/.jam` when running as maestro; resolves to `~/.jam` when running as caleb (for CLI). Tools and services use `JAM_HOME` everywhere instead of hardcoded `~/`.

For the skills repo path specifically, add a config:

```toml
# /home/maestro/.jam/config/skills.toml
skills-repo = "/home/caleb/code/jam-skills"
```

The conductor reads this; the inotify watcher uses this path; CLI tools that read skills use this path.

### 7.2 Worker spawn (§4.5.1, §24.3)

The harness adapter must spawn the worker as `picker` rather than as the orchestrator's own user. Replace the v5 spawn pattern:

```rust
// v5 §24.3 (single-user)
let cmd = Command::new(harness_binary)
    .current_dir(&worktree_path)
    .env_clear()
    .envs(allowlist)
    ...;
```

with the multi-user pattern:

```rust
// multi-user
let env_keys: Vec<String> = allowlist.iter().map(|(k, _)| k.clone()).collect();
let preserve_env = env_keys.join(",");

let cmd = Command::new("sudo")
    .args([
        "-n",                                       // non-interactive (NOPASSWD)
        "-u", "picker",
        "--preserve-env={preserve_env}",            // pass through specified env
        "--",
        harness_binary,
    ])
    .current_dir(&worktree_path)
    .env_clear()
    .envs(allowlist)
    ...;
```

Worktree path also changes: `~/.jam/worktrees/<task-id>/` becomes `/home/picker/workers/<task-id>/`. The worktree creation protocol (§6.9 in v5) still applies; only the root path changes.

### 7.3 Secrets backend (§11.3)

The `pass` backend in v5 implicitly uses caleb's pass store. With the multi-user model, the orchestrator's pass store belongs to `maestro`. Two implementation options:

**Option A — orchestrator services run as maestro.** When the conductor / tool services run as `maestro`, GPG and `pass` find maestro's keyring naturally via `$HOME`. No code changes needed beyond running services as the right user. **Preferred.**

**Option B — bridge via sudo when needed.** If a service runs as a different user but needs maestro's pass, use `sudo -n -u maestro -i pass show <key>`. Slower and grants more capability than necessary. **Avoid unless option A is impossible.**

The CLI runs as caleb but needs to read NATS tokens etc. — see §6.2 for the sudo-via-pass pattern.

### 7.4 process-compose configuration

`process-compose.yaml` should declare each service's user explicitly:

```yaml
processes:
  nats:
    command: /usr/local/bin/nats-server -c /home/maestro/.jam/config/nats.toml
    user: maestro
  conductor:
    command: /opt/jam/bin/jam-conductor
    user: maestro
    environment:
      - JAM_HOME=/home/maestro/.jam
  jam-svc-observe:
    command: /opt/jam/bin/jam-svc-observe
    user: maestro
  jam-svc-session:
    command: /opt/jam/bin/jam-svc-session
    user: maestro
  # ... etc
  ui-server:
    command: /opt/jam/bin/jam-ui-server
    user: maestro
    # binds 127.0.0.1 + Tailscale CGNAT range per §4.11.1
```

The `user:` directive requires process-compose to launch as root (or via `sudo`); each subprocess then runs as the declared user. For WSL: launch process-compose as root via `sudo`, or have a simple shell wrapper that calls `sudo -u maestro /opt/jam/bin/jam start`.

### 7.5 Setup script (§11.4) — additional checks

`jam setup` and `jam doctor` should verify the multi-user layout:

- Users `maestro` and `picker` exist with expected UIDs.
- Calling user is in `maestro` group (via `id -nG`).
- `/etc/sudoers.d/jam-users` exists and validates with `visudo -c`.
- Test transition: `sudo -n -u maestro id` succeeds without password.
- `/etc/jam/bootstrap.log` exists and matches expected format.
- JAM_HOME (resolved from current user) is on native FS.
- Skill repo path (from config) exists, is readable by the calling identity, and has correct group permissions.
- Canonical Tempyr worktree path (per project) has correct group ownership and setgid.

If any check fails, surface a specific error pointing at this addendum:

```
✗ User 'maestro' missing.

  This deployment uses the multi-user security model.
  Run: sudo ./bootstrap-users.sh

  See: docs/security-setup.md
```

### 7.6 UI server attribution (§4.11.1)

The UI server runs as `maestro`. UI session tokens still attribute actions to a per-user-id (each token is generated for a specific human, even if it's just "caleb" today); journal records `from: human:caleb` rather than `from: maestro` for UI-initiated actions.

### 7.7 inotify watchers (§21.4)

Watchers run as `maestro`. For watchers on shared dirs (`~caleb/code/jam-skills/`, `~caleb/code/blueberry-tempyr-live/`), maestro must have read access via group membership. The directory permissions (mode 2770, group maestro) make this work; just verify in setup that maestro can in fact read these paths.

---

## 8. Migrating from a single-user setup

If you've been running the orchestrator as caleb directly and now want to add the multi-user model:

### 8.1 Stop the orchestrator

```bash
jam stop
# Or, if no graceful stop yet implemented:
pkill -u caleb -f 'jam-'
```

### 8.2 Run the bootstrap script

```bash
sudo ./bootstrap-users.sh --user caleb
```

### 8.3 Move existing state to maestro

```bash
# Move orchestrator state (CHECK PATHS FIRST):
sudo cp -a ~/.jam /home/maestro/.jam
sudo chown -R maestro:maestro /home/maestro/.jam
sudo chmod 750 /home/maestro/.jam

# Move worktrees to picker:
sudo mkdir -p /home/picker/workers
sudo cp -a ~/.jam/worktrees/* /home/picker/workers/
sudo chown -R picker:picker /home/picker/workers/
sudo chmod 750 /home/picker/workers
sudo chmod 700 /home/picker/workers/*

# Move skills to shared location:
mv ~/.jam/skills ~/code/jam-skills
sudo chown -R caleb:maestro ~/code/jam-skills
sudo chmod -R 2770 ~/code/jam-skills

# Move canonical Tempyr worktree group ownership:
sudo chown -R caleb:maestro ~/code/blueberry-tempyr-live
sudo chmod -R 2770 ~/code/blueberry-tempyr-live
# Re-do setgid on dirs (chmod -R won't set the bit reliably):
sudo find ~/code/blueberry-tempyr-live -type d -exec chmod 2770 {} \;
```

### 8.4 Re-encrypt secrets to maestro's GPG key

Caleb's existing `pass` store is encrypted to caleb's GPG key. maestro cannot read it. Re-encrypt:

```bash
# As caleb, list orchestrator-related secrets:
pass list jam/

# For each, copy the value and re-insert into maestro's pass:
pass show jam/conductor/openai-api-key | sudo -u maestro -i pass insert -e jam/conductor/openai-api-key
# ... repeat for each key

# Then delete from caleb's pass to avoid confusion:
pass rm jam/conductor/openai-api-key
# ...
```

The `-e` flag to `pass insert` reads the value from stdin (avoiding a prompt).

### 8.5 Update config paths

Edit `~maestro/.jam/config/conductor.toml`, `secrets.toml`, etc., to reflect new paths if any are absolute references to caleb's home. Most should use `$JAM_HOME` or relative paths and not need changes.

### 8.6 Restart and verify

```bash
sudo -u maestro -i jam start
jam doctor
```

Expect every check to pass. Spot-check a worker spawn to confirm the new layout works end-to-end.

---

## 9. Recovery and gotchas

### 9.1 "Group change not taking effect"

After `usermod -aG maestro caleb`, the change applies to **new login sessions only**. Existing shells still have the old group set. Three fixes:

1. Log out and back in (most reliable).
2. `newgrp maestro` — replaces the current shell with one that has the new group.
3. `sudo -u caleb -i` — opens a new login session as caleb with refreshed groups.

### 9.2 "Sudo asks for password despite NOPASSWD"

Causes:

- The sudoers file has the wrong username. Verify: `sudo -l -U caleb` should show NOPASSWD entries for `maestro` and `picker`.
- Another sudoers file overrides yours. Check `/etc/sudoers` and other `/etc/sudoers.d/*` files for conflicting rules.
- `Defaults targetpw` is set somewhere, which inverts password behavior.

Fix: ensure `/etc/sudoers.d/jam-users` is mode 440 owned root:root, runs through `visudo -c`, and doesn't conflict with other rules.

### 9.3 "maestro cannot read the canonical Tempyr worktree"

Symptoms: the orchestrator can't write to `tempyr/tasks/`; the Tempyr MCP server can't read nodes.

Causes:

- Group ownership wasn't `maestro` (probably `caleb`).
- setgid bit not set (new files inherit caleb's group, not maestro's).
- `/home/caleb` mode is `700`, blocking traversal.

Fix:

```bash
sudo chown -R caleb:maestro ~/code/blueberry-tempyr-live
sudo find ~/code/blueberry-tempyr-live -type d -exec chmod 2770 {} \;
sudo find ~/code/blueberry-tempyr-live -type f -exec chmod 660 {} \;
sudo chmod 751 /home/caleb
```

Then test: `sudo -u maestro ls ~/code/blueberry-tempyr-live/tempyr/tasks/`.

### 9.4 "Secrets work for caleb but fail for maestro"

Caleb's pass and maestro's pass are different stores with different GPG keys. maestro cannot read caleb's secrets, and vice versa. Make sure orchestrator secrets were inserted into maestro's store.

```bash
sudo -u maestro -i pass list   # what maestro can see
pass list                       # what caleb can see
```

### 9.5 "I locked myself out somehow"

If the sudoers file is corrupt and caleb can no longer sudo:

- WSL: `wsl --user root` from PowerShell drops you into a root shell, bypassing sudo entirely.
- Native Linux: boot to single-user mode (kernel param `init=/bin/sh` or recovery mode).

Then fix `/etc/sudoers.d/jam-users` (or remove it) and re-run the bootstrap script.

### 9.6 "GPG agent issues on WSL"

Symptoms: `pass show` hangs or fails with "no pinentry."

Fix: install `pinentry-curses`, configure `~/.gnupg/gpg-agent.conf`:

```
pinentry-program /usr/bin/pinentry-curses
default-cache-ttl 14400
max-cache-ttl 28800
```

If you generated the key with `%no-protection` (passphrase-less, recommended for maestro), pinentry shouldn't be invoked at all. If it is, something is misconfigured — check the key with `gpg --list-secret-keys`.

### 9.7 "WSL drvfs interaction"

WSL mounts your Windows drives at `/mnt/c/`, `/mnt/d/`, etc. These are case-insensitive, have different permission semantics, and are slow for git operations. The bootstrap script refuses to install if orchestrator paths resolve to drvfs (per v5 §2.14). Always keep orchestrator state on the Linux native filesystem (`/home/<user>/`).

If you've been running everything on `/mnt/c/Users/caleb/.jam/`, migrating to `/home/caleb/...` is the prerequisite to running the bootstrap script. See §8 for the migration steps; treat the drvfs-to-native move as part of step §8.3.

### 9.8 "How do I undo the bootstrap?"

Reverse, roughly:

```bash
sudo userdel -r picker
sudo userdel -r maestro
sudo rm /etc/sudoers.d/jam-users
sudo rm /etc/jam/bootstrap.log
sudo gpasswd -d caleb maestro 2>/dev/null || true   # remove from group
sudo chmod 750 /home/caleb                           # restore
```

You'll lose maestro's pass store (encrypted secrets) unless you backed up `~maestro/` first. Worktrees in `/home/picker/workers/` are also gone. Be sure before you run `userdel -r`.

---

## 10. jam doctor additions

`jam doctor` should include these checks (added to the v5 §11.4 list):

```
14. Service users maestro and picker exist
15. Calling user is in maestro group (active in current shell)
16. /etc/sudoers.d/jam-users present and valid
17. sudo -n -u maestro id succeeds (NOPASSWD works)
18. /etc/jam/bootstrap.log present (and matches expected version)
19. JAM_HOME for current process is on native FS
20. Skills repo path exists and is readable by the running user
21. Canonical Tempyr worktree per active project has correct group ownership and setgid
22. maestro's pass store has the expected keys (per project config)
23. Worker spawn smoke test: sudo -u picker can write to a temp file in /home/picker/workers/
24. picker cannot sudo (verify least privilege)
```

Each of these maps to a specific failure mode covered above. Like the original 13 checks, each prints a specific remediation hint pointing at the relevant section of this addendum.

---

## 11. Summary

The multi-user model adds three Linux user accounts and a sudoers config to provide kernel-enforced filesystem isolation between the human user, the orchestrator's substrate, and worker processes. Setup is one bash script (`bootstrap-users.sh`) plus one manual GPG/pass init. Day-to-day operation is unchanged: caleb runs `jam` CLI commands as caleb; the orchestrator runs as maestro; workers run as picker.

The defense gained: workers cannot read SSH keys, AWS credentials, browser sessions, or other user-owned secrets. The orchestrator's substrate cannot read the human user's secrets. Per-worker worktree isolation prevents one worker from interfering with another. GitHub access is bounded by App-scoped installation tokens.

The cost: one bootstrap step, one GPG init step, slightly more mental overhead about which user owns which path. The convenience model (NOPASSWD sudo, no per-task user creation, no Docker) keeps day-to-day friction near zero.

Hardening to per-task isolation, network sandboxing, or systemd-managed services is additive — none of it requires changing what's described here.
