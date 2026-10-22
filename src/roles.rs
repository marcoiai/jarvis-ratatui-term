use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Roles {
    pub investigator: String,
    pub architect: String,
    pub reviewer: String,
    #[serde(default)] pub investigator_model: String,
    #[serde(default)] pub architect_model: String,
    #[serde(default)] pub reviewer_model: String,
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(default = "default_solo_role")]
    pub solo_role: String,
}

impl Default for Roles {
    fn default() -> Self {
        Self {
            investigator: "ollama".into(),
            architect: "gemini".into(),
            reviewer: "groq".into(),
            investigator_model: String::new(),
            architect_model: String::new(),
            reviewer_model: String::new(),
            mode: default_mode(),
            solo_role: default_solo_role(),
        }
    }
}

fn default_mode() -> String { "solo".into() }
fn default_solo_role() -> String { "investigator".into() }

impl Roles {
    pub fn load(workspace: &Path) -> Result<Self, String> {
        let path = workspace.join(".dude/roles.json");
        if !path.exists() { return Ok(Self::default()); }
        if fs::symlink_metadata(&path).map_err(|e|e.to_string())?.file_type().is_symlink() {
            return Err("Refusing symlink role configuration".into());
        }
        let data = fs::read(&path).map_err(|e|e.to_string())?;
        if data.len() > 16_384 { return Err("Role configuration exceeds size limit".into()); }
        let roles: Self = serde_json::from_slice(&data).map_err(|e|e.to_string())?;
        for value in [&roles.investigator, &roles.architect, &roles.reviewer] {
            if !Self::valid(value) { return Err(format!("Unknown provider in role configuration: {value}")); }
        }
        if !matches!(roles.mode.as_str(), "solo" | "symphony") { return Err("Invalid mode".into()); }
        if !matches!(roles.solo_role.as_str(), "investigator" | "architect" | "reviewer") { return Err("Invalid solo role".into()); }
        Ok(roles)
    }

    pub fn valid(provider: &str) -> bool {
        matches!(provider, "ollama" | "groq" | "gemini" | "openai")
    }

    pub fn set(&mut self, role: &str, provider: &str) -> Result<(), String> {
        if !Self::valid(provider) { return Err("Unknown provider".into()); }
        match role {
            "investigator" => self.investigator = provider.into(),
            "architect" => self.architect = provider.into(),
            "reviewer" => self.reviewer = provider.into(),
            _ => return Err("Use investigator, architect or reviewer".into()),
        }
        Ok(())
    }

    pub fn set_model(&mut self,role:&str,model:&str)->Result<(),String>{
        if model.len()>128 || model.is_empty() || !model.chars().all(|c|
            c.is_ascii_alphanumeric() || matches!(c, '-'|'_'|'.'|'/'|':')) {
            return Err("Invalid model name".into());
        }
        match role {
            "investigator"=>self.investigator_model=model.into(),
            "architect"=>self.architect_model=model.into(),
            "reviewer"=>self.reviewer_model=model.into(),
            _=>return Err("Unknown role".into())
        }
        Ok(())
    }
    pub fn model_for<'a>(&'a self,role:&str,default:&'a str)->&'a str {
        let model=match role {
            "investigator"=>&self.investigator_model,
            "architect"=>&self.architect_model,
            "reviewer"=>&self.reviewer_model,
            _=>return default
        };
        if model.is_empty(){default}else{model}
    }
    pub fn set_mode(&mut self, mode: &str) -> Result<(), String> {
        match mode {
            "solo" => self.mode = "solo".into(),
            "symphony" | "symfony" => self.mode = "symphony".into(),
            _ => return Err("Use solo or symphony".into()),
        }
        Ok(())
    }
    pub fn set_solo_role(&mut self, role: &str) -> Result<(), String> {
        if !matches!(role, "investigator" | "architect" | "reviewer") {
            return Err("Use investigator, architect or reviewer".into());
        }
        self.solo_role = role.into();
        Ok(())
    }
    pub fn selected_provider(&self) -> &str {
        match self.solo_role.as_str() {
            "architect" => &self.architect,
            "reviewer" => &self.reviewer,
            _ => &self.investigator,
        }
    }
    pub fn save(&self, workspace: &Path) -> Result<(), String> {
        let dir = workspace.join(".dude");
        fs::create_dir_all(&dir).map_err(|e|e.to_string())?;
        let target = dir.join("roles.json");
        if target.exists() && fs::symlink_metadata(&target).map_err(|e|e.to_string())?.file_type().is_symlink() {
            return Err("Refusing symlink role configuration".into());
        }
        let temp = dir.join(format!(".roles-{}.tmp", std::process::id()));
        let data = serde_json::to_vec_pretty(self).map_err(|e|e.to_string())?;
        let result = (|| -> Result<(), String> {
            use std::io::Write;
            let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&temp).map_err(|e|e.to_string())?;
            file.write_all(&data).map_err(|e|e.to_string())?;
            file.sync_all().map_err(|e|e.to_string())?;
            fs::rename(&temp, &target).map_err(|e|e.to_string())
        })();
        if result.is_err() { let _ = fs::remove_file(&temp); }
        result
    }

    pub fn summary(&self) -> String {
        format!("Investigator: {} [{}]\nArchitect: {} [{}]\nReviewer: {} [{}]\n",
        self.investigator,if self.investigator_model.is_empty(){"default"}else{&self.investigator_model},
        self.architect,if self.architect_model.is_empty(){"default"}else{&self.architect_model},
        self.reviewer,if self.reviewer_model.is_empty(){"default"}else{&self.reviewer_model})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configure_roles() {
        let mut roles=Roles::default();
        assert!(roles.set("architect","groq").is_ok());
        assert_eq!(roles.architect,"groq");
        assert!(roles.set("reviewer","invalid").is_err());
        assert!(roles.set("unknown","ollama").is_err());
        assert!(roles.set_mode("symfony").is_ok());
        assert_eq!(roles.mode, "symphony");
        assert!(roles.set_solo_role("reviewer").is_ok());
        assert_eq!(roles.selected_provider(),"groq");
    }
    #[test]
    fn serialized_roles_round_trip() {
        let roles=Roles::default();
        let value=serde_json::to_string(&roles).unwrap();
        assert_eq!(roles, serde_json::from_str::<Roles>(&value).unwrap());
    }
}
