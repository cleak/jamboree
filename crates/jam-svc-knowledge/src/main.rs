//! `jam-svc-knowledge` - knowledge service slice for skills hot-edit events.
//!
//! This initial slice owns the recursive Linux inotify watcher for configured
//! skill paths. On file create/write/move/delete it emits `skills.changed` so
//! the Maestro can invalidate any per-session skill cache (§21.4, §21.5).

#![deny(missing_docs)]

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use chrono::Utc;
use clap::Parser;
use inotify::{EventMask, Inotify, WatchDescriptor, WatchMask};
use jam_events::generated::{Event, SkillsChanged};
use jam_events::EventEnvelope;
use jam_nats::JamNats;
use jam_trace::TraceCtx;
use serde::Deserialize;
use tracing::{error, info, warn};

const SERVICE_NAME: &str = "jam-svc-knowledge";
const SERVICE_VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_SKILLS_ROOT: &str = "/home/caleb/jamboree/skills";
const MIN_INOTIFY_WATCHES: u64 = 524_288;

#[derive(Debug, Parser)]
#[command(name = SERVICE_NAME, version, about = "Watch skill files and publish skills.changed")]
struct Cli {
    /// Skills config TOML; defaults to $JAM_SKILLS_CONFIG or $JAM_HOME/config/skills.toml.
    #[arg(long)]
    skills_config: Option<PathBuf>,

    /// Skills folder to watch recursively. May be passed multiple times.
    #[arg(long = "skills-root")]
    skills_roots: Vec<PathBuf>,

    /// Stop after publishing this many skills.changed events; useful for smoke tests.
    #[arg(long)]
    max_events: Option<u64>,
}

#[derive(Debug, thiserror::Error)]
enum KnowledgeError {
    #[error("nats: {0}")]
    Nats(#[from] jam_nats::NatsError),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("toml: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("inotify: {0}")]
    Inotify(String),

    #[error("config: {0}")]
    Config(String),

    #[error("watcher thread exited")]
    WatcherExited,
}

#[derive(Debug, Clone)]
struct Config {
    nats_url: String,
    nats_token: Option<String>,
    skill_toml: PathBuf,
    watched: WatchedPaths,
}

impl Config {
    fn from_env_and_cli(cli: &Cli) -> Result<Self, KnowledgeError> {
        let jam_home = jam_tools_core::paths::jam_home();
        let skills_config = cli
            .skills_config
            .clone()
            .or_else(|| std::env::var_os("JAM_SKILLS_CONFIG").map(PathBuf::from))
            .unwrap_or_else(|| jam_home.join("config").join("skills.toml"));
        let watched = if cli.skills_roots.is_empty() {
            WatchedPaths::from_config_or_default(&skills_config)?
        } else {
            WatchedPaths {
                folders: normalize_paths(cli.skills_roots.clone()),
                files: BTreeSet::new(),
            }
        };
        watched.validate()?;

        Ok(Self {
            nats_url: std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".into()),
            nats_token: std::env::var("NATS_TOKEN").ok(),
            skill_toml: skills_config,
            watched,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WatchedPaths {
    folders: BTreeSet<PathBuf>,
    files: BTreeSet<PathBuf>,
}

impl WatchedPaths {
    fn from_config_or_default(config_path: &Path) -> Result<Self, KnowledgeError> {
        if !config_path.exists() {
            return Ok(Self {
                folders: normalize_paths(vec![default_skills_root()]),
                files: BTreeSet::new(),
            });
        }

        let raw = fs::read_to_string(config_path)?;
        let parsed: SkillsConfigToml = toml::from_str(&raw)?;
        let Some(skills) = parsed.skills else {
            return Ok(Self {
                folders: normalize_paths(vec![default_skills_root()]),
                files: BTreeSet::new(),
            });
        };
        let folders = skills.folders.unwrap_or_default();
        let files = skills.files.unwrap_or_default();
        Ok(Self {
            folders: normalize_paths(if folders.is_empty() {
                vec![default_skills_root()]
            } else {
                folders
            }),
            files: normalize_paths(files),
        })
    }

    fn validate(&self) -> Result<(), KnowledgeError> {
        if self.folders.is_empty() && self.files.is_empty() {
            return Err(KnowledgeError::Config(
                "no skills folders or files configured".into(),
            ));
        }
        Ok(())
    }

    fn watch_roots(&self) -> BTreeSet<PathBuf> {
        let mut roots = self.folders.clone();
        for file in &self.files {
            if let Some(parent) = file.parent() {
                roots.insert(parent.to_path_buf());
            }
        }
        roots
    }

    fn should_publish(&self, path: &Path) -> bool {
        let normalized = normalize_path(path.to_path_buf());
        if self.files.contains(&normalized) {
            return true;
        }
        if normalized.extension().and_then(|ext| ext.to_str()) != Some("md") {
            return false;
        }
        self.folders
            .iter()
            .any(|folder| normalized.starts_with(folder))
    }
}

#[derive(Debug, Deserialize)]
struct SkillsConfigToml {
    skills: Option<SkillsSection>,
}

#[derive(Debug, Deserialize)]
struct SkillsSection {
    folders: Option<Vec<PathBuf>>,
    files: Option<Vec<PathBuf>>,
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    if let Err(err) = run().await {
        error!("jam-svc-knowledge fatal: {err}");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

async fn run() -> Result<(), KnowledgeError> {
    init_tracing();
    let cli = Cli::parse();
    check_inotify_limit()?;
    let config = Config::from_env_and_cli(&cli)?;

    info!(
        service = %SERVICE_NAME,
        version = %SERVICE_VERSION,
        nats = %config.nats_url,
        skills_config = %config.skill_toml.display(),
        folders = config.watched.folders.len(),
        files = config.watched.files.len(),
        "starting",
    );

    let nats = JamNats::connect(&config.nats_url, config.nats_token.clone()).await?;
    info!("connected to NATS");

    let (tx, rx) = mpsc::channel();
    let watched = config.watched.clone();
    std::thread::Builder::new()
        .name("jam-svc-knowledge-inotify".into())
        .spawn(move || SkillWatcher::start(watched, &tx))
        .map_err(KnowledgeError::from)?;

    let mut published = 0_u64;
    loop {
        match rx.recv() {
            Ok(WatchMessage::Changed(path)) => {
                publish_skills_changed(&nats, &path).await?;
                published = published.saturating_add(1);
                if cli
                    .max_events
                    .is_some_and(|max_events| published >= max_events)
                {
                    info!(published, "max events reached");
                    return Ok(());
                }
            }
            Ok(WatchMessage::Fatal(message)) => return Err(KnowledgeError::Inotify(message)),
            Err(_) => return Err(KnowledgeError::WatcherExited),
        }
    }
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("jam_svc_knowledge=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

fn check_inotify_limit() -> Result<(), KnowledgeError> {
    let raw = fs::read_to_string("/proc/sys/fs/inotify/max_user_watches")?;
    let current = raw.trim().parse::<u64>().map_err(|err| {
        KnowledgeError::Config(format!(
            "invalid fs.inotify.max_user_watches value {raw:?}: {err}"
        ))
    })?;
    if current < MIN_INOTIFY_WATCHES {
        return Err(KnowledgeError::Config(format!(
            "fs.inotify.max_user_watches = {current}; need >= {MIN_INOTIFY_WATCHES}. Fix: echo 'fs.inotify.max_user_watches=524288' | sudo tee -a /etc/sysctl.d/99-jam.conf && sudo sysctl --system"
        )));
    }
    Ok(())
}

async fn publish_skills_changed(nats: &JamNats, path: &Path) -> Result<(), KnowledgeError> {
    let file_path = path.display().to_string();
    let ctx = TraceCtx::new_root("skills.changed", format!("skill file changed: {file_path}"));
    let payload = SkillsChanged {
        file_path: file_path.clone(),
        ts: Utc::now(),
    };
    let envelope = EventEnvelope::new(
        SkillsChanged::EVENT_TYPE,
        SkillsChanged::EVENT_SUBTYPE_VERSION,
        0,
        ctx.trace_id.to_string(),
        SERVICE_NAME,
        payload,
    );
    nats.publish_traced("journal.skills.changed", &envelope, &ctx)
        .await?;
    info!(file_path, "published skills.changed");
    Ok(())
}

enum WatchMessage {
    Changed(PathBuf),
    Fatal(String),
}

struct SkillWatcher {
    watched: WatchedPaths,
    inotify: Inotify,
    dirs_by_watch: HashMap<WatchDescriptor, PathBuf>,
}

impl SkillWatcher {
    fn start(watched: WatchedPaths, tx: &mpsc::Sender<WatchMessage>) {
        if let Err(err) = Self::run(watched, tx) {
            let _ = tx.send(WatchMessage::Fatal(err.to_string()));
        }
    }

    fn run(watched: WatchedPaths, tx: &mpsc::Sender<WatchMessage>) -> Result<(), KnowledgeError> {
        let mut service = Self {
            watched,
            inotify: Inotify::init().map_err(|err| KnowledgeError::Inotify(err.to_string()))?,
            dirs_by_watch: HashMap::new(),
        };
        service.add_initial_watches()?;
        service.event_loop(tx)
    }

    fn add_initial_watches(&mut self) -> Result<(), KnowledgeError> {
        for root in self.watched.watch_roots() {
            self.add_dir_recursive(&root)?;
        }
        info!(
            watches = self.dirs_by_watch.len(),
            "skill watches installed"
        );
        Ok(())
    }

    fn add_dir_recursive(&mut self, root: &Path) -> Result<(), KnowledgeError> {
        if !root.is_dir() {
            warn!(path = %root.display(), "skills watch root is not a directory");
            return Ok(());
        }
        self.add_dir(root)?;
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                self.add_dir_recursive(&path)?;
            }
        }
        Ok(())
    }

    fn add_dir(&mut self, path: &Path) -> Result<(), KnowledgeError> {
        if self.dirs_by_watch.values().any(|existing| existing == path) {
            return Ok(());
        }
        let descriptor = self
            .inotify
            .watches()
            .add(
                path,
                WatchMask::CREATE
                    | WatchMask::MODIFY
                    | WatchMask::CLOSE_WRITE
                    | WatchMask::MOVED_TO
                    | WatchMask::MOVED_FROM
                    | WatchMask::DELETE
                    | WatchMask::DELETE_SELF
                    | WatchMask::MOVE_SELF,
            )
            .map_err(|err| KnowledgeError::Inotify(format!("watch {}: {err}", path.display())))?;
        self.dirs_by_watch.insert(descriptor, path.to_path_buf());
        Ok(())
    }

    fn event_loop(&mut self, tx: &mpsc::Sender<WatchMessage>) -> Result<(), KnowledgeError> {
        let mut buffer = [0_u8; 16 * 1024];
        loop {
            let events = self
                .inotify
                .read_events_blocking(&mut buffer)
                .map_err(|err| KnowledgeError::Inotify(err.to_string()))?;
            let mut changed = BTreeSet::new();
            let mut new_dirs = Vec::new();

            for event in events {
                let Some(base_dir) = self.dirs_by_watch.get(&event.wd) else {
                    continue;
                };
                let path = event
                    .name
                    .map_or_else(|| base_dir.clone(), |name| base_dir.join(name));
                if is_new_directory_event(event.mask) {
                    new_dirs.push(path.clone());
                }
                if is_skill_change_event(event.mask) && self.watched.should_publish(&path) {
                    changed.insert(path);
                }
            }

            for dir in new_dirs {
                if let Err(err) = self.add_dir_recursive(&dir) {
                    warn!(path = %dir.display(), "failed to add recursive skill watch: {err}");
                }
            }

            for path in changed {
                if tx.send(WatchMessage::Changed(path)).is_err() {
                    return Ok(());
                }
            }
        }
    }
}

fn is_new_directory_event(mask: EventMask) -> bool {
    mask.contains(EventMask::ISDIR)
        && (mask.contains(EventMask::CREATE) || mask.contains(EventMask::MOVED_TO))
}

fn is_skill_change_event(mask: EventMask) -> bool {
    mask.intersects(
        EventMask::CREATE
            | EventMask::MODIFY
            | EventMask::CLOSE_WRITE
            | EventMask::MOVED_TO
            | EventMask::MOVED_FROM
            | EventMask::DELETE
            | EventMask::DELETE_SELF
            | EventMask::MOVE_SELF,
    )
}

fn default_skills_root() -> PathBuf {
    std::env::var_os("JAM_SKILLS_ROOT").map_or_else(|| DEFAULT_SKILLS_ROOT.into(), PathBuf::from)
}

fn normalize_paths(paths: Vec<PathBuf>) -> BTreeSet<PathBuf> {
    paths.into_iter().map(normalize_path).collect()
}

fn normalize_path(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn loads_skills_config_folders_and_files() {
        let temp = TempDir::new().unwrap();
        let config = temp.path().join("skills.toml");
        fs::write(
            &config,
            format!(
                "[skills]\nfolders = [\"{}\"]\nfiles = [\"{}\"]\n",
                temp.path().join("skills").display(),
                temp.path().join("AGENTS.md").display(),
            ),
        )
        .unwrap();

        let watched = WatchedPaths::from_config_or_default(&config).unwrap();

        assert!(watched.folders.contains(&temp.path().join("skills")));
        assert!(watched.files.contains(&temp.path().join("AGENTS.md")));
    }

    #[test]
    fn should_publish_markdown_under_configured_folder() {
        let root = PathBuf::from("/tmp/jam-skills");
        let watched = WatchedPaths {
            folders: normalize_paths(vec![root.clone()]),
            files: BTreeSet::new(),
        };

        assert!(watched.should_publish(&root.join("projects/blueberry/hot-paths.md")));
        assert!(!watched.should_publish(&root.join("projects/blueberry/cache.tmp")));
    }

    #[test]
    fn should_publish_configured_individual_file() {
        let file = PathBuf::from("/home/caleb/blueberry/AGENTS.md");
        let watched = WatchedPaths {
            folders: BTreeSet::new(),
            files: normalize_paths(vec![file.clone()]),
        };

        assert!(watched.should_publish(&file));
    }
}
