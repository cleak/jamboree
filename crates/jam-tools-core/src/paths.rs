//! Runtime path defaults shared by Jamboree tools and services.
//!
//! Per `security-setup.md` §7.1, services normally run as `maestro` and use
//! `/home/maestro/.jam`, while the user-facing CLI resolves `~/.jam` for the
//! invoking Manager unless `JAM_HOME` is set.

use std::ffi::OsStr;
use std::path::PathBuf;

/// The substrate user that owns orchestrator runtime state.
pub const MAESTRO_USER: &str = "maestro";

/// The default runtime home for services running as `maestro`.
pub const MAESTRO_JAM_HOME: &str = "/home/maestro/.jam";

/// Resolve `JAM_HOME` for the current process.
///
/// If `JAM_HOME` is set, that value wins. Otherwise `maestro` resolves to
/// `/home/maestro/.jam`; all other users resolve to `$HOME/.jam` when `HOME`
/// is available, falling back to the Maestro runtime home if the environment
/// is too sparse to identify a home directory.
pub fn jam_home() -> PathBuf {
    if let Some(explicit) = std::env::var_os("JAM_HOME") {
        return PathBuf::from(explicit);
    }

    let user = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_default();
    default_jam_home_for(&user, std::env::var_os("HOME").as_deref())
}

/// Compute the default `JAM_HOME` for an already-known user and home.
pub fn default_jam_home_for(user: &str, home: Option<&OsStr>) -> PathBuf {
    if user == MAESTRO_USER {
        return PathBuf::from(MAESTRO_JAM_HOME);
    }

    home.map_or_else(
        || PathBuf::from(MAESTRO_JAM_HOME),
        |home| PathBuf::from(home).join(".jam"),
    )
}

#[cfg(test)]
mod tests {
    use super::{default_jam_home_for, MAESTRO_JAM_HOME};
    use std::ffi::OsStr;
    use std::path::PathBuf;

    #[test]
    fn maestro_defaults_to_runtime_home() {
        assert_eq!(
            default_jam_home_for("maestro", Some(OsStr::new("/home/maestro"))),
            PathBuf::from(MAESTRO_JAM_HOME)
        );
    }

    #[test]
    fn caleb_defaults_to_invoking_home() {
        assert_eq!(
            default_jam_home_for("caleb", Some(OsStr::new("/home/caleb"))),
            PathBuf::from("/home/caleb/.jam")
        );
    }

    #[test]
    fn sparse_environment_falls_back_to_maestro_runtime_home() {
        assert_eq!(
            default_jam_home_for("", None),
            PathBuf::from(MAESTRO_JAM_HOME)
        );
    }
}
