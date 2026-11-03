//! Is there a newer Axiom? One anonymous request for the latest published release; the answer is
//! shown as a pill in the title bar. Nothing is downloaded or installed from here.

use axiom_engine::tool::{Ctx, Effects, Requires, ToolOutput, ToolSpec, Toolset, parse_params, schema_of};
use axiom_evidence::ToolError;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::Duration;

/// Where the latest published release is described.
const LATEST: &str = "https://api.github.com/repos/LakoMoor/Axiom/releases/latest";
/// The page a person opens to get it.
pub const RELEASES_PAGE: &str = "https://github.com/LakoMoor/Axiom/releases/latest";

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoParams {}

pub fn specs() -> Vec<ToolSpec> {
    vec![ToolSpec {
        id: "app_update_check",
        title: "Is there a newer Axiom?",
        summary: "Ask GitHub for the latest published Axiom release and compare it with this one: `current`, `latest`, `newer` (true when the latest is newer) and `url`. Sends one anonymous request; installs nothing.",
        beginner: "Checks whether a newer version of Axiom is out.",
        toolset: Toolset::Setup,
        effects: Effects::ACCESSES_NETWORK,
        requires: Requires::Nothing,
        input_schema: schema_of::<NoParams>,
        run: update_check,
    }]
}

/// The numbers of a version (`v1.2.3-rc.1` → `[1, 2, 3]`) and whether it is a release candidate.
fn parse(v: &str) -> Option<(Vec<u64>, bool)> {
    let v = v.trim().trim_start_matches('v');
    let (nums, pre) = match v.split_once('-') {
        Some((n, _)) => (n, true),
        None => (v, false),
    };
    let parts: Option<Vec<u64>> = nums.split('.').map(|p| p.parse().ok()).collect();
    parts.filter(|p| !p.is_empty()).map(|p| (p, pre))
}

/// Whether `latest` is newer than `current`. A final release beats its own release candidates.
pub fn is_newer(latest: &str, current: &str) -> bool {
    let (Some((a, a_pre)), Some((b, b_pre))) = (parse(latest), parse(current)) else { return false };
    match a.cmp(&b) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => b_pre && !a_pre,
    }
}

fn update_check(_ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let _: NoParams = parse_params(params)?;
    let agent = crate::http::quick_agent(Duration::from_secs(8));
    let v = crate::http::get_json(&agent, "GitHub", LATEST, &[("accept", "application/vnd.github+json".to_string())])?;
    let tag = v["tag_name"].as_str().unwrap_or_default().trim_start_matches('v').to_string();
    let current = env!("CARGO_PKG_VERSION");
    Ok(ToolOutput::Value(json!({
        "current": current,
        "latest": tag,
        "newer": is_newer(&tag, current),
        "url": RELEASES_PAGE,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_means_a_higher_number_and_a_final_beats_its_candidate() {
        assert!(is_newer("0.3.1", "0.3.0"));
        assert!(is_newer("v1.0.0", "0.9.9"));
        assert!(is_newer("0.3.0", "0.3.0-rc.1"));
        assert!(!is_newer("0.3.0-rc.2", "0.3.0"));
        assert!(!is_newer("0.3.0", "0.3.0"));
        assert!(!is_newer("0.2.9", "0.3.0"));
    }

    #[test]
    fn nonsense_is_never_newer() {
        for bad in ["", "latest", "1..2", "v"] {
            assert!(!is_newer(bad, "0.3.0"), "{bad}");
            assert!(!is_newer("0.3.0", bad), "{bad}");
        }
    }
}
