//! The models offered first in the hub: a typed table, not prompt text. Every entry names a real
//! public repository and the one file to take (4-bit, the usual balance of size and quality), with
//! the size in bytes so "will it fit on this computer?" is a sum, not a guess.

use serde::Serialize;

/// What a model is best at, in the words people choose by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Focus {
    /// Everyday questions and writing.
    Chat,
    /// Writing and fixing code.
    Code,
    /// Very small and fast: for older computers.
    Tiny,
}

impl Focus {
    pub fn label(self) -> &'static str {
        match self {
            Focus::Chat => "Chat",
            Focus::Code => "Code",
            Focus::Tiny => "Tiny",
        }
    }
}

/// One recommended model.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Recommended {
    /// Stable id for this entry.
    pub id: &'static str,
    /// The name people see.
    pub title: &'static str,
    /// Who made it.
    pub maker: &'static str,
    /// The Hugging Face repository the file comes from (`owner/name`).
    pub repo: &'static str,
    /// The file in that repository.
    pub file: &'static str,
    /// The file's size in bytes, as the repository lists it.
    pub size_bytes: u64,
    /// Parameters, in billions (a rough size class).
    pub params_b: f32,
    pub focus: Focus,
    /// One plain sentence.
    pub about: &'static str,
}

impl Recommended {
    /// Memory this model needs to run, in GB: the file, plus working space for the conversation.
    pub fn needs_ram_gb(&self) -> u64 {
        // The file is held in memory; the context and the runtime add about a quarter and a gigabyte.
        let gb = self.size_bytes as f64 / (1u64 << 30) as f64;
        (gb * 1.25 + 1.0).ceil() as u64
    }

    /// How it sits on a computer with `ram_gb` of memory (`None`: unknown).
    pub fn fit(&self, ram_gb: Option<u64>) -> Fit {
        let Some(ram) = ram_gb else { return Fit::Unknown };
        let need = self.needs_ram_gb();
        if ram >= need.saturating_mul(3) / 2 {
            Fit::Comfortable
        } else if ram >= need {
            Fit::Tight
        } else {
            Fit::TooBig
        }
    }
}

/// Whether a model suits this computer's memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    Comfortable,
    Tight,
    TooBig,
    Unknown,
}

/// The table, smallest first within each focus.
pub const RECOMMENDED: &[Recommended] = &[
    Recommended {
        id: "llama-3.2-1b",
        title: "Llama 3.2 1B",
        maker: "Meta",
        repo: "bartowski/Llama-3.2-1B-Instruct-GGUF",
        file: "Llama-3.2-1B-Instruct-Q4_K_M.gguf",
        size_bytes: 807_694_464,
        params_b: 1.2,
        focus: Focus::Tiny,
        about: "Runs on almost any computer. Good for quick, simple answers.",
    },
    Recommended {
        id: "qwen3-1.7b",
        title: "Qwen3 1.7B",
        maker: "Alibaba",
        repo: "unsloth/Qwen3-1.7B-GGUF",
        file: "Qwen3-1.7B-Q4_K_M.gguf",
        size_bytes: 1_107_409_472,
        params_b: 1.7,
        focus: Focus::Tiny,
        about: "Small and quick, and can think step by step before it answers.",
    },
    Recommended {
        id: "llama-3.2-3b",
        title: "Llama 3.2 3B",
        maker: "Meta",
        repo: "bartowski/Llama-3.2-3B-Instruct-GGUF",
        file: "Llama-3.2-3B-Instruct-Q4_K_M.gguf",
        size_bytes: 2_019_377_696,
        params_b: 3.2,
        focus: Focus::Chat,
        about: "A good everyday model for a laptop.",
    },
    Recommended {
        id: "gemma-3-4b",
        title: "Gemma 3 4B",
        maker: "Google",
        repo: "unsloth/gemma-3-4b-it-GGUF",
        file: "gemma-3-4b-it-Q4_K_M.gguf",
        size_bytes: 2_489_894_016,
        params_b: 4.3,
        focus: Focus::Chat,
        about: "Clear writing and answers in many languages.",
    },
    Recommended {
        id: "qwen3-4b",
        title: "Qwen3 4B",
        maker: "Alibaba",
        repo: "unsloth/Qwen3-4B-GGUF",
        file: "Qwen3-4B-Q4_K_M.gguf",
        size_bytes: 2_497_281_312,
        params_b: 4.0,
        focus: Focus::Chat,
        about: "Strong for its size, and can think step by step.",
    },
    Recommended {
        id: "qwen3-8b",
        title: "Qwen3 8B",
        maker: "Alibaba",
        repo: "unsloth/Qwen3-8B-GGUF",
        file: "Qwen3-8B-Q4_K_M.gguf",
        size_bytes: 5_027_784_512,
        params_b: 8.2,
        focus: Focus::Chat,
        about: "The sweet spot for a computer with 16 GB of memory.",
    },
    Recommended {
        id: "qwen3-14b",
        title: "Qwen3 14B",
        maker: "Alibaba",
        repo: "unsloth/Qwen3-14B-GGUF",
        file: "Qwen3-14B-Q4_K_M.gguf",
        size_bytes: 9_001_753_984,
        params_b: 14.8,
        focus: Focus::Chat,
        about: "Noticeably more capable; wants 24 GB of memory or more.",
    },
    Recommended {
        id: "qwen2.5-coder-3b",
        title: "Qwen2.5 Coder 3B",
        maker: "Alibaba",
        repo: "bartowski/Qwen2.5-Coder-3B-Instruct-GGUF",
        file: "Qwen2.5-Coder-3B-Instruct-Q4_K_M.gguf",
        size_bytes: 1_929_903_360,
        params_b: 3.1,
        focus: Focus::Code,
        about: "Small and fast at code, for modest computers.",
    },
    Recommended {
        id: "qwen2.5-coder-7b",
        title: "Qwen2.5 Coder 7B",
        maker: "Alibaba",
        repo: "bartowski/Qwen2.5-Coder-7B-Instruct-GGUF",
        file: "Qwen2.5-Coder-7B-Instruct-Q4_K_M.gguf",
        size_bytes: 4_683_074_336,
        params_b: 7.6,
        focus: Focus::Code,
        about: "Writes and fixes code well on a 16 GB computer.",
    },
    Recommended {
        id: "qwen2.5-coder-14b",
        title: "Qwen2.5 Coder 14B",
        maker: "Alibaba",
        repo: "bartowski/Qwen2.5-Coder-14B-Instruct-GGUF",
        file: "Qwen2.5-Coder-14B-Instruct-Q4_K_M.gguf",
        size_bytes: 8_988_111_072,
        params_b: 14.8,
        focus: Focus::Code,
        about: "The strongest code model here; wants 24 GB of memory or more.",
    },
];

/// The recommended model with this id.
pub fn by_id(id: &str) -> Option<&'static Recommended> {
    RECOMMENDED.iter().find(|r| r.id == id)
}

/// The best model for `focus` that fits comfortably in `ram_gb`, else the smallest of that focus.
pub fn pick(focus: Focus, ram_gb: Option<u64>) -> Option<&'static Recommended> {
    let of_focus = || RECOMMENDED.iter().filter(move |r| r.focus == focus);
    of_focus().filter(|r| r.fit(ram_gb) == Fit::Comfortable).max_by_key(|r| r.size_bytes).or_else(|| of_focus().min_by_key(|r| r.size_bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_files_are_unique_and_well_formed() {
        let mut ids: Vec<&str> = RECOMMENDED.iter().map(|r| r.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), RECOMMENDED.len());
        for r in RECOMMENDED {
            assert!(r.file.ends_with(".gguf") && !r.file.contains('/'), "{}", r.id);
            assert!(super::super::hf::valid_repo(r.repo), "{}", r.repo);
            assert!(r.size_bytes > 100_000_000 && !r.about.is_empty());
        }
    }

    #[test]
    fn fit_follows_memory() {
        let m = by_id("qwen3-8b").unwrap();
        assert_eq!(m.needs_ram_gb(), 7);
        assert_eq!(m.fit(Some(32)), Fit::Comfortable);
        assert_eq!(m.fit(Some(8)), Fit::Tight);
        assert_eq!(m.fit(Some(4)), Fit::TooBig);
        assert_eq!(m.fit(None), Fit::Unknown);
    }

    #[test]
    fn the_pick_is_the_biggest_that_fits_comfortably() {
        assert_eq!(pick(Focus::Chat, Some(16)).map(|r| r.id), Some("qwen3-8b"));
        assert_eq!(pick(Focus::Code, Some(16)).map(|r| r.id), Some("qwen2.5-coder-7b"));
        assert_eq!(pick(Focus::Chat, Some(64)).map(|r| r.id), Some("qwen3-14b"));
        // Too little memory for anything comfortable: the smallest of the kind.
        assert_eq!(pick(Focus::Code, Some(2)).map(|r| r.id), Some("qwen2.5-coder-3b"));
        assert_eq!(pick(Focus::Chat, None).map(|r| r.id), Some("llama-3.2-3b"));
    }
}
