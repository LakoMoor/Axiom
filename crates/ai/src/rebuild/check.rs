//! Building a project with its own tools (cargo or npm), for [`BuildCheck`].

use super::*;

/// For non-website builds: run the project's build (cargo or npm) and report whether it succeeded, with a note (the first error lines when it didn't).
/// `place` says where `app` is, for messages (`Workspace::app_place`).
pub(super) fn build_check(app: &Path, place: &str, stop: &CancelToken) -> (bool, String) {
    let run = |prog: &Path, args: &[String], minutes: u64| {
        let s = stop.clone();
        craft_anything_platform::Process::new(prog)
            .args(args)
            .cwd(app)
            .timeout(Duration::from_secs(minutes * 60))
            // Engines go quiet for long stretches (shader compiles, cooking, a cold cargo build).
            .idle_timeout(Duration::from_secs(60 * 60))
            .stdout_cap(8 << 20)
            .cancel_when(move || s.is_cancelled())
            .run()
            .map(|o| (o.success, format!("{}{}", o.stdout_text(), o.stderr_text())))
            .map_err(|e| format!("Couldn't run the build: {}", e.message))
    };
    let result: Result<(bool, String), String> = if app.join("Cargo.toml").is_file() {
        run(Path::new("cargo"), &["build".into(), "--quiet".into()], 2 * 60)
    } else if app.join("package.json").is_file() {
        run(Path::new(if cfg!(windows) { "npm.cmd" } else { "npm" }), &["run".into(), "build".into()], 2 * 60)
    } else {
        return (false, format!("No project to build found in {place} yet."));
    };
    match result {
        Ok((true, _)) => (true, "Build succeeded.".into()),
        Ok((false, out)) => (false, format!("Build failed: {}", error_lines(&out))),
        Err(e) => (false, e),
    }
}

/// The lines that explain a failed build: error lines first, else the last lines.
pub(super) fn error_lines(out: &str) -> String {
    let lines: Vec<&str> = out.lines().filter(|l| !l.trim().is_empty()).collect();
    let errors: Vec<&str> = lines.iter().copied().filter(|l| l.to_ascii_lowercase().contains("error") || l.contains("licen")).take(12).collect();
    if errors.is_empty() { lines[lines.len().saturating_sub(12)..].join("\n") } else { errors.join("\n") }
}
