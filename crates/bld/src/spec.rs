//! The domain spec — the YAML a team authors — parsed into Rust, verbatim.
//!
//! Parsing here is *shape only*: it accepts any YAML that fits the structure and
//! makes no claim that the domain is sound. That judgement belongs to
//! [`crate::topology`], which derives the full grid and validates it. Keeping the
//! two apart means a malformed *file* and an unsound *domain* fail with different,
//! honest messages.

use serde::Deserialize;

/// The three provenance doors, by name. The only ways a state changes.
pub const DOOR_NAMES: [&str; 3] = ["proposal", "fact", "system_event"];

/// A whole domain, as authored.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DomainSpec {
    /// The domain's name (`loan-approval`, `town-hall-booking`).
    pub domain: String,
    /// The state a fresh instance starts in. Must be one of `states`.
    pub initial: String,
    /// Every state the domain can be in.
    pub states: Vec<String>,
    /// The intent door — what someone WANTS. Always present.
    pub proposal: Door,
    /// The verified-external-truth door — what the WORLD confirmed.
    #[serde(default)]
    pub fact: Option<Door>,
    /// The deterministic-runtime-fact door — retries, timeouts.
    #[serde(default)]
    pub system_event: Option<Door>,
}

/// One door: its inputs, and the edges those inputs open from some states.
///
/// Edges not listed are `no_edge` — a transition that does not exist. The author
/// never writes the absences; [`crate::topology`] derives them.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Door {
    /// `true` when the door is decided by `(state, input)` alone; `false` when it
    /// reads persisted data (recorded, not enforced here — it rides into the
    /// artifact so a reader knows whether the enumeration is complete).
    #[serde(default)]
    pub fixed_table: bool,
    /// Every input this door accepts.
    pub inputs: Vec<String>,
    /// The transitions those inputs open. Every `(state, input)` pair not covered
    /// here is a `no_edge`.
    #[serde(default)]
    pub edges: Vec<Edge>,
}

/// One authored transition. The fields are a superset across doors; which are
/// required is a per-door rule enforced in [`crate::topology`]:
/// - proposal / fact edges carry `to` (and proposal edges may carry `effect`);
/// - `system_event` edges carry `records` and no `to` (a runtime fact moves no
///   state — it records a pursuit decision).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Edge {
    pub from: String,
    pub input: String,
    #[serde(default)]
    pub to: Option<String>,
    #[serde(default)]
    pub effect: Option<String>,
    #[serde(default)]
    pub records: Option<String>,
}

/// Why a spec file could not even be read as a domain.
#[derive(Debug)]
pub enum ParseError {
    /// The file could not be read from disk.
    Io(std::io::Error),
    /// The bytes were not YAML matching the spec shape.
    Yaml(serde_yaml::Error),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "cannot read spec: {error}"),
            Self::Yaml(error) => write!(f, "spec is not valid YAML for this format: {error}"),
        }
    }
}

impl std::error::Error for ParseError {}

impl DomainSpec {
    /// Parse a spec from YAML text. Shape only — see the module note.
    ///
    /// # Errors
    /// [`ParseError::Yaml`] when the text does not fit the spec structure.
    pub fn from_yaml(text: &str) -> Result<Self, ParseError> {
        serde_yaml::from_str(text).map_err(ParseError::Yaml)
    }

    /// Read and parse a spec file.
    ///
    /// # Errors
    /// [`ParseError::Io`] if the file cannot be read; [`ParseError::Yaml`] if it
    /// does not parse.
    pub fn from_path(path: &std::path::Path) -> Result<Self, ParseError> {
        let text = std::fs::read_to_string(path).map_err(ParseError::Io)?;
        Self::from_yaml(&text)
    }

    /// The doors present on this spec, paired with their canonical name, in the
    /// fixed provenance order (`proposal`, then `fact`, then `system_event`).
    #[must_use]
    pub fn doors(&self) -> Vec<(&'static str, &Door)> {
        let mut doors: Vec<(&'static str, &Door)> = vec![("proposal", &self.proposal)];
        if let Some(door) = &self.fact {
            doors.push(("fact", door));
        }
        if let Some(door) = &self.system_event {
            doors.push(("system_event", door));
        }
        doors
    }
}
