//! §20.3 atomic-swap and §20.4 mechanical-rollback procedures.
//!
//! Lives in the patch-agent (which runs as `maestro`), so all file ops on
//! `~maestro/.jam/{bin,logs,...}` and the spawn of versioned tool services
//! happen under maestro's identity — no `sudo` from the caller needed. Driven
//! by `patch.staged` and `patch.rollback-requested` events on NATS; see
//! `main.rs` for the subscription wiring.

#![allow(missing_docs)]

use chrono::{DateTime, Utc};
use jam_events::generated::{
    Event, PatchApplied, PatchConfirmed, PatchLockAcquired, PatchLockReleased, PatchRolledBack,
};
use jam_events::EventEnvelope;
use jam_nats::JamNats;
use jam_tools_core::paths::jam_home;
use jam_trace::TraceCtx;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command as ProcessCommand, Stdio};
use std::time::Duration;

const PATCH_LOCK_BUCKET: &str = "patch-lock";
const PATCH_LOCK_KEY: &str = "current";
const DEFAULT_PATCH_HEALTH_TIMEOUT_SECS: u64 = 30;
const DEFAULT_PATCH_DRAIN_TIMEOUT_SECS: u64 = 5;

/// Inputs for an atomic-swap apply, gathered from a `patch.staged` event.
#[derive(Debug, Clone)]
pub struct ApplyRequest {
    /// Tool service name, e.g. `observe`.
    pub service: String,
    /// Target version, e.g. `0.4.7`.
    pub version: String,
    /// Absolute path to the binary to install. Must be readable by the agent
    /// (i.e. the `maestro` user) and have its executable bit set.
    pub staging_path: PathBuf,
    /// SHA-256 of the binary, hex-lowercase. Verified before launch.
    pub expected_sha256: String,
    /// Origin actor recorded in lock + manifest events.
    pub requested_by: String,
    /// Trace context propagated from the originating `patch.staged` envelope.
    pub trace_ctx: TraceCtx,
    /// NATS URL handed to the new service via env (`NATS_URL`).
    pub nats_url: String,
    /// NATS auth token, if any. Forwarded as `NATS_TOKEN`.
    pub nats_token: Option<String>,
}

/// Inputs for a stop-replace-restart deploy of a singleton process (HTTP
/// server, file-locking reconciler, etc.) that cannot run two versions
/// side-by-side.
#[derive(Debug, Clone)]
pub struct StopReplaceRequest {
    /// Short service name from the registry (e.g. `ui-server`).
    pub service: String,
    /// Target version (informational; used in the runtime path).
    pub version: String,
    /// Absolute path to the staged binary.
    pub staging_path: PathBuf,
    /// SHA-256 of the staged binary, hex-lowercase.
    pub expected_sha256: String,
    /// Origin actor recorded in patch.confirmed.
    pub requested_by: String,
    /// Trace context propagated from `patch.staged`.
    pub trace_ctx: TraceCtx,
    /// process-compose process name to stop/start (from the registry strategy).
    pub process_name: String,
}

/// Inputs for a Python virtualenv deploy (maestro). `source_path` is a
/// directory containing `pyproject.toml`, `uv.lock`, and `src/`.
#[derive(Debug, Clone)]
pub struct PythonAppRequest {
    pub service: String,
    pub version: String,
    pub source_path: PathBuf,
    pub requested_by: String,
    pub trace_ctx: TraceCtx,
    pub install_dir: String,
    pub process_name: String,
}

/// Inputs for installing the `jam` CLI (or any other root-owned canonical
/// binary) at a fixed absolute path.
#[derive(Debug, Clone)]
pub struct CanonicalBinaryRequest {
    pub service: String,
    pub version: String,
    pub staging_path: PathBuf,
    pub expected_sha256: String,
    pub requested_by: String,
    pub trace_ctx: TraceCtx,
    pub dest_path: PathBuf,
}

/// Inputs for a §20.4 mechanical rollback, gathered from a
/// `patch.rollback-requested` event or from the post-apply verifier.
#[derive(Debug, Clone)]
pub struct RollbackRequest {
    /// Tool service name to roll back.
    pub service: String,
    /// Reason recorded in `patch.rolled-back`.
    pub reason: String,
    /// Origin actor.
    pub requested_by: String,
    /// Trace context propagated from the rollback request.
    pub trace_ctx: TraceCtx,
}

/// Routing-manifest update emitted whenever apply or rollback advances the manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingManifestUpdatedEvent {
    pub manifest_id: String,
    pub service: String,
    pub from_version: Option<String>,
    pub to_version: String,
    pub subject_prefix: String,
    pub revision: u64,
    pub ts: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PatchLockRecord {
    service: String,
    action: String,
    actor: String,
    trace_id: String,
    acquired_at: DateTime<Utc>,
}

struct InstalledPatchBinary {
    runtime_path: PathBuf,
    binary_sha256: String,
}

struct PatchServiceProcess {
    child: Child,
    stdout_log: PathBuf,
    stderr_log: PathBuf,
}

impl PatchServiceProcess {
    fn stop(&mut self) -> String {
        match self.child.try_wait() {
            Ok(Some(status)) => return format!("already exited with {status}"),
            Ok(None) => {}
            Err(err) => return format!("could not inspect candidate process: {err}"),
        }
        let kill = self.child.kill();
        let wait = self.child.wait();
        match (kill, wait) {
            (Ok(()), Ok(status)) => format!("killed candidate process; wait status {status}"),
            (Ok(()), Err(err)) => format!("killed candidate process; wait failed: {err}"),
            (Err(kill_err), Ok(status)) => {
                format!("kill returned {kill_err}; wait status {status}")
            }
            (Err(kill_err), Err(wait_err)) => {
                format!("kill failed: {kill_err}; wait failed: {wait_err}")
            }
        }
    }

    fn detach(self) {
        drop(self);
    }
}

/// Acquire the patch-lock, run the §20.3 procedure, release the lock.
pub async fn apply_staged_patch(
    nats: &JamNats,
    request: ApplyRequest,
) -> Result<RoutingManifestUpdatedEvent, String> {
    let lock_revision = acquire_patch_lock(
        nats,
        &request.requested_by,
        &request.trace_ctx,
        &request.service,
        &format!("apply:{}", request.version),
    )
    .await?;
    let result = apply_staged_patch_locked(nats, &request).await;
    let release = release_patch_lock(
        nats,
        lock_revision,
        &request.requested_by,
        &request.trace_ctx,
    )
    .await;
    finish_locked_patch_result(result, release)
}

/// Acquire the patch-lock, run the §20.4 procedure, release the lock.
pub async fn perform_rollback(
    nats: &JamNats,
    request: RollbackRequest,
) -> Result<RoutingManifestUpdatedEvent, String> {
    let lock_revision = acquire_patch_lock(
        nats,
        &request.requested_by,
        &request.trace_ctx,
        &request.service,
        "rollback",
    )
    .await?;
    let result = rollback_locked(nats, &request).await;
    let release = release_patch_lock(
        nats,
        lock_revision,
        &request.requested_by,
        &request.trace_ctx,
    )
    .await;
    finish_locked_patch_result(result, release)
}

/// Deploy a singleton process via stop-replace-restart. Replaces
/// `~maestro/.jam/bin/<binary_name>` (no `-<version>` suffix) atomically and
/// cycles the named process-compose service. Emits `patch.confirmed` on
/// success.
///
/// Unlike `apply_staged_patch` this strategy does not touch the routing
/// manifest because singletons have no versioned subject namespace — the
/// process-compose `command:` points at a fixed path.
pub async fn stop_replace_restart(
    nats: &JamNats,
    request: StopReplaceRequest,
) -> Result<(), String> {
    let staging = &request.staging_path;
    if !staging.is_file() {
        return Err(format!("staged binary is missing: {}", staging.display()));
    }
    validate_executable(staging)?;
    let staged_sha = sha256_file_hex(staging)?;
    if staged_sha != request.expected_sha256 {
        return Err(format!(
            "staged binary sha256 mismatch: declared={} actual={}",
            request.expected_sha256, staged_sha
        ));
    }
    let binary_basename = jam_tools_core::deploy_targets::binary_name(&request.service)
        .map(str::to_owned)
        .ok_or_else(|| {
            format!(
                "unknown deploy target `{}` in stop_replace_restart",
                request.service
            )
        })?;
    let runtime_path = jam_home().join("bin").join(&binary_basename);
    install_via_atomic_rename(staging, &runtime_path)?;

    process_compose_call(&["process", "stop", &request.process_name])?;
    // process-compose stop is synchronous on the API; the child has exited by
    // the time we get here. Start the same process — it'll pick up the new
    // binary at the same path.
    process_compose_call(&["process", "start", &request.process_name])?;
    wait_for_process_running(&request.process_name, Duration::from_secs(30))?;

    let now = Utc::now();
    let confirmed = PatchConfirmed {
        service: request.service.clone(),
        version: request.version.clone(),
        // checks_run=1: we verified the process returned to Running.
        // Routing-manifest health-check pings don't apply here because the
        // process is a singleton without versioned subjects.
        checks_run: 1,
        ts: now,
    };
    let envelope = EventEnvelope::new(
        PatchConfirmed::EVENT_TYPE,
        PatchConfirmed::EVENT_SUBTYPE_VERSION,
        0,
        request.trace_ctx.trace_id.to_string(),
        request.requested_by.clone(),
        confirmed,
    );
    nats.publish_traced(PatchConfirmed::EVENT_TYPE, &envelope, &request.trace_ctx)
        .await
        .map_err(|err| format!("publish patch.confirmed: {err}"))?;
    Ok(())
}

/// Deploy a Python virtualenv app via rsync + uv pip install + restart.
/// `source_path` must be a directory containing `pyproject.toml`, `uv.lock`,
/// and `src/`. Emits `patch.confirmed` once the restarted process is back
/// to `Running`.
pub async fn deploy_python_app(nats: &JamNats, request: PythonAppRequest) -> Result<(), String> {
    if !request.source_path.is_dir() {
        return Err(format!(
            "source path is not a directory: {}",
            request.source_path.display()
        ));
    }
    for required in ["pyproject.toml", "uv.lock", "src"] {
        let child = request.source_path.join(required);
        if !child.exists() {
            return Err(format!("source missing {required}: {}", child.display()));
        }
    }

    // rsync src/ separately so we preserve the venv at install_dir/.venv.
    let src_arg = format!("{}/", request.source_path.join("src").display());
    let dest_src_arg = format!("{}/src/", request.install_dir);
    run_external_cmd("rsync", &["-a", "--delete", &src_arg, &dest_src_arg])?;
    for file in ["pyproject.toml", "uv.lock"] {
        let from = request.source_path.join(file);
        let to = format!("{}/{file}", request.install_dir);
        run_external_cmd("cp", &["-f", &from.display().to_string(), &to])?;
    }

    let venv_python = format!("{}/.venv/bin/python", request.install_dir);
    let uv = find_uv()?;
    let uv_str = uv
        .to_str()
        .ok_or_else(|| format!("uv path is not valid UTF-8: {}", uv.display()))?;
    run_external_cmd(
        uv_str,
        &[
            "pip",
            "install",
            "--quiet",
            "--python",
            &venv_python,
            &request.install_dir,
        ],
    )?;
    run_external_cmd(
        &venv_python,
        &["-m", &request.service.replace('-', "_"), "--help"],
    )
    .or_else(|_| run_external_cmd(&venv_python, &["-m", "jam_maestro", "--help"]))?;
    process_compose_call(&["process", "restart", &request.process_name])?;
    wait_for_process_running(&request.process_name, Duration::from_secs(30))?;

    publish_confirmed(
        nats,
        &request.service,
        &request.version,
        &request.requested_by,
        &request.trace_ctx,
        1,
    )
    .await
}

/// Install a binary directly into a canonical root-owned path (e.g.
/// `/opt/jam/bin/jam` for the CLI). Atomic rename + emit `patch.confirmed`.
pub async fn install_canonical_binary(
    nats: &JamNats,
    request: CanonicalBinaryRequest,
) -> Result<(), String> {
    if !request.staging_path.is_file() {
        return Err(format!(
            "staged binary is missing: {}",
            request.staging_path.display()
        ));
    }
    validate_executable(&request.staging_path)?;
    let staged_sha = sha256_file_hex(&request.staging_path)?;
    if !request.expected_sha256.is_empty() && staged_sha != request.expected_sha256 {
        return Err(format!(
            "staged binary sha256 mismatch: declared={} actual={}",
            request.expected_sha256, staged_sha
        ));
    }
    install_via_atomic_rename(&request.staging_path, &request.dest_path)?;
    publish_confirmed(
        nats,
        &request.service,
        &request.version,
        &request.requested_by,
        &request.trace_ctx,
        1,
    )
    .await
}

async fn publish_confirmed(
    nats: &JamNats,
    service: &str,
    version: &str,
    requested_by: &str,
    trace_ctx: &TraceCtx,
    checks_run: u32,
) -> Result<(), String> {
    let confirmed = PatchConfirmed {
        service: service.to_owned(),
        version: version.to_owned(),
        checks_run,
        ts: Utc::now(),
    };
    let envelope = EventEnvelope::new(
        PatchConfirmed::EVENT_TYPE,
        PatchConfirmed::EVENT_SUBTYPE_VERSION,
        0,
        trace_ctx.trace_id.to_string(),
        requested_by.to_owned(),
        confirmed,
    );
    nats.publish_traced(PatchConfirmed::EVENT_TYPE, &envelope, trace_ctx)
        .await
        .map_err(|err| format!("publish patch.confirmed: {err}"))
}

fn run_external_cmd(bin: &str, args: &[&str]) -> Result<(), String> {
    let status = std::process::Command::new(bin)
        .args(args)
        .status()
        .map_err(|err| format!("run {bin} {}: {err}", args.join(" ")))?;
    if !status.success() {
        return Err(format!(
            "{bin} {} failed (exit {})",
            args.join(" "),
            status
                .code()
                .map_or_else(|| "signal".to_owned(), |c| c.to_string())
        ));
    }
    Ok(())
}

fn find_uv() -> Result<PathBuf, String> {
    // Patch-agent runs as maestro (per process-compose `user:` directive) but
    // currently winds up as root depending on launcher. Either way, try the
    // caleb-local install first (the convention from install-substrate.sh),
    // then PATH.
    let caleb_uv = PathBuf::from("/home/caleb/.local/bin/uv");
    if caleb_uv.is_file() {
        return Ok(caleb_uv);
    }
    if let Ok(output) = std::process::Command::new("which").arg("uv").output() {
        if output.status.success() {
            let raw = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if !raw.is_empty() {
                let path = PathBuf::from(raw);
                if path.is_file() {
                    return Ok(path);
                }
            }
        }
    }
    Err("uv not found in /home/caleb/.local/bin/uv or on PATH".into())
}

/// Atomically install `staging_path` over `runtime_path` using a tempfile +
/// rename in the same directory. Works even if `runtime_path` is currently a
/// running binary (rename(2) doesn't touch the inode, so ETXTBSY doesn't
/// apply).
fn install_via_atomic_rename(staging_path: &Path, runtime_path: &Path) -> Result<(), String> {
    let parent = runtime_path
        .parent()
        .ok_or_else(|| format!("runtime path has no parent: {}", runtime_path.display()))?;
    fs::create_dir_all(parent).map_err(|err| format!("create {}: {err}", parent.display()))?;
    let file_name = runtime_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            format!(
                "runtime path has no valid file name: {}",
                runtime_path.display()
            )
        })?;
    let mut tmp = tempfile::Builder::new()
        .prefix(&format!(".{file_name}.staging."))
        .suffix(".tmp")
        .tempfile_in(parent)
        .map_err(|err| format!("create tempfile in {}: {err}", parent.display()))?;
    {
        use std::io::Write;
        let bytes = fs::read(staging_path)
            .map_err(|err| format!("read {}: {err}", staging_path.display()))?;
        tmp.as_file_mut()
            .write_all(&bytes)
            .map_err(|err| format!("write {}: {err}", tmp.path().display()))?;
        tmp.as_file_mut()
            .sync_all()
            .map_err(|err| format!("fsync {}: {err}", tmp.path().display()))?;
    }
    #[cfg(unix)]
    {
        let mode = fs::metadata(staging_path)
            .map_err(|err| format!("stat {}: {err}", staging_path.display()))?
            .permissions()
            .mode();
        fs::set_permissions(tmp.path(), fs::Permissions::from_mode(mode))
            .map_err(|err| format!("chmod {}: {err}", tmp.path().display()))?;
    }
    let tmp_path_for_err = tmp.path().to_path_buf();
    tmp.persist(runtime_path).map_err(|err| {
        format!(
            "atomic rename {} -> {}: {}",
            tmp_path_for_err.display(),
            runtime_path.display(),
            err.error
        )
    })?;
    Ok(())
}

fn process_compose_call(args: &[&str]) -> Result<(), String> {
    let status = std::process::Command::new("/opt/jam/bin/process-compose")
        .args(args)
        .status()
        .map_err(|err| format!("run process-compose {}: {err}", args.join(" ")))?;
    if !status.success() {
        return Err(format!(
            "process-compose {} failed (exit {})",
            args.join(" "),
            status
                .code()
                .map_or_else(|| "signal".to_owned(), |c| c.to_string())
        ));
    }
    Ok(())
}

fn wait_for_process_running(name: &str, timeout: Duration) -> Result<(), String> {
    let deadline = std::time::Instant::now() + timeout;
    let mut last_status = String::from("unknown");
    loop {
        let output = std::process::Command::new("/opt/jam/bin/process-compose")
            .args(["process", "list", "-o", "json"])
            .output()
            .map_err(|err| format!("run process-compose process list: {err}"))?;
        if output.status.success() {
            if let Ok(parsed) = serde_json::from_slice::<serde_json::Value>(&output.stdout) {
                if let Some(arr) = parsed.as_array() {
                    for entry in arr {
                        if entry.get("name").and_then(serde_json::Value::as_str) == Some(name) {
                            if let Some(status) =
                                entry.get("status").and_then(serde_json::Value::as_str)
                            {
                                last_status = status.to_owned();
                                if status == "Running" {
                                    return Ok(());
                                }
                            }
                        }
                    }
                }
            }
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!(
                "process `{name}` did not return to Running within {}s; last status={last_status}",
                timeout.as_secs()
            ));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

async fn apply_staged_patch_locked(
    nats: &JamNats,
    request: &ApplyRequest,
) -> Result<RoutingManifestUpdatedEvent, String> {
    let installed =
        install_staged_binary(&request.staging_path, &request.service, &request.version)?;
    if installed.binary_sha256 != request.expected_sha256 {
        return Err(format!(
            "staged binary sha256 mismatch: declared={} actual={}",
            request.expected_sha256, installed.binary_sha256
        ));
    }

    let loaded = jam_nats::load_current_routing_manifest(nats.jetstream())
        .await
        .map_err(|err| format!("load routing manifest: {err}"))?;
    let now = Utc::now();
    let expected_revision = loaded.as_ref().map(|entry| entry.revision);
    let previous_manifest_id = loaded
        .as_ref()
        .map(jam_nats::RoutingManifestEntry::manifest_id);
    let mut manifest = loaded.as_ref().map_or_else(
        || {
            jam_nats::RoutingManifest::empty(
                request.requested_by.clone(),
                request.trace_ctx.trace_id.to_string(),
                now,
            )
        },
        |entry| entry.manifest.clone(),
    );
    let previous_route = manifest.services.get(&request.service).cloned();
    let from_version = previous_route
        .as_ref()
        .map(|route| route.current_version.clone());
    let subject_prefix = jam_nats::default_subject_prefix(&request.service, &request.version);
    if previous_route
        .as_ref()
        .is_some_and(|route| route.subject_prefix == subject_prefix)
    {
        // Idempotent re-deploy: the same version is already wired up. Emit
        // PatchConfirmed (with checks_run=0 as the no-op signal) so any CLI or
        // UI subscriber gets a terminal event instead of timing out, then
        // return Err so the patch-agent logs the no-op for operators.
        let already_confirmed = PatchConfirmed {
            service: request.service.clone(),
            version: request.version.clone(),
            checks_run: 0,
            ts: now,
        };
        let envelope = EventEnvelope::new(
            PatchConfirmed::EVENT_TYPE,
            PatchConfirmed::EVENT_SUBTYPE_VERSION,
            0,
            request.trace_ctx.trace_id.to_string(),
            request.requested_by.clone(),
            already_confirmed,
        );
        if let Err(err) = nats
            .publish_traced(PatchConfirmed::EVENT_TYPE, &envelope, &request.trace_ctx)
            .await
        {
            return Err(format!(
                "publish patch.confirmed for already-current {}: {err}",
                request.version
            ));
        }
        return Err(format!(
            "{} is already current for {} (no-op; patch.confirmed emitted)",
            request.version, request.service
        ));
    }

    let mut candidate = start_versioned_patch_service(
        &jam_home(),
        &installed,
        &request.service,
        &request.version,
        &subject_prefix,
        &request.nats_url,
        request.nats_token.as_deref(),
    )?;
    if let Err(err) =
        verify_patch_service_health(nats, &request.service, &subject_prefix, &request.trace_ctx)
            .await
    {
        let stop = candidate.stop();
        return Err(format!(
            "{err}\nStopped candidate service after failed health gate: {stop}\nstdout: {}\nstderr: {}",
            candidate.stdout_log.display(),
            candidate.stderr_log.display()
        ));
    }

    manifest.schema_version = jam_nats::ROUTING_MANIFEST_SCHEMA_VERSION;
    manifest.updated_at = now;
    manifest.updated_by = request.requested_by.clone();
    manifest.trace_id = request.trace_ctx.trace_id.to_string();
    manifest.previous_manifest_id = previous_manifest_id;
    manifest.services.insert(
        request.service.clone(),
        jam_nats::RoutingService {
            current_version: request.version.clone(),
            subject_prefix: subject_prefix.clone(),
            binary_path: installed.runtime_path,
            binary_sha256: installed.binary_sha256,
            started_at: now,
            expected_health: "ok".into(),
        },
    );
    let revision = match jam_nats::write_current_routing_manifest(
        nats.jetstream(),
        &manifest,
        expected_revision,
    )
    .await
    {
        Ok(revision) => revision,
        Err(err) => {
            let stop = candidate.stop();
            return Err(format!(
                "write routing manifest: {err}\nStopped candidate service after failed manifest swap: {stop}\nstdout: {}\nstderr: {}",
                candidate.stdout_log.display(),
                candidate.stderr_log.display()
            ));
        }
    };
    candidate.detach();
    let manifest_id = jam_nats::manifest_id_for_revision(revision);
    let updated = RoutingManifestUpdatedEvent {
        manifest_id,
        service: request.service.clone(),
        from_version: from_version.clone(),
        to_version: request.version.clone(),
        subject_prefix: subject_prefix.clone(),
        revision,
        ts: now,
    };
    publish_apply_events(nats, request, &updated, from_version, now).await?;
    drain_previous_patch_service(
        nats,
        &request.trace_ctx,
        previous_route.as_ref(),
        &subject_prefix,
    )
    .await?;
    Ok(updated)
}

async fn rollback_locked(
    nats: &JamNats,
    request: &RollbackRequest,
) -> Result<RoutingManifestUpdatedEvent, String> {
    let current = jam_nats::load_current_routing_manifest(nats.jetstream())
        .await
        .map_err(|err| format!("load current routing manifest: {err}"))?
        .ok_or_else(|| "routing manifest is missing; nothing to roll back".to_owned())?;
    let previous_manifest_id = current
        .manifest
        .previous_manifest_id
        .as_deref()
        .ok_or_else(|| "current routing manifest has no previous_manifest_id".to_owned())?;
    let previous_revision = jam_nats::revision_from_manifest_id(previous_manifest_id)
        .ok_or_else(|| format!("unsupported previous_manifest_id: {previous_manifest_id}"))?;
    let previous = jam_nats::load_routing_manifest_revision(nats.jetstream(), previous_revision)
        .await
        .map_err(|err| format!("load previous routing manifest {previous_manifest_id}: {err}"))?
        .ok_or_else(|| format!("previous routing manifest not found: {previous_manifest_id}"))?;

    let from_route = current
        .manifest
        .services
        .get(&request.service)
        .ok_or_else(|| {
            format!(
                "current manifest has no service entry for {}",
                request.service
            )
        })?;
    let to_route = previous
        .manifest
        .services
        .get(&request.service)
        .ok_or_else(|| {
            format!(
                "previous manifest has no service entry for {}",
                request.service
            )
        })?;

    let from_version = from_route.current_version.clone();
    let to_version = to_route.current_version.clone();
    let subject_prefix = to_route.subject_prefix.clone();
    let revision = jam_nats::write_current_routing_manifest(
        nats.jetstream(),
        &previous.manifest,
        Some(current.revision),
    )
    .await
    .map_err(|err| format!("write rollback manifest: {err}"))?;
    let manifest_id = jam_nats::manifest_id_for_revision(revision);
    let now = Utc::now();
    let updated = RoutingManifestUpdatedEvent {
        manifest_id,
        service: request.service.clone(),
        from_version: Some(from_version.clone()),
        to_version: to_version.clone(),
        subject_prefix: subject_prefix.clone(),
        revision,
        ts: now,
    };
    nats.publish_traced(
        jam_nats::ROUTING_MANIFEST_UPDATED_SUBJECT,
        &updated,
        &request.trace_ctx,
    )
    .await
    .map_err(|err| format!("publish routing-manifest.updated: {err}"))?;

    let patch = PatchRolledBack {
        service: request.service.clone(),
        from_version,
        to_version,
        reason: request.reason.clone(),
        ts: now,
    };
    let envelope = EventEnvelope::new(
        PatchRolledBack::EVENT_TYPE,
        PatchRolledBack::EVENT_SUBTYPE_VERSION,
        0,
        request.trace_ctx.trace_id.to_string(),
        request.requested_by.clone(),
        patch,
    );
    nats.publish_traced(PatchRolledBack::EVENT_TYPE, &envelope, &request.trace_ctx)
        .await
        .map_err(|err| format!("publish patch.rolled-back: {err}"))?;
    Ok(updated)
}

async fn publish_apply_events(
    nats: &JamNats,
    request: &ApplyRequest,
    updated: &RoutingManifestUpdatedEvent,
    from_version: Option<String>,
    now: DateTime<Utc>,
) -> Result<(), String> {
    nats.publish_traced(
        jam_nats::ROUTING_MANIFEST_UPDATED_SUBJECT,
        updated,
        &request.trace_ctx,
    )
    .await
    .map_err(|err| format!("publish routing-manifest.updated: {err}"))?;
    let patch = PatchApplied {
        service: request.service.clone(),
        from_version: from_version.unwrap_or_else(|| "none".into()),
        to_version: request.version.clone(),
        subject_prefix: updated.subject_prefix.clone(),
        ts: now,
    };
    let envelope = EventEnvelope::new(
        PatchApplied::EVENT_TYPE,
        PatchApplied::EVENT_SUBTYPE_VERSION,
        0,
        request.trace_ctx.trace_id.to_string(),
        request.requested_by.clone(),
        patch,
    );
    nats.publish_traced(PatchApplied::EVENT_TYPE, &envelope, &request.trace_ctx)
        .await
        .map_err(|err| format!("publish patch.applied: {err}"))
}

async fn acquire_patch_lock(
    nats: &JamNats,
    actor: &str,
    trace_ctx: &TraceCtx,
    service: &str,
    action: &str,
) -> Result<u64, String> {
    let kv = nats
        .jetstream()
        .get_key_value(PATCH_LOCK_BUCKET)
        .await
        .map_err(|err| format!("open {PATCH_LOCK_BUCKET} KV bucket: {err}"))?;
    let acquired_at = Utc::now();
    let record = PatchLockRecord {
        service: service.into(),
        action: action.into(),
        actor: actor.into(),
        trace_id: trace_ctx.trace_id.to_string(),
        acquired_at,
    };
    let payload =
        serde_json::to_vec(&record).map_err(|err| format!("serialize patch lock: {err}"))?;
    let revision = match kv.create(PATCH_LOCK_KEY, payload.into()).await {
        Ok(revision) => revision,
        Err(err) => {
            let holder = kv
                .get(PATCH_LOCK_KEY)
                .await
                .ok()
                .flatten()
                .and_then(|bytes| String::from_utf8(bytes.to_vec()).ok())
                .unwrap_or_else(|| "<unreadable>".into());
            return Err(format!(
                "patch lock is already held or unavailable: {err}\ncurrent {PATCH_LOCK_BUCKET}/{PATCH_LOCK_KEY}: {holder}"
            ));
        }
    };
    publish_patch_lock_event(
        nats,
        PatchLockAcquired::EVENT_TYPE,
        PatchLockAcquired::EVENT_SUBTYPE_VERSION,
        actor,
        trace_ctx,
        PatchLockAcquired { ts: acquired_at },
    )
    .await?;
    Ok(revision)
}

async fn release_patch_lock(
    nats: &JamNats,
    lock_revision: u64,
    actor: &str,
    trace_ctx: &TraceCtx,
) -> Result<(), String> {
    let kv = nats
        .jetstream()
        .get_key_value(PATCH_LOCK_BUCKET)
        .await
        .map_err(|err| format!("open {PATCH_LOCK_BUCKET} KV bucket: {err}"))?;
    kv.delete_expect_revision(PATCH_LOCK_KEY, Some(lock_revision))
        .await
        .map_err(|err| format!("release {PATCH_LOCK_BUCKET}/{PATCH_LOCK_KEY}: {err}"))?;
    publish_patch_lock_event(
        nats,
        PatchLockReleased::EVENT_TYPE,
        PatchLockReleased::EVENT_SUBTYPE_VERSION,
        actor,
        trace_ctx,
        PatchLockReleased { ts: Utc::now() },
    )
    .await
}

async fn publish_patch_lock_event<T: Serialize>(
    nats: &JamNats,
    event_type: &'static str,
    subtype_version: u32,
    actor: &str,
    trace_ctx: &TraceCtx,
    payload: T,
) -> Result<(), String> {
    let envelope = EventEnvelope::new(
        event_type,
        subtype_version,
        0,
        trace_ctx.trace_id.to_string(),
        actor.to_owned(),
        payload,
    );
    nats.publish_traced(event_type, &envelope, trace_ctx)
        .await
        .map_err(|err| format!("publish {event_type}: {err}"))
}

fn finish_locked_patch_result<T>(
    result: Result<T, String>,
    release: Result<(), String>,
) -> Result<T, String> {
    match (result, release) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(err), Ok(())) => Err(err),
        (Ok(_), Err(err)) => Err(format!(
            "patch action completed but patch-lock release failed: {err}"
        )),
        (Err(result_err), Err(release_err)) => Err(format!(
            "{result_err}\nAdditionally, patch-lock release failed: {release_err}"
        )),
    }
}

async fn verify_patch_service_health(
    nats: &JamNats,
    service: &str,
    subject_prefix: &str,
    trace_ctx: &TraceCtx,
) -> Result<(), String> {
    let subject = format!("{subject_prefix}.ping");
    let timeout = patch_health_timeout();
    let deadline = tokio::time::Instant::now() + timeout;
    let mut last_error = None;

    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            let detail = last_error.unwrap_or_else(|| "no response received".into());
            return Err(format!(
                "candidate health check failed on {subject} within {}s: {detail}",
                timeout.as_secs()
            ));
        }
        let attempt_timeout = (deadline - now).min(Duration::from_secs(1));
        match nats
            .request_traced::<_, serde_json::Value>(
                &subject,
                &serde_json::json!({}),
                trace_ctx,
                attempt_timeout,
            )
            .await
        {
            Ok(response) => {
                let status = response
                    .get("status")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| {
                        format!("health response on {subject} is missing string status")
                    })?;
                if status != "ok" {
                    return Err(format!(
                        "health response on {subject} returned non-ok status: {status}"
                    ));
                }
                let actual_service = response
                    .get("service")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| {
                        format!("health response on {subject} is missing string service")
                    })?;
                let expected = jam_tools_core::deploy_targets::service_id(service)
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("jam-svc-{service}"));
                if actual_service != expected {
                    return Err(format!(
                        "health response on {subject} came from {actual_service}, expected {expected}"
                    ));
                }
                return Ok(());
            }
            Err(err) => {
                last_error = Some(err.to_string());
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

async fn drain_previous_patch_service(
    nats: &JamNats,
    trace_ctx: &TraceCtx,
    previous_route: Option<&jam_nats::RoutingService>,
    new_subject_prefix: &str,
) -> Result<(), String> {
    let Some(previous_route) = previous_route else {
        return Ok(());
    };
    if previous_route.subject_prefix == new_subject_prefix {
        return Ok(());
    }

    let subject = format!("{}.drain", previous_route.subject_prefix);
    let timeout = patch_drain_timeout();
    let response: serde_json::Value = nats
        .request_traced(&subject, &serde_json::json!({}), trace_ctx, timeout)
        .await
        .map_err(|err| {
            format!(
                "previous service drain failed on {subject} within {}s: {err}",
                timeout.as_secs()
            )
        })?;
    let status = response
        .get("status")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("drain response on {subject} is missing string status"))?;
    if status == "draining" {
        Ok(())
    } else {
        Err(format!(
            "drain response on {subject} returned unexpected status: {status}"
        ))
    }
}

fn install_staged_binary(
    staging_path: &Path,
    service: &str,
    version: &str,
) -> Result<InstalledPatchBinary, String> {
    install_staged_binary_in(&jam_home(), staging_path, service, version)
}

fn install_staged_binary_in(
    runtime_root: &Path,
    staging_path: &Path,
    service: &str,
    version: &str,
) -> Result<InstalledPatchBinary, String> {
    if !staging_path.is_file() {
        return Err(format!(
            "staged binary is missing: {}",
            staging_path.display()
        ));
    }
    validate_executable(staging_path)?;
    let runtime_path = patch_runtime_path_in(runtime_root, service, version);
    let staged_sha = sha256_file_hex(staging_path)?;
    install_via_atomic_rename(staging_path, &runtime_path)?;
    let runtime_sha = sha256_file_hex(&runtime_path)?;
    if runtime_sha != staged_sha {
        return Err(format!(
            "installed binary checksum mismatch: staged={staged_sha} runtime={runtime_sha}"
        ));
    }
    Ok(InstalledPatchBinary {
        runtime_path,
        binary_sha256: runtime_sha,
    })
}

fn validate_executable(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        let mode = fs::metadata(path)
            .map_err(|err| format!("stat {}: {err}", path.display()))?
            .permissions()
            .mode();
        if mode & 0o111 == 0 {
            return Err(format!(
                "staged binary is not executable: {}",
                path.display()
            ));
        }
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn patch_binary_basename(service: &str) -> String {
    // Look up the canonical binary name from the shared registry so we honour
    // the `jam-svc-<name>` vs `jam-<name>` convention without hardcoding it
    // here. Unknown services fall back to `jam-svc-<name>` for backward
    // compatibility with services that exist before they're registered.
    jam_tools_core::deploy_targets::binary_name(service)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("jam-svc-{service}"))
}

fn patch_runtime_path_in(root: &Path, service: &str, version: &str) -> PathBuf {
    let basename = patch_binary_basename(service);
    root.join("bin").join(format!("{basename}-{version}"))
}

fn patch_service_log_path_in(root: &Path, service: &str, version: &str, stream: &str) -> PathBuf {
    let basename = patch_binary_basename(service);
    root.join("logs")
        .join("patch")
        .join(format!("{basename}-{version}.{stream}.log"))
}

fn open_patch_service_log(path: &Path) -> Result<File, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("create {}: {err}", parent.display()))?;
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| format!("open {}: {err}", path.display()))
}

fn start_versioned_patch_service(
    root: &Path,
    installed: &InstalledPatchBinary,
    service: &str,
    version: &str,
    subject_prefix: &str,
    nats_url: &str,
    nats_token: Option<&str>,
) -> Result<PatchServiceProcess, String> {
    let stdout_log = patch_service_log_path_in(root, service, version, "stdout");
    let stderr_log = patch_service_log_path_in(root, service, version, "stderr");
    let stdout = open_patch_service_log(&stdout_log)?;
    let stderr = open_patch_service_log(&stderr_log)?;

    let mut command = ProcessCommand::new(&installed.runtime_path);
    command
        .env("NATS_URL", nats_url)
        .env("JAM_DEPLOY_VERSION", version)
        .env("JAM_TOOL_SUBJECT_PREFIX", subject_prefix)
        .env(service_subject_prefix_env(service), subject_prefix)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    if let Some(token) = nats_token {
        command.env("NATS_TOKEN", token);
    }
    #[cfg(unix)]
    {
        command.process_group(0);
    }

    let child = command.spawn().map_err(|err| {
        format!(
            "start candidate service {} with prefix {subject_prefix}: {err}",
            installed.runtime_path.display()
        )
    })?;
    Ok(PatchServiceProcess {
        child,
        stdout_log,
        stderr_log,
    })
}

fn service_subject_prefix_env(service: &str) -> String {
    let token: String = service
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    format!("JAM_{token}_SUBJECT_PREFIX")
}

fn patch_health_timeout() -> Duration {
    std::env::var("JAM_PATCH_HEALTH_TIMEOUT_SECS")
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .filter(|secs| *secs > 0)
        .map_or(
            Duration::from_secs(DEFAULT_PATCH_HEALTH_TIMEOUT_SECS),
            Duration::from_secs,
        )
}

fn patch_drain_timeout() -> Duration {
    std::env::var("JAM_PATCH_DRAIN_TIMEOUT_SECS")
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .filter(|secs| *secs > 0)
        .map_or(
            Duration::from_secs(DEFAULT_PATCH_DRAIN_TIMEOUT_SECS),
            Duration::from_secs,
        )
}

use jam_tools_core::hashing::sha256_file_hex;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[cfg(unix)]
    fn set_executable(path: &Path) {
        let mut perms = fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).unwrap();
    }

    #[cfg(not(unix))]
    fn set_executable(_: &Path) {}

    #[test]
    fn runtime_path_is_under_bin_dir() {
        let tmp = TempDir::new().unwrap();
        assert_eq!(
            patch_runtime_path_in(tmp.path(), "observe", "0.4.7"),
            tmp.path().join("bin/jam-svc-observe-0.4.7")
        );
        assert_eq!(
            patch_service_log_path_in(tmp.path(), "observe", "0.4.7", "stdout"),
            tmp.path()
                .join("logs/patch/jam-svc-observe-0.4.7.stdout.log")
        );
        assert_eq!(
            service_subject_prefix_env("observe"),
            "JAM_OBSERVE_SUBJECT_PREFIX"
        );
        assert_eq!(
            service_subject_prefix_env("review-agent"),
            "JAM_REVIEW_AGENT_SUBJECT_PREFIX"
        );
    }

    #[test]
    fn install_copies_and_hashes() {
        let tmp = TempDir::new().unwrap();
        let staged = tmp.path().join("source/jam-svc-observe-0.4.7");
        fs::create_dir_all(staged.parent().unwrap()).unwrap();
        fs::write(&staged, b"service-binary").unwrap();
        set_executable(&staged);

        let runtime_root = tmp.path().join("runtime");
        let installed =
            install_staged_binary_in(&runtime_root, &staged, "observe", "0.4.7").unwrap();

        assert_eq!(
            installed.runtime_path,
            runtime_root.join("bin/jam-svc-observe-0.4.7")
        );
        assert_eq!(
            fs::read(installed.runtime_path).unwrap(),
            b"service-binary".to_vec()
        );
        assert_eq!(installed.binary_sha256, sha256_file_hex(&staged).unwrap());
    }

    #[test]
    fn install_rejects_non_executable_when_unix() {
        let tmp = TempDir::new().unwrap();
        let staged = tmp.path().join("source/jam-svc-observe-0.4.7");
        fs::create_dir_all(staged.parent().unwrap()).unwrap();
        fs::write(&staged, b"service-binary").unwrap();
        let runtime_root = tmp.path().join("runtime");

        let result = install_staged_binary_in(&runtime_root, &staged, "observe", "0.4.7");
        #[cfg(unix)]
        match result {
            Ok(_) => panic!("non-executable staged binary should fail on unix"),
            Err(err) => assert!(err.contains("not executable")),
        }
        #[cfg(not(unix))]
        assert!(result.is_ok());
    }

    #[test]
    fn finish_locked_patch_result_reports_release_failure() {
        let err = finish_locked_patch_result::<()>(
            Ok(()),
            Err("release patch-lock/current: wrong revision".into()),
        )
        .unwrap_err();
        assert!(err.contains("patch action completed"));
        assert!(err.contains("wrong revision"));
    }

    #[test]
    fn patch_lock_record_serializes_holder_context() {
        let trace = TraceCtx::new_root("test.patch", "lock");
        let record = PatchLockRecord {
            service: "observe".into(),
            action: "apply:0.4.7".into(),
            actor: "human:caleb".into(),
            trace_id: trace.trace_id.to_string(),
            acquired_at: Utc::now(),
        };
        let json = serde_json::to_value(record).unwrap();
        assert_eq!(json["service"], "observe");
        assert_eq!(json["action"], "apply:0.4.7");
        assert_eq!(json["actor"], "human:caleb");
        assert_eq!(json["trace_id"], trace.trace_id.to_string());
    }
}
