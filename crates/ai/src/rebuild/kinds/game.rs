//! Games: built in Rust, studied with the game's own folder, and told how to play like the
//! original (mechanics sheet, decompiling in Deep mode, generated assets).

use super::*;

/// Told to a resumed builder when the person turned generated assets off after some were made.
const ASSETS_NOW_OFF: &str =
    "Generated assets are now off for this project: don't make more with assets_image_generate or assets_model_generate. Keep the ones already in app/.";

pub struct GameKind;

impl GameKind {
    pub const ID: &'static str = "game";
}

impl TargetKind for GameKind {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn words(&self) -> KindWords {
        KindWords {
            button: "Play the game",
            icon: "play",
            ready: "Your game is ready",
            change_hint: "Ask for a change, e.g. \"make it faster\" or \"add a high-score table\"",
        }
    }

    fn detect(&self, cx: &DetectCx) -> Option<u32> {
        // Built with the game stack (the person picked a game on Make): a game whatever its words
        // say ("Clone <its title> 1:1" never says "game").
        if cx.stack == stacks::GAME_STACK {
            Some(200)
        } else if cx.says(&["game", "gameplay", "steam", "level", "player", "游戏", "遊戲", "玩法"]) {
            Some(60)
        } else {
            None
        }
    }

    fn default_stack(&self, _cx: &DetectCx) -> &'static str {
        stacks::GAME_STACK
    }

    /// Rust named in the request ("…in Rust", "with Rust"), as whole words.
    fn named_stack(&self, cx: &DetectCx) -> Option<&'static str> {
        ["in", "with", "using"].iter().any(|p| cx.words.contains(&format!(" {p} rust "))).then_some(stacks::GAME_STACK)
    }

    fn studies_folder(&self) -> bool {
        true
    }

    fn goal(&self) -> &'static str {
        "a playable game that plays like the original: the same rules, controls, numbers and feel"
    }

    fn brief_sections(&self, cx: &BriefCx) -> Vec<String> {
        let target = &cx.ws.manifest.target;
        if matches!(target, TargetRef::Website { .. }) {
            return vec![screenshot::SCREENSHOT_CONTRACT.to_string(), self_test::SELF_TEST_CONTRACT.to_string()];
        }
        // Paid, so only when the person turned it on for this project and a key is set up.
        let generated = cx.assets && cx.ws.manifest.generate_assets;
        let mut s = vec![game_steps(cx.deep, cx.ghidra, matches!(target, TargetRef::Local { .. }), generated)];
        if generated {
            s.push(assets_note(cx.ws, cx.image_model));
        }
        s.push(screenshot::SCREENSHOT_CONTRACT.to_string());
        s.push(self_test::SELF_TEST_CONTRACT.to_string());
        s
    }

    /// The person may have turned generated assets on (or added a key) or off since the brief.
    fn follow_up_sections(&self, cx: &BriefCx) -> Vec<String> {
        if cx.assets && cx.ws.manifest.generate_assets {
            vec![assets_note(cx.ws, cx.image_model)]
        } else if !cx.ws.assets().is_empty() {
            vec![ASSETS_NOW_OFF.to_string()]
        } else {
            Vec::new()
        }
    }

    /// Every game must play its core loop; a game of a website has no mechanics sheet (its brief
    /// doesn't ask for one), and only Deep mode on a program brings in the original's files.
    fn checks(&self, ws: &Workspace) -> Vec<Box<dyn Check>> {
        let mut v: Vec<Box<dyn Check>> = vec![Box::new(BuildCheck), Box::new(ScreenshotCheck), Box::new(SelfTestCheck)];
        if !matches!(ws.manifest.target, TargetRef::Website { .. }) {
            v.push(Box::new(MechanicsCheck));
        }
        v
    }
}
