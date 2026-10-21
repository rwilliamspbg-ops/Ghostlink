//! Skills: optional, loadable procedures an agent turn may consult.
//!
//! Phase 4 of the local assistant. A skill is a short procedure plus the set of
//! tools it is allowed to use, loaded on demand rather than pasted into a system
//! prompt. The critical property is the last one in that sentence: **a skill
//! cannot widen the capability boundary.** It may only *narrow* it.
//!
//! That is enforced structurally in [`SkillSet::allows`] rather than by
//! convention, because a skill is authored data — plausibly by the model, by a
//! user, or pulled from a `.agents/skills` directory — and treating it as trusted
//! input would hand it the ability to escalate. The intersection it computes can
//! only ever shrink the set of permitted tools relative to the server-side
//! classification.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::capability::CapabilityClass;

/// One loadable procedure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    /// When the skill applies, shown to the model alongside the name.
    #[serde(default)]
    pub description: String,
    /// The procedure itself — markdown, a few hundred words.
    #[serde(default)]
    pub instructions: String,
    /// Tools this skill is permitted to use.
    ///
    /// A *maximum*, never a grant: anything not listed is unavailable to the
    /// skill, and anything listed is still gated by the capability table when it
    /// actually runs.
    #[serde(default)]
    pub tools: Vec<String>,
    /// Optional directory the skill is scoped to. When set, the skill's tools may
    /// only touch paths inside it.
    #[serde(default)]
    pub workspace_subdir: Option<String>,
}

impl Skill {
    /// Whether this skill claims `tool` at all, ignoring capability classes.
    fn claims(&self, tool: &str) -> bool {
        // An empty list means "declares no tools", not "any tool": a skill with a
        // typo'd or omitted tool list must not become a blank cheque.
        self.tools.iter().any(|t| t == tool)
    }
}

/// A skill's name and summary, with no procedure body.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SkillSummary {
    pub name: String,
    pub description: String,
    pub tool_count: usize,
}

/// The skills available to a turn, keyed by name.
#[derive(Debug, Default)]
pub struct SkillSet {
    skills: HashMap<String, Skill>,
}

impl SkillSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, skill: Skill) {
        self.skills.insert(skill.name.clone(), skill);
    }

    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.skills.get(name)
    }

    pub fn len(&self) -> usize {
        self.skills.len()
    }

    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    /// Names and descriptions only — never the full procedure.
    ///
    /// The same explicit-memory discipline as `memory_catalog`: a turn discovers
    /// what skills exist by name and summary, then fetches one body on demand,
    /// so a large skill library doesn't sit in every prompt.
    pub fn catalog(&self) -> Vec<SkillSummary> {
        let mut out: Vec<SkillSummary> = self
            .skills
            .values()
            .map(|s| SkillSummary {
                name: s.name.clone(),
                description: s.description.clone(),
                tool_count: s.tools.len(),
            })
            .collect();
        // Stable order so a prompt doesn't reshuffle between identical turns.
        // OPTIMIZATION: Unstable sorting avoids scratch buffer allocations and stability tracking overhead.
        // Skill names are unique keys in the map, so relative order stability for duplicate keys is irrelevant.
        out.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// Whether a named skill may use a tool, given the capability table.
    ///
    /// This is the narrowing step: true only when the skill claims the tool
    /// *and* the server classifies it as `read`. A skill naming `write_file` does
    /// not gain it — it simply doesn't get it offered, and the same call would
    /// still hit the approval gate if the model reached for it another way.
    pub fn allows(&self, skill_name: &str, server: &str, tool: &str) -> bool {
        let Some(skill) = self.skills.get(skill_name) else {
            return false;
        };
        if !skill.claims(tool) {
            return false;
        }
        // Only observational tools can be pre-authorized by a skill. Write and
        // exec stay behind the human gate no matter which skill asked: a
        // procedure is not an authorization.
        crate::capability::classify(server, tool) == CapabilityClass::Read
    }

    /// The intersection of a skill's claimed tools and the read-only set.
    ///
    /// Used to build the tool list actually offered while a skill is active.
    pub fn permitted_tools(&self, skill_name: &str, catalog: &[(String, String)]) -> Vec<String> {
        if !self.skills.contains_key(skill_name) {
            return Vec::new();
        }
        catalog
            .iter()
            .filter(|(server, tool)| self.allows(skill_name, server, tool))
            .map(|(_, tool)| tool.to_string())
            .collect()
    }

    /// Loads skills from a directory of `*.md` files with front matter, plus any
    /// `*.json` files.
    ///
    /// A malformed file is skipped rather than failing the load: one bad skill
    /// shouldn't take the others down. Returns the set and how many files were
    /// skipped.
    pub fn load_from_dir(dir: &Path) -> (Self, usize) {
        let mut set = Self::new();
        let mut skipped = 0usize;
        let Ok(entries) = std::fs::read_dir(dir) else {
            return (set, 0);
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let parsed = match ext {
                "json" => parse_skill_json(&path),
                "md" => parse_skill_markdown(&path),
                _ => continue,
            };
            match parsed {
                Ok(skill) => set.insert(skill),
                Err(_) => skipped += 1,
            }
        }
        (set, skipped)
    }
}

/// Parses one skill from JSON.
fn parse_skill_json(path: &Path) -> Result<Skill, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str::<Skill>(&raw).map_err(|e| e.to_string())
}

/// Parses a skill from a markdown file with front matter.
///
/// The format is deliberately the narrowest thing that works, because a skill
/// file is untrusted-adjacent input:
///
/// ```text
/// ---
/// name: triage-failing-test
/// description: Use when a test fails and you need the failure reason
/// tools: [run_tests, read_text_file]
/// ---
/// 1. Run the test.
/// 2. Read the output.
/// ```
///
/// No nested YAML, no includes, no interpolation. Only `name`, `description`,
/// `tools`, and `workspace_subdir` are recognized; the rest of the document is the
/// instruction body. Front matter without a name is rejected rather than guessed
/// at, since a nameless skill could never be selected.
pub fn parse_skill_markdown(path: &Path) -> Result<Skill, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    parse_skill_str(&raw)
}

/// The parsing half of [`parse_skill_markdown`], split out for testing.
pub fn parse_skill_str(raw: &str) -> Result<Skill, String> {
    let trimmed = raw.trim_start_matches('\u{feff}').trim_start();
    let rest = trimmed
        .strip_prefix("---")
        .ok_or_else(|| "missing front-matter delimiter".to_string())?;
    // Front matter must open on its own line.
    let rest = rest
        .strip_prefix("\r\n")
        .or_else(|| rest.strip_prefix('\n'))
        .ok_or_else(|| "front-matter delimiter must be followed by a newline".to_string())?;

    let end = rest
        .find("\n---")
        .ok_or_else(|| "unterminated front matter".to_string())?;
    let (front, body) = rest.split_at(end);
    // Drop the closing `---` explicitly rather than trimming: the split point is
    // the *newline* before the delimiter, so a leading `\r` or `\n` would
    // otherwise defeat every trim pattern, and with CRLF input the body would
    // come back as "---\r\nbody".
    let body = body
        .trim_start_matches('\n')
        .trim_start_matches('\r')
        .trim_start_matches('\n')
        .trim_start_matches("---")
        .trim_start_matches("\r\n")
        .trim_start_matches('\n');

    let mut name = None;
    let mut description = String::new();
    let mut tools: Vec<String> = Vec::new();
    let mut workspace_subdir = None;

    for line in front.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "name" => name = Some(value.trim_matches(['"', '\'']).to_string()),
            "description" => description = value.to_string(),
            "workspace_subdir" => workspace_subdir = Some(value.to_string()),
            "tools" => {
                let inner = value.trim().trim_start_matches('[').trim_end_matches(']');
                tools = inner
                    .split(',')
                    .map(|t| t.trim().trim_matches(['"', '\'']).to_string())
                    .filter(|t| !t.is_empty())
                    .collect();
            }
            _ => {}
        }
    }

    let name = name
        .filter(|n| !n.is_empty())
        .ok_or_else(|| "front matter has no `name`".to_string())?;

    Ok(Skill {
        name,
        description,
        instructions: body.trim().to_string(),
        tools,
        workspace_subdir,
    })
}

/// Resolves the skills directory.
///
/// `GHOSTLINK_SKILLS_DIR` if set, else `.agents/skills` under the configured
/// workspace root — the path the assistant brief names.
pub fn skills_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("GHOSTLINK_SKILLS_DIR") {
        return PathBuf::from(dir);
    }
    let root = std::env::var("GHOSTLINK_WORKSPACE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
        });
    root.join(".agents").join("skills")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill_with(tools: &[&str]) -> Skill {
        Skill {
            name: "s".to_string(),
            description: "d".to_string(),
            instructions: "do the thing".to_string(),
            tools: tools.iter().map(|t| t.to_string()).collect(),
            workspace_subdir: None,
        }
    }

    #[test]
    fn a_skill_cannot_widen_the_boundary_to_a_write() {
        let mut set = SkillSet::new();
        set.insert(skill_with(&["write_file"]));
        // The skill claims it, but the server-side class says write, so it is
        // not pre-authorized.
        assert!(!set.allows("s", "filesystem", "write_file"));
    }

    #[test]
    fn a_skill_cannot_widen_the_boundary_to_exec() {
        let mut set = SkillSet::new();
        set.insert(skill_with(&["run_command"]));
        assert!(!set.allows("s", "terminal", "run_command"));
    }

    #[test]
    fn a_skill_may_grant_a_read_it_declares() {
        let mut set = SkillSet::new();
        set.insert(skill_with(&["read_text_file"]));
        assert!(set.allows("s", "filesystem", "read_text_file"));
    }

    #[test]
    fn a_skill_cannot_use_a_tool_it_did_not_declare() {
        let mut set = SkillSet::new();
        set.insert(skill_with(&["read_text_file"]));
        assert!(!set.allows("s", "filesystem", "list_directory"));
    }

    #[test]
    fn an_empty_tool_list_grants_nothing() {
        // A skill with a missing or typo'd tool list must not become a blank
        // cheque for every read tool.
        let mut set = SkillSet::new();
        set.insert(skill_with(&[]));
        assert!(!set.allows("s", "filesystem", "read_text_file"));
        assert!(!set.allows("s", "calculator", "calculate"));
    }

    #[test]
    fn an_unknown_skill_allows_nothing() {
        let set = SkillSet::new();
        assert!(!set.allows("nope", "filesystem", "read_text_file"));
    }

    #[test]
    fn permitted_tools_is_the_intersection() {
        let mut set = SkillSet::new();
        set.insert(skill_with(&["read_text_file", "write_file"]));
        let catalog = vec![
            ("filesystem".to_string(), "read_text_file".to_string()),
            ("filesystem".to_string(), "write_file".to_string()),
            ("filesystem".to_string(), "list_directory".to_string()),
        ];
        assert_eq!(
            set.permitted_tools("s", &catalog),
            vec!["read_text_file".to_string()]
        );
    }

    #[test]
    fn catalog_carries_no_instructions() {
        let mut set = SkillSet::new();
        let mut s = skill_with(&["read_text_file"]);
        s.instructions = "SECRET PROCEDURE BODY".to_string();
        set.insert(s);
        let json = serde_json::to_string(&set.catalog()).unwrap();
        assert!(!json.contains("SECRET PROCEDURE BODY"));
    }

    #[test]
    fn catalog_is_sorted_for_stability() {
        let mut set = SkillSet::new();
        for n in ["zebra", "alpha", "middle"] {
            let mut s = skill_with(&[]);
            s.name = n.to_string();
            set.insert(s);
        }
        let got: Vec<String> = set.catalog().into_iter().map(|c| c.name).collect();
        assert_eq!(got, vec!["alpha", "middle", "zebra"]);
    }

    #[test]
    fn parses_markdown_front_matter() {
        let raw = "---\nname: triage\ndescription: Use when a test fails\ntools: [run_tests, read_text_file]\n---\nRun the test.\nRead the output.\n";
        let s = parse_skill_str(raw).unwrap();
        assert_eq!(s.name, "triage");
        assert_eq!(s.description, "Use when a test fails");
        assert_eq!(s.tools, vec!["run_tests", "read_text_file"]);
        assert!(s.instructions.contains("Run the test."));
        assert!(s.instructions.contains("Read the output."));
    }

    #[test]
    fn handles_crlf_line_endings() {
        let raw = "---\r\nname: win\r\ntools: [a]\r\n---\r\nbody";
        let s = parse_skill_str(raw).unwrap();
        assert_eq!(s.name, "win");
        assert_eq!(s.tools, vec!["a"]);
        assert_eq!(s.instructions, "body");
    }

    #[test]
    fn rejects_front_matter_without_a_name() {
        let err = parse_skill_str("---\ndescription: nameless\n---\nbody").unwrap_err();
        assert!(err.contains("no `name`"));
    }

    #[test]
    fn rejects_unterminated_front_matter() {
        assert!(parse_skill_str("---\nname: x\nbody without close").is_err());
    }

    #[test]
    fn rejects_missing_front_matter() {
        assert!(parse_skill_str("just a body, no front matter").is_err());
    }

    #[test]
    fn unknown_front_matter_keys_are_ignored() {
        let raw = "---\nname: x\nauthor: someone\nnested:\n  a: 1\n---\nbody";
        assert_eq!(parse_skill_str(raw).unwrap().name, "x");
    }

    #[test]
    fn quoted_tool_names_are_unwrapped() {
        let raw = "---\nname: x\ntools: [\"a\", 'b']\n---\nbody";
        assert_eq!(parse_skill_str(raw).unwrap().tools, vec!["a", "b"]);
    }

    #[test]
    fn loading_skips_a_malformed_file_and_keeps_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("good.md"), "---\nname: good\n---\nbody").unwrap();
        std::fs::write(dir.path().join("bad.md"), "no front matter at all").unwrap();
        let (set, skipped) = SkillSet::load_from_dir(dir.path());
        assert!(set.get("good").is_some());
        assert_eq!(skipped, 1);
    }

    #[test]
    fn loading_a_missing_directory_is_not_an_error() {
        let (set, skipped) = SkillSet::load_from_dir(Path::new("/definitely/not/here/skills"));
        assert!(set.is_empty());
        assert_eq!(skipped, 0);
    }

    #[test]
    fn skills_json_is_also_loaded() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("s.json"),
            r#"{"name":"j","description":"d","instructions":"i","tools":["calculate"]}"#,
        )
        .unwrap();
        let (set, _) = SkillSet::load_from_dir(dir.path());
        assert_eq!(
            set.get("j").expect("json skill should load").tools,
            vec!["calculate"]
        );
    }
}
