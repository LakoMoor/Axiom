//! Running a local model: `llama-server` (llama.cpp) started on this computer's own loopback
//! address, on a free port, with the model's file. Axiom adds it as an AI that speaks the
//! OpenAI protocol (`BackendKind::OpenaiCompatible`), so every other part treats it like any server.
//!
//! The program runs through `platform::process` like every other: stdin closed, output capped, the
//! whole process group killed on stop or drop. Starting waits until the model has loaded (the
//! server answers `/health` with 200), bounded, cancellable, and stops waiting the moment the
//! program exits, with its last output in the error.

use craft_anything_evidence::{ErrorCode, ToolError};
use craft_anything_platform::process::{Cancel, Process, RunningProcess};
use craft_anything_platform::sync::Mutex;
use serde::Serialize;
use std::net::TcpListener;
use std::path::Path;
use std::time::{Duration, Instant};

/// Longest wait for a model to load (big files from a slow disk take a while).
const LOAD_LIMIT: Duration = Duration::from_secs(300);
/// A server runs until it's stopped; these only stop a runaway.
const RUN_LIMIT: Duration = Duration::from_secs(60 * 60 * 24 * 14);
/// Smallest and largest context, in tokens.
pub const CONTEXT: std::ops::RangeInclusive<u32> = 512..=131_072;
pub const DEFAULT_CONTEXT: u32 = 8192;

/// A server that's running.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Running {
    /// The model file it serves.
    pub file: String,
    pub port: u16,
    /// Where an OpenAI client talks to it (`http://127.0.0.1:PORT/v1`).
    pub base_url: String,
    pub context: u32,
    pub seconds_up: u64,
}

struct Entry {
    file: String,
    port: u16,
    context: u32,
    since: Instant,
    process: RunningProcess,
}

/// The servers started in this session.
#[derive(Default)]
pub struct Servers {
    list: Mutex<Vec<Entry>>,
}

/// A port nothing is listening on, found by asking the system for one.
fn free_port() -> Result<u16, ToolError> {
    let l = TcpListener::bind(("127.0.0.1", 0)).map_err(|e| ToolError::new(ErrorCode::FileError, format!("Couldn't find a free port: {e}")))?;
    l.local_addr().map(|a| a.port()).map_err(|e| ToolError::new(ErrorCode::FileError, format!("Couldn't find a free port: {e}")))
}

/// The arguments for `llama-server`. Kept apart so a test can read them.
pub fn server_args(model: &Path, port: u16, context: u32, alias: &str) -> Vec<String> {
    vec![
        "-m".into(),
        model.display().to_string(),
        "--host".into(),
        "127.0.0.1".into(),
        "--port".into(),
        port.to_string(),
        "-c".into(),
        context.to_string(),
        "--alias".into(),
        alias.into(),
        // The model's own chat template, so tool calls and thinking come out the way it was trained.
        "--jinja".into(),
        // Use the graphics chip when the build has one (CPU builds ignore it).
        "-ngl".into(),
        "99".into(),
        "--no-webui".into(),
    ]
}

/// Whether the server at `port` has finished loading.
fn healthy(port: u16) -> bool {
    let agent = crate::http::quick_agent(Duration::from_secs(2));
    agent.get(&format!("http://127.0.0.1:{port}/health")).call().is_ok_and(|r| r.status().as_u16() == 200)
}

impl Servers {
    fn guard(&self) -> craft_anything_platform::sync::MutexGuard<'_, Vec<Entry>> {
        self.list.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The servers that are running now (one that has exited is dropped from the list).
    pub fn running(&self) -> Vec<Running> {
        let mut list = self.guard();
        list.retain_mut(|e| !e.process.is_finished());
        list.iter().map(running_of).collect()
    }

    /// Stop the server for `file`. Whether there was one.
    pub fn stop(&self, file: &str) -> bool {
        let taken: Vec<Entry> = {
            let mut list = self.guard();
            let (stop, keep): (Vec<Entry>, Vec<Entry>) = std::mem::take(&mut *list).into_iter().partition(|e| e.file == file);
            *list = keep;
            stop
        };
        let any = !taken.is_empty();
        for mut e in taken {
            e.process.kill_and_wait(Duration::from_secs(5));
        }
        any
    }

    /// Start `llama-server` for `model` and wait until it can answer. A server already running
    /// for the same file is returned as it is.
    pub fn start(&self, program: &Path, model: &Path, context: u32, cancel: &Cancel, report: &dyn Fn(&str)) -> Result<Running, ToolError> {
        let file = model.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
        if let Some(r) = self.running().into_iter().find(|r| r.file == file) {
            return Ok(r);
        }
        if !model.is_file() {
            return Err(ToolError::new(ErrorCode::NotFound, format!("{file} isn't downloaded."))
                .with_remediation("Download it from the models hub first (models_file_download)."));
        }
        let context = context.clamp(*CONTEXT.start(), *CONTEXT.end());
        let port = free_port()?;
        let mut process = Process::new(program)
            .args(server_args(model, port, context, &file))
            .timeout(RUN_LIMIT)
            .idle_timeout(RUN_LIMIT)
            .stdout_cap(256 * 1024)
            .stderr_cap(256 * 1024)
            .label("llama-server")
            .remediation("Check that the model file is complete, or try a smaller model or context.")
            .cancel(cancel.clone())
            .spawn()?;
        report("Loading the model…");
        let began = Instant::now();
        loop {
            if cancel() {
                process.kill_and_wait(Duration::from_secs(5));
                return Err(ToolError::new(ErrorCode::Cancelled, "Stopped while the model was loading."));
            }
            if healthy(port) {
                break;
            }
            if let Some(result) = process.wait_timeout(Duration::from_millis(400)) {
                let said = match result {
                    Ok(o) => format!("{}{}", o.stderr_text(), o.stdout_text()),
                    Err(e) => e.message.clone(),
                };
                let tail: String = said.lines().rev().take(6).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
                return Err(ToolError::new(ErrorCode::ProgramFailed, format!("The model server stopped while loading {file}. {tail}"))
                    .with_remediation("The file may be damaged or too big for this computer's memory: try a smaller model."));
            }
            if began.elapsed() > LOAD_LIMIT {
                process.kill_and_wait(Duration::from_secs(5));
                return Err(ToolError::new(ErrorCode::TimedOut, format!("{file} didn't finish loading in {} minutes.", LOAD_LIMIT.as_secs() / 60))
                    .with_remediation("Try a smaller model, or close other programs to free memory."));
            }
        }
        let entry = Entry { file, port, context, since: Instant::now(), process };
        let r = running_of(&entry);
        self.guard().push(entry);
        Ok(r)
    }
}

fn running_of(e: &Entry) -> Running {
    Running {
        file: e.file.clone(),
        port: e.port,
        base_url: format!("http://127.0.0.1:{}/v1", e.port),
        context: e.context,
        seconds_up: e.since.elapsed().as_secs(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn the_server_listens_on_loopback_only_and_names_the_model() {
        let a = server_args(&PathBuf::from("/m/qwen.gguf"), 5123, 8192, "qwen.gguf");
        let at = |flag: &str| a.iter().position(|x| x == flag).and_then(|i| a.get(i + 1)).cloned();
        assert_eq!(at("--host").as_deref(), Some("127.0.0.1"), "never open to the network");
        assert_eq!((at("--port").as_deref(), at("-c").as_deref(), at("-m").as_deref()), (Some("5123"), Some("8192"), Some("/m/qwen.gguf")));
        assert_eq!(at("--alias").as_deref(), Some("qwen.gguf"));
        assert!(a.iter().any(|x| x == "--jinja"));
    }

    #[test]
    fn a_free_port_is_found_and_nothing_runs_at_first() {
        assert!(free_port().unwrap() > 0);
        let s = Servers::default();
        assert!(s.running().is_empty());
        assert!(!s.stop("a.gguf"));
    }

    #[test]
    fn a_missing_model_is_said_before_anything_starts() {
        let d = tempfile::tempdir().unwrap();
        let s = Servers::default();
        let never: Cancel = std::sync::Arc::new(|| false);
        let e = s.start(Path::new("/definitely/not/llama-server"), &d.path().join("m.gguf"), 4096, &never, &|_| {}).unwrap_err();
        assert_eq!(e.code, ErrorCode::NotFound);
    }

    #[test]
    fn a_program_that_cannot_start_is_an_error_not_a_hang() {
        let d = tempfile::tempdir().unwrap();
        let model = d.path().join("m.gguf");
        std::fs::write(&model, "x").unwrap();
        let s = Servers::default();
        let never: Cancel = std::sync::Arc::new(|| false);
        let e = s.start(Path::new("/definitely/not/llama-server"), &model, 4096, &never, &|_| {}).unwrap_err();
        assert!(!e.message.is_empty());
        assert!(s.running().is_empty());
    }
}

#[cfg(test)]
mod live {
    use super::*;

    /// A real run: `CA_LIVE_LLAMA` is the llama-server program, `CA_LIVE_MODEL` a .gguf file.
    /// `cargo test -p craft-anything-ai live_model -- --ignored`
    #[test]
    #[ignore = "starts a real model server; needs CA_LIVE_LLAMA and CA_LIVE_MODEL"]
    fn live_model_starts_answers_and_stops() {
        let (Ok(program), Ok(model)) = (std::env::var("CA_LIVE_LLAMA"), std::env::var("CA_LIVE_MODEL")) else { return };
        let s = Servers::default();
        let never: Cancel = std::sync::Arc::new(|| false);
        let r = s.start(Path::new(&program), Path::new(&model), 2048, &never, &|_| {}).unwrap();
        assert!(r.base_url.starts_with("http://127.0.0.1:"));
        let agent = crate::http::quick_agent(Duration::from_secs(120));
        let body = serde_json::json!({"model": r.file, "max_tokens": 16, "messages": [{"role": "user", "content": "Say hi in one word."}]});
        let mut res = agent.post(&format!("{}/chat/completions", r.base_url)).header("content-type", "application/json").send(body.to_string()).unwrap();
        let text = res.body_mut().read_to_string().unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        let said = v["choices"][0]["message"]["content"].as_str().unwrap_or_default().to_string();
        println!("model said: {said:?}");
        assert!(!said.trim().is_empty());
        assert_eq!(s.running().len(), 1);
        assert!(s.stop(&r.file));
        assert!(s.running().is_empty());
    }
}
