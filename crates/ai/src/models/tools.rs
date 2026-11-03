//! The models hub as tools: what to run, what's here, downloads, and running a model as an AI.

use super::{catalog, hf, server, store, strata};
use crate::AiHub;
use axiom_engine::EngineRef;
use axiom_engine::tool::{Ctx, Effects, Requires, ToolOutput, ToolSpec, Toolset, parse_params, schema_of};
use axiom_evidence::{ErrorCode, ToolError};
use axiom_platform::process::Cancel;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;

pub fn specs() -> Vec<ToolSpec> {
    let spec = |id, title, summary, beginner, effects, input_schema, run| ToolSpec {
        id,
        title,
        summary,
        beginner,
        toolset: Toolset::Setup,
        effects,
        requires: Requires::Nothing,
        input_schema,
        run,
    };
    let net = Effects::ACCESSES_NETWORK;
    vec![
        spec(
            "models_recommended_list",
            "Models worth running here",
            "The recommended local models, each with its size, what it's good at and whether it fits this computer's memory (`fit`: comfortable, tight, too_big, unknown), and whether it is already downloaded. `focus` (chat, code, tiny) narrows the list.",
            "Shows AI models you can run on this computer, and which ones fit.",
            Effects::READ_ONLY,
            schema_of::<RecommendedParams>,
            recommended_list,
        ),
        spec(
            "models_hub_search",
            "Search Hugging Face for models",
            "Search Hugging Face for models in the GGUF format, most downloaded first (`query`, `limit` up to 30). Needs no account.",
            "Searches the internet for more AI models.",
            net,
            schema_of::<SearchParams>,
            hub_search,
        ),
        spec(
            "models_repo_files_list",
            "List a model's files",
            "The single-file GGUF models in a Hugging Face repository (`repo`, as owner/name), smallest first, with sizes, quantisations and the hub's SHA-256 for each.",
            "Shows the downloadable versions of a model.",
            net,
            schema_of::<RepoParams>,
            repo_files_list,
        ),
        spec(
            "models_installed_list",
            "Models on this computer",
            "The model files downloaded here, with their sizes and where they came from, and which are running now.",
            "Shows the models you have downloaded.",
            Effects::READ_ONLY,
            schema_of::<NoParams>,
            installed_list,
        ),
        spec(
            "models_file_download",
            "Download a model",
            "Download a model file. Give `id` (a recommended model) or `repo` and `file`. The hub's SHA-256 is checked when it lists one; a mismatch deletes the file. Large: progress is reported.",
            "Downloads an AI model to this computer.",
            net | Effects::WRITES_FILESYSTEM,
            schema_of::<DownloadParams>,
            file_download,
        ),
        spec(
            "models_file_remove",
            "Delete a downloaded model",
            "Delete a downloaded model file (`file`). A model that's running is stopped first.",
            "Deletes a model you downloaded.",
            Effects::WRITES_FILESYSTEM,
            schema_of::<FileParams>,
            file_remove,
        ),
        spec(
            "models_server_start",
            "Run a downloaded model",
            "Start llama.cpp's server for a downloaded model (`file`, optional `context` in tokens) on this computer's loopback address and wait until it has loaded. It's then added as an AI named `Local: <file>`, usable like any other. Needs the llamaserver engine (engines_install).",
            "Starts a model so you can chat with it.",
            Effects::LAUNCHES_PROCESS | Effects::WRITES_FILESYSTEM,
            schema_of::<StartParams>,
            server_start,
        ),
        spec(
            "models_server_stop",
            "Stop a running model",
            "Stop the server running `file` and take its AI out of the list.",
            "Stops a running model.",
            Effects::LAUNCHES_PROCESS | Effects::WRITES_FILESYSTEM,
            schema_of::<FileParams>,
            server_stop,
        ),
        spec(
            "models_strata_status",
            "Is Strata set up?",
            "Whether Strata (the one-click local model program) is installed here, whether it is serving a model, and the packs it can set up (`packs`, each with the memory it needs).",
            "Checks whether the easy way to run a big model is ready.",
            Effects::READ_ONLY,
            schema_of::<NoParams>,
            strata_status,
        ),
        spec(
            "models_strata_install",
            "Install Strata",
            "Download Strata's installer from its release, check it against the release's SHA-256 list, and run it. Installs into the home folder; needs no administrator rights. Asks for consent: it runs a program from the internet.",
            "Installs the easy way to run a big model on your computer.",
            net | Effects::WRITES_FILESYSTEM | Effects::LAUNCHES_PROCESS | Effects::EXECUTES_FOREIGN_CODE,
            schema_of::<NoParams>,
            strata_install,
        ),
        spec(
            "models_strata_start",
            "Start a model with Strata",
            "Set up and start a Strata pack (`pack`: everyday or coder; default everyday). The first time this downloads the model, which is large. When it answers, it is added as an AI named `Local: Strata`.",
            "Downloads and starts a ready-made model, then lets you chat with it.",
            net | Effects::WRITES_FILESYSTEM | Effects::LAUNCHES_PROCESS,
            schema_of::<StrataStartParams>,
            strata_start,
        ),
        spec(
            "models_servers_list",
            "Models running now",
            "The model servers running in this session, with their addresses and how long they have been up.",
            "Shows which models are running.",
            Effects::READ_ONLY,
            schema_of::<NoParams>,
            servers_list,
        ),
    ]
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoParams {}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecommendedParams {
    /// `chat`, `code` or `tiny`; omitted: all.
    #[serde(default)]
    pub focus: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchParams {
    /// What to look for: a model family or name.
    pub query: String,
    /// How many results (default 12, at most 30).
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RepoParams {
    /// The Hugging Face repository, `owner/name`.
    pub repo: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DownloadParams {
    /// A recommended model's id (see models_recommended_list).
    #[serde(default)]
    pub id: Option<String>,
    /// Or a repository, `owner/name`…
    #[serde(default)]
    pub repo: Option<String>,
    /// …and the `.gguf` file in it.
    #[serde(default)]
    pub file: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FileParams {
    /// The model file's name, as models_installed_list shows it.
    pub file: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StrataStartParams {
    /// Which pack: everyday (default) or coder.
    #[serde(default)]
    pub pack: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StartParams {
    pub file: String,
    /// Context length in tokens (default 8192).
    #[serde(default)]
    pub context: Option<u32>,
}

fn models_dir(ctx: &Ctx<'_>) -> PathBuf {
    store::dir(&ctx.session.settings.engine_store_dir())
}

fn cancel_of(ctx: &Ctx<'_>) -> Cancel {
    let t = ctx.cancel.clone();
    Arc::new(move || t.is_cancelled())
}

/// The session's servers, made on first use.
fn servers(ctx: &mut Ctx<'_>) -> Arc<server::Servers> {
    if let Some(s) = ctx.session.ext.get::<server::Servers>() {
        return s;
    }
    let s = Arc::new(server::Servers::default());
    ctx.session.ext.insert(s.clone());
    s
}

fn hub(ctx: &Ctx<'_>) -> Result<AiHub, ToolError> {
    let engine = ctx
        .session
        .ext
        .get::<EngineRef>()
        .and_then(|r| r.0.upgrade())
        .ok_or_else(|| ToolError::new(ErrorCode::FeatureMissing, "Models can't be added as an AI in this engine."))?;
    Ok(AiHub::new(engine))
}

/// The name a running model has in the AI list.
pub fn backend_name(file: &str) -> String {
    format!("Local: {}", file.strip_suffix(".gguf").unwrap_or(file))
}

fn focus_of(s: &str) -> Result<catalog::Focus, ToolError> {
    match s.trim().to_ascii_lowercase().as_str() {
        "chat" => Ok(catalog::Focus::Chat),
        "code" => Ok(catalog::Focus::Code),
        "tiny" => Ok(catalog::Focus::Tiny),
        other => Err(ToolError::invalid(format!("{other:?} isn't a kind of model.")).with_remediation("Use chat, code or tiny.")),
    }
}

fn recommended_list(ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let p: RecommendedParams = parse_params(params)?;
    let focus = p.focus.as_deref().filter(|f| !f.trim().is_empty()).map(focus_of).transpose()?;
    let ram = axiom_engines::machine_memory_gb();
    let have = store::list(&models_dir(ctx));
    let items: Vec<Value> = catalog::RECOMMENDED
        .iter()
        .filter(|r| focus.is_none_or(|f| r.focus == f))
        .map(|r| {
            json!({
                "id": r.id, "title": r.title, "maker": r.maker, "about": r.about, "focus": r.focus,
                "repo": r.repo, "file": r.file, "size_bytes": r.size_bytes, "needs_ram_gb": r.needs_ram_gb(),
                "fit": r.fit(ram), "downloaded": have.iter().any(|h| h.file == r.file),
            })
        })
        .collect();
    let best: Vec<Value> = [catalog::Focus::Chat, catalog::Focus::Code, catalog::Focus::Tiny]
        .into_iter()
        .filter_map(|f| catalog::pick(f, ram).map(|r| json!({"focus": f, "id": r.id})))
        .collect();
    Ok(ToolOutput::Value(json!({"memory_gb": ram, "models": items, "best_for_this_computer": best})))
}

fn hub_search(_ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let p: SearchParams = parse_params(params)?;
    let hits = hf::search(&p.query, p.limit.unwrap_or(12))?;
    Ok(ToolOutput::Value(json!({"query": p.query, "results": hits})))
}

fn repo_files_list(_ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let p: RepoParams = parse_params(params)?;
    let files = hf::files(&p.repo)?;
    Ok(ToolOutput::Value(json!({"repo": p.repo, "files": files})))
}

fn installed_list(ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let _: NoParams = parse_params(params)?;
    let dir = models_dir(ctx);
    let running = servers(ctx).running();
    let items: Vec<Value> = store::list(&dir)
        .into_iter()
        .map(|i| json!({"file": i.file, "size_bytes": i.size_bytes, "repo": i.repo, "verified": i.sha256.is_some(), "running": running.iter().any(|r| r.file == i.file)}))
        .collect();
    Ok(ToolOutput::Value(json!({"folder": dir.display().to_string(), "models": items})))
}

fn file_download(ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let p: DownloadParams = parse_params(params)?;
    let (repo, file, known_size) = match (p.id.as_deref(), p.repo.as_deref(), p.file.as_deref()) {
        (Some(id), _, _) => {
            let r = catalog::by_id(id).ok_or_else(|| {
                ToolError::invalid(format!("There's no recommended model {id:?}.")).with_remediation("See models_recommended_list for the ids.")
            })?;
            (r.repo.to_string(), r.file.to_string(), Some(r.size_bytes))
        }
        (None, Some(repo), Some(file)) => (repo.to_string(), file.to_string(), None),
        _ => return Err(ToolError::invalid("Say which model: an `id`, or a `repo` and a `file`.").with_remediation("models_recommended_list shows ids.")),
    };
    // The hub lists the hash of every file; a failed lookup only means this one isn't checked against it.
    let sha = hf::files(&repo).ok().and_then(|f| f.into_iter().find(|f| f.name == file)).and_then(|f| f.sha256);
    let cancel = cancel_of(ctx);
    let dir = models_dir(ctx);
    let installed = {
        let reporter = &*ctx;
        store::download(&dir, &repo, &file, sha.as_deref(), Some(&cancel), &|done, total| {
            let total = total.or(known_size).filter(|t| *t > 0);
            let frac = total.map(|t| (done as f64 / t as f64).clamp(0.0, 1.0) as f32);
            reporter.report(frac, format!("Downloading {file}"));
        })?
    };
    Ok(ToolOutput::Value(
        json!({"file": installed.file, "size_bytes": installed.size_bytes, "verified": installed.sha256.is_some(), "folder": dir.display().to_string()}),
    ))
}

fn file_remove(ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let p: FileParams = parse_params(params)?;
    let dir = models_dir(ctx);
    store::path_of(&dir, &p.file)?;
    if servers(ctx).stop(&p.file) {
        let _ = hub(ctx).and_then(|h| h.remove_backend(&backend_name(&p.file)));
    }
    let removed = store::remove(&dir, &p.file)?;
    Ok(ToolOutput::Value(json!({"file": p.file, "removed": removed})))
}

fn server_start(ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let p: StartParams = parse_params(params)?;
    let model = store::path_of(&models_dir(ctx), &p.file)?;
    let program = ctx.session.settings.engine_path("llamaserver").ok_or_else(|| {
        ToolError::new(ErrorCode::EngineMissing, "The program that runs models (llama.cpp's server) isn't installed.")
            .with_remediation("Install it with engines_install llamaserver.")
    })?;
    let cancel = cancel_of(ctx);
    let all = servers(ctx);
    let run = {
        let reporter = &*ctx;
        all.start(&program, &model, p.context.unwrap_or(server::DEFAULT_CONTEXT), &cancel, &|m| reporter.report(None, m))?
    };
    // Add it as an AI; if that fails, the server is stopped so nothing runs unseen.
    let name = backend_name(&run.file);
    let added = hub(ctx).and_then(|h| h.add_server(&name, &run.base_url, Some(&run.file), None));
    if let Err(e) = added {
        all.stop(&run.file);
        return Err(e);
    }
    Ok(ToolOutput::Value(json!({"running": run, "ai": name})))
}

fn server_stop(ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let p: FileParams = parse_params(params)?;
    let stopped = servers(ctx).stop(&p.file);
    let _ = hub(ctx).and_then(|h| h.remove_backend(&backend_name(&p.file)));
    Ok(ToolOutput::Value(json!({"file": p.file, "stopped": stopped})))
}

fn strata_status(_ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let _: NoParams = parse_params(params)?;
    Ok(ToolOutput::Value(json!(strata::status())))
}

fn strata_install(ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let _: NoParams = parse_params(params)?;
    let cancel = cancel_of(ctx);
    let dir = ctx.session.settings.engine_store_dir().join("strata-installer");
    let program = {
        let reporter = &*ctx;
        strata::install(&dir, &cancel, &|m| reporter.report(None, m))?
    };
    let _ = std::fs::remove_dir_all(&dir);
    Ok(ToolOutput::Value(json!({"installed": true, "program": program.display().to_string()})))
}

fn strata_start(ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let p: StrataStartParams = parse_params(params)?;
    let id = p.pack.as_deref().unwrap_or("everyday");
    let chosen = strata::pack(id)
        .ok_or_else(|| ToolError::invalid(format!("There is no pack called {id}.")).with_remediation("Use everyday or coder (see models_strata_status)."))?;
    let program = strata::locate()
        .ok_or_else(|| ToolError::new(ErrorCode::EngineMissing, "Strata isn't installed.").with_remediation("Install it with models_strata_install."))?;
    let cancel = cancel_of(ctx);
    let url = {
        let reporter = &*ctx;
        strata::launch(&program, chosen, &cancel, &|m| reporter.report(None, m))?
    };
    hub(ctx).and_then(|h| h.add_server(strata::AI_NAME, &url, None, None))?;
    Ok(ToolOutput::Value(json!({"running": true, "url": url, "ai": strata::AI_NAME, "pack": chosen.id})))
}

fn servers_list(ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let _: NoParams = parse_params(params)?;
    Ok(ToolOutput::Value(json!({"running": servers(ctx).running()})))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axiom_engine::{CallOptions, Caller, Engine};

    fn call(e: &Engine, id: &str, v: Value) -> Result<Value, ToolError> {
        e.call(id, v, CallOptions::new(Caller::Cli)).map(|o| o.to_json())
    }

    #[test]
    fn the_names_are_stable_and_inside_the_id_scheme() {
        assert_eq!(backend_name("Qwen3-8B-Q4_K_M.gguf"), "Local: Qwen3-8B-Q4_K_M");
        assert!(specs().iter().all(|s| s.id.starts_with("models_")));
    }

    #[test]
    fn recommendations_list_with_fit_and_nothing_downloaded() {
        let e = Engine::shared(axiom_engine::testing::session(), specs());
        let v = call(&e, "models_recommended_list", json!({})).unwrap();
        let n = v["result"]["models"].as_array().unwrap().len();
        assert_eq!(n, catalog::RECOMMENDED.len());
        assert!(v["result"]["models"].as_array().unwrap().iter().all(|m| m["downloaded"] == false));
        let code = call(&e, "models_recommended_list", json!({"focus": "code"})).unwrap();
        assert_eq!(code["result"]["models"].as_array().unwrap().len(), 3);
        assert!(call(&e, "models_recommended_list", json!({"focus": "huge"})).is_err());
    }

    #[test]
    fn bad_requests_are_refused_before_any_network_or_program_is_used() {
        let e = Engine::shared(axiom_engine::testing::session(), specs());
        assert!(call(&e, "models_file_download", json!({})).is_err());
        assert!(call(&e, "models_file_download", json!({"id": "nope"})).is_err());
        assert!(call(&e, "models_file_download", json!({"repo": "a/b", "file": "../x.gguf"})).is_err());
        assert!(call(&e, "models_file_remove", json!({"file": "../x"})).is_err());
        assert!(call(&e, "models_repo_files_list", json!({"repo": "nope"})).is_err());
        assert!(call(&e, "models_hub_search", json!({"query": " "})).is_err());
        let none = call(&e, "models_installed_list", json!({})).unwrap();
        assert!(none["result"]["models"].as_array().unwrap().is_empty());
        assert!(call(&e, "models_servers_list", json!({})).unwrap()["result"]["running"].as_array().unwrap().is_empty());
    }
}
