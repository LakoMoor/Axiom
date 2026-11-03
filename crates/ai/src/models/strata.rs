//! Strata: the one-click way to run a big local model for people who have never set one up.
//!
//! Strata is a separate program (a local inference engine with an OpenAI-compatible server on
//! `localhost`). Axiom downloads its installer from the project's release, checks the file against
//! the release's own `SHA256SUMS.txt`, runs it, then starts `strata launch --yes --no-tui` and adds
//! the server as an AI. Every program runs through `platform::process`: stdin closed, deadlines,
//! output capped, killed on cancel.

use axiom_evidence::{ErrorCode, ToolError};
use axiom_platform::process::{Cancel, Process};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Where the installer and its checksums are published (the project's own release).
pub const RELEASE: &str = "https://github.com/Niko1221/Strata/releases/latest/download";
/// The address Strata serves on when nothing else is asked for.
pub const DEFAULT_URL: &str = "http://127.0.0.1:8080";
/// What Axiom calls the server in the AI list.
pub const AI_NAME: &str = "Local: Strata";

const INSTALL_LIMIT: Duration = Duration::from_secs(20 * 60);
const LAUNCH_LIMIT: Duration = Duration::from_secs(6 * 60 * 60);
const INSTALLER_MAX: u64 = 2 * 1024 * 1024;

/// A model Strata can set up, chosen for people who don't want to choose.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Pack {
    pub id: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
    pub family: &'static str,
    pub model: &'static str,
    /// Memory (RAM plus graphics memory) it needs to be comfortable, in GB.
    pub memory_gb: u32,
}

/// The packs offered, smallest first. The first is the default.
pub const PACKS: &[Pack] = &[
    Pack {
        id: "everyday",
        title: "Everyday assistant",
        summary: "Qwen3.8 Flash-Next, the smallest pack: chat, writing and images.",
        family: "qwen",
        model: "q2_0",
        memory_gb: 16,
    },
    Pack {
        id: "coder",
        title: "Coding helper",
        summary: "Qwen3.8 Coder, tuned for programming and for the agent mode.",
        family: "coder",
        model: "IQ1_M",
        memory_gb: 16,
    },
];

pub fn pack(id: &str) -> Option<&'static Pack> {
    PACKS.iter().find(|p| p.id == id.trim())
}

/// The installer's file name for this system, or `None` where Strata has none.
pub fn installer_name(os: &str) -> Option<&'static str> {
    match os {
        "windows" => Some("install.ps1"),
        "linux" | "macos" => Some("install.sh"),
        _ => None,
    }
}

/// The arguments of `strata launch` that set a pack up and start it without asking anything.
pub fn launch_args(p: &Pack, context: &str) -> Vec<String> {
    ["launch", "--family", p.family, "--model", p.model, "--context", context, "--yes", "--no-tui"].iter().map(|s| (*s).to_string()).collect()
}

/// The line for `file` in a `SHA256SUMS.txt` (`<hash>  <name>` or `<hash> *<name>`), lowercase.
pub fn checksum_of(sums: &str, file: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let mut it = l.split_whitespace();
        let (hash, name) = (it.next()?, it.next()?);
        let ok = name.trim_start_matches('*') == file && hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit());
        ok.then(|| hash.to_ascii_lowercase())
    })
}

/// Where Strata's program is, when it is installed.
pub fn locate() -> Option<PathBuf> {
    axiom_platform::which::which("strata")
}

/// What is known about Strata on this computer.
#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub installed: bool,
    pub program: Option<String>,
    /// Whether a server answers at [`DEFAULT_URL`].
    pub running: bool,
    pub url: String,
    pub supported: bool,
    pub packs: Vec<Pack>,
}

pub fn status() -> Status {
    let program = locate();
    Status {
        installed: program.is_some(),
        program: program.map(|p| p.display().to_string()),
        running: healthy(DEFAULT_URL),
        url: format!("{DEFAULT_URL}/v1"),
        supported: installer_name(std::env::consts::OS).is_some(),
        packs: PACKS.to_vec(),
    }
}

/// Whether a Strata server answers `/health` at `base`.
pub fn healthy(base: &str) -> bool {
    let agent = crate::http::quick_agent(Duration::from_secs(2));
    agent.get(&format!("{}/health", base.trim_end_matches('/'))).call().is_ok_and(|r| r.status().as_u16() == 200)
}

/// Download the installer into `dir`, check it against the release's checksums, and run it.
pub fn install(dir: &Path, cancel: &Cancel, report: &dyn Fn(&str)) -> Result<PathBuf, ToolError> {
    let name = installer_name(std::env::consts::OS).ok_or_else(|| {
        ToolError::new(ErrorCode::FeatureMissing, "Strata has no installer for this system.")
            .with_remediation("Use a Windows, Linux or Apple Silicon Mac computer, or run a model with llama.cpp instead (models_server_start).")
    })?;
    std::fs::create_dir_all(dir).map_err(|e| ToolError::new(ErrorCode::FileError, format!("Couldn't make the download folder: {e}")))?;
    report("Downloading the installer…");
    let sums_path = dir.join("SHA256SUMS.txt");
    axiom_engines::install::download_file(&format!("{RELEASE}/SHA256SUMS.txt"), &sums_path, INSTALLER_MAX, Some(cancel), &|_, _| {})?;
    let sums = std::fs::read_to_string(&sums_path).map_err(|e| ToolError::new(ErrorCode::FileError, format!("Couldn't read the checksums: {e}")))?;
    let hash = checksum_of(&sums, name).ok_or_else(|| {
        ToolError::new(ErrorCode::NotFound, format!("The release's checksums don't list {name}."))
            .with_remediation("Try again later; Axiom won't run an installer it can't check.")
    })?;
    let script = dir.join(name);
    axiom_engines::install::download_verified(&format!("{RELEASE}/{name}"), &script, &hash, INSTALLER_MAX, Some(cancel), &|_, _| {})?;
    report("Installing Strata…");
    let process = if cfg!(windows) {
        Process::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(axiom_platform::process::native_path(&script))
    } else {
        Process::new("sh").arg(&script)
    };
    process
        .timeout(INSTALL_LIMIT)
        .idle_timeout(INSTALL_LIMIT)
        .stdout_cap(256 * 1024)
        .stderr_cap(256 * 1024)
        .label("Strata installer")
        .remediation("Check the internet connection and free disk space, then try again.")
        .cancel(cancel.clone())
        .run_checked()?;
    locate().ok_or_else(|| {
        ToolError::new(ErrorCode::EngineMissing, "Strata installed, but its program wasn't found.")
            .with_remediation("Open a new terminal window (or restart Axiom) so the new program is on the path, then try again.")
    })
}

/// Set a pack up and start it with `program`, waiting for the server to answer. Downloads can take
/// hours on a slow line; a repeated call resumes.
pub fn launch(program: &Path, p: &Pack, cancel: &Cancel, report: &dyn Fn(&str)) -> Result<String, ToolError> {
    if healthy(DEFAULT_URL) {
        return Ok(format!("{DEFAULT_URL}/v1"));
    }
    report("Setting up the model. The first time this downloads it: it can take a while.");
    Process::new(program)
        .args(launch_args(p, "32k"))
        .timeout(LAUNCH_LIMIT)
        .idle_timeout(Duration::from_secs(30 * 60))
        .stdout_cap(256 * 1024)
        .stderr_cap(256 * 1024)
        .label("strata launch")
        .remediation("Run `strata doctor` in a terminal, or pick a smaller pack.")
        .cancel(cancel.clone())
        .run_checked()?;
    // Exit code 0 isn't enough: a launch can finish after downloading without starting anything.
    for _ in 0..30 {
        if cancel() {
            return Err(ToolError::new(ErrorCode::Cancelled, "Stopped while the model was starting."));
        }
        if healthy(DEFAULT_URL) {
            return Ok(format!("{DEFAULT_URL}/v1"));
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    Err(ToolError::new(ErrorCode::ProgramFailed, "Strata finished, but no model is answering.")
        .with_remediation("The model may be downloaded but have no engine for this graphics card: run `strata doctor --live` in a terminal."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_are_unique_and_the_first_is_the_default() {
        assert_eq!(PACKS.first().map(|p| p.id), Some("everyday"));
        let mut ids: Vec<_> = PACKS.iter().map(|p| p.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), PACKS.len());
        assert!(pack("coder").is_some() && pack("nope").is_none());
    }

    #[test]
    fn launch_never_waits_for_an_answer() {
        let a = launch_args(&PACKS[0], "32k");
        assert!(a.iter().any(|x| x == "--yes") && a.iter().any(|x| x == "--no-tui"));
        let at = |f: &str| a.iter().position(|x| x == f).and_then(|i| a.get(i + 1)).cloned();
        assert_eq!((at("--family").as_deref(), at("--model").as_deref()), (Some("qwen"), Some("q2_0")));
    }

    #[test]
    fn checksums_are_read_only_for_the_exact_file() {
        let h = "a".repeat(64);
        let sums = format!("{h}  install.sh\n{} *install.ps1.sig\nbad  install.ps1\n", "b".repeat(64));
        assert_eq!(checksum_of(&sums, "install.sh"), Some(h));
        assert_eq!(checksum_of(&sums, "install.ps1"), None, "a short hash is refused");
        assert_eq!(checksum_of(&sums, "install"), None);
    }

    #[test]
    fn systems_without_an_installer_are_said_so() {
        assert_eq!(installer_name("windows"), Some("install.ps1"));
        assert_eq!(installer_name("freebsd"), None);
    }

    #[test]
    fn nothing_listens_on_a_closed_port() {
        assert!(!healthy("http://127.0.0.1:9"));
    }
}
