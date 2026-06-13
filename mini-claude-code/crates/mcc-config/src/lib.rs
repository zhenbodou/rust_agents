//! 配置加载：home (~/.mcc/settings.json) + project (.mcc/settings.json) 合并。

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub model: ModelConfig,
    #[serde(default)]
    pub permissions: PermissionConfig,
    #[serde(default)]
    pub budget: BudgetConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelConfig {
    pub main: String,
    pub subagent: String,
    pub summarize: String,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            main: "claude-opus-4-8".into(),
            subagent: "claude-sonnet-4-6".into(),
            summarize: "claude-haiku-4-5-20251001".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PermissionConfig {
    pub mode: Option<String>,
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub deny: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BudgetConfig {
    pub max_usd_per_session: f64,
    pub max_iterations: u32,
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            max_usd_per_session: 2.0,
            max_iterations: 40,
        }
    }
}

pub async fn load(cwd: &Path) -> anyhow::Result<Config> {
    let home = async {
        let home = dirs::home_dir()?;
        let p = home.join(".mcc/settings.json");
        if !p.exists() { return None; }
        let raw = tokio::fs::read_to_string(&p).await.ok()?;
        serde_json::from_str::<Config>(&raw).ok()
    };
    let project = async {
        let p = cwd.join(".mcc/settings.json");
        if !p.exists() { return None; }
        let raw = tokio::fs::read_to_string(&p).await.ok()?;
        serde_json::from_str::<Config>(&raw).ok()
    };
    let (h, p) = tokio::join!(home, project);
    Ok(merge(h, p))
}

fn merge(home: Option<Config>, proj: Option<Config>) -> Config {
    let mut base = home.unwrap_or_default();
    if let Some(p) = proj {
        base.permissions.deny.extend(p.permissions.deny);
        base.permissions.allow.extend(p.permissions.allow);
        if p.permissions.mode.is_some() {
            base.permissions.mode = p.permissions.mode;
        }
        // Only let the project override these if it actually set them.
        // (A project file that omits `budget`/`model` deserializes to defaults;
        // overwriting unconditionally would silently wipe the home config.)
        if p.budget != BudgetConfig::default() {
            base.budget = p.budget;
        }
        if p.model != ModelConfig::default() {
            base.model = p.model;
        }
    }
    base
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_without_budget_keeps_home_budget() {
        let home = Config {
            budget: BudgetConfig { max_usd_per_session: 9.0, max_iterations: 99 },
            ..Default::default()
        };
        // Project only sets permissions; budget/model parsed as defaults.
        let proj = Config {
            permissions: PermissionConfig {
                deny: vec!["Bash(rm:*)".into()],
                ..Default::default()
            },
            ..Default::default()
        };
        let merged = merge(Some(home), Some(proj));
        assert_eq!(merged.budget.max_usd_per_session, 9.0); // home preserved
        assert!(merged.permissions.deny.iter().any(|r| r == "Bash(rm:*)"));
    }

    #[test]
    fn project_budget_overrides_when_set() {
        let home = Config {
            budget: BudgetConfig { max_usd_per_session: 9.0, max_iterations: 99 },
            ..Default::default()
        };
        let proj = Config {
            budget: BudgetConfig { max_usd_per_session: 1.0, max_iterations: 5 },
            ..Default::default()
        };
        let merged = merge(Some(home), Some(proj));
        assert_eq!(merged.budget.max_usd_per_session, 1.0); // project wins
    }
}
