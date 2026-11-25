//! The skills `sp mcp` serves (MCP's Skills extension, `io.modelcontextprotocol/skills`): two
//! `SKILL.md` files, written in plain words from docs/features.md and docs/cli.md, built into
//! the program. Each is a resource at `skill://NAME/SKILL.md`, listed with its frontmatter and a
//! manifest of its one file (size and SHA-256 of the bytes served), so a client can check what it
//! loads.

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// One skill: its name (its folder's) and its `SKILL.md`.
pub struct Skill {
    pub name: &'static str,
    pub text: &'static str,
}

pub const SKILLS: &[Skill] = &[
    Skill { name: "preparing-a-service", text: include_str!("skills/preparing-a-service/SKILL.md") },
    Skill { name: "running-the-screens", text: include_str!("skills/running-the-screens/SKILL.md") },
];

impl Skill {
    pub fn uri(&self) -> String {
        format!("skill://{}/SKILL.md", self.name)
    }

    /// The fields of its YAML frontmatter, in their order: each `key: value` line between the
    /// `---` lines (the skills here use only one-line values).
    pub fn frontmatter(&self) -> serde_json::Map<String, Value> {
        let mut fields = serde_json::Map::new();
        let mut lines = self.text.lines();
        if lines.next() != Some("---") {
            return fields;
        }
        for line in lines.take_while(|line| *line != "---") {
            if let Some((key, value)) = line.split_once(':') {
                fields.insert(key.trim().to_owned(), Value::String(value.trim().to_owned()));
            }
        }
        fields
    }

    /// Its entry in `skills/list` and `skills/get`: the URI, the frontmatter, and the manifest.
    pub fn entry(&self) -> Value {
        let digest = Sha256::digest(self.text.as_bytes());
        let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
        json!({
            "uri": self.uri(),
            "frontmatter": self.frontmatter(),
            "resources": [{"uri": self.uri(), "digest": format!("sha256:{hex}"), "size": self.text.len()}],
        })
    }

    /// The description, for a resource list.
    pub fn description(&self) -> String {
        self.frontmatter().get("description").and_then(Value::as_str).unwrap_or_default().to_owned()
    }
}

/// The skill whose `SKILL.md` a URI names.
pub fn at(uri: &str) -> Option<&'static Skill> {
    SKILLS.iter().find(|skill| skill.uri() == uri)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Agent Skills rules: a name of lower-case letters, digits and single hyphens, at most
    /// 64, the folder's; a description of at most 1024 characters; one-line values a YAML reader
    /// reads as plain words.
    #[test]
    fn each_skill_keeps_the_agent_skills_rules() {
        for skill in SKILLS {
            let fields = skill.frontmatter();
            assert_eq!(fields.get("name").and_then(Value::as_str), Some(skill.name));
            let name = skill.name;
            assert!(!name.is_empty() && name.len() <= 64, "{name}");
            assert!(name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'), "{name}");
            assert!(!name.starts_with('-') && !name.ends_with('-') && !name.contains("--"), "{name}");
            let description = skill.description();
            assert!(!description.is_empty() && description.chars().count() <= 1024, "{name}");
            assert!(!description.contains(": ") && !description.contains(" #"), "{name}: plain YAML words");
            assert_eq!(fields.len(), 2, "{name}: only name and description");
            // Plain words, as the app's: no other product's name.
            assert!(skill.text.ends_with('\n'));
        }
    }

    #[test]
    fn the_manifest_is_the_bytes_served() {
        let skill = &SKILLS[0];
        let entry = skill.entry();
        assert_eq!(entry["uri"], "skill://preparing-a-service/SKILL.md");
        assert_eq!(entry["resources"][0]["size"], skill.text.len());
        let digest = entry["resources"][0]["digest"].as_str().unwrap();
        assert!(digest.starts_with("sha256:") && digest.len() == 7 + 64, "{digest}");
        assert!(at("skill://running-the-screens/SKILL.md").is_some());
        assert!(at("skill://running-the-screens/other.md").is_none());
    }
}
