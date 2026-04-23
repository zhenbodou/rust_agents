//! 第 11 章：细粒度权限系统。
//! deny > allow > mode default > ask，deny 永远优先、无法覆盖。

use anyhow::{anyhow, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    Default,
    AcceptEdits,
    Bypass,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct PermissionConfig {
    #[serde(default)]
    pub mode: Option<PermissionMode>,
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub deny: Vec<String>,
}

#[derive(Debug)]
pub enum Decision {
    Allow,
    Deny(String),
    Ask(String),
}

#[derive(Debug)]
pub struct PermissionRequest {
    pub category: String,
    pub action: Action,
}

#[derive(Debug)]
pub enum Action {
    Bash { cmd: String },
    Path { path: String },
    Network { host: String },
}

pub struct Rule {
    pub category: String,
    pub matcher: RuleMatcher,
}

pub enum RuleMatcher {
    BashPrefix(String),
    PathGlob(GlobSet),
    HostGlob(GlobSet),
    Wildcard,
}

impl Rule {
    pub fn parse(raw: &str) -> Result<Self> {
        let (cat, inner) = raw
            .split_once('(')
            .and_then(|(c, i)| i.strip_suffix(')').map(|s| (c, s)))
            .ok_or_else(|| anyhow!("bad rule: {raw}"))?;
        let matcher = match cat {
            "Bash" => {
                if inner == "*" {
                    RuleMatcher::Wildcard
                } else if let Some(prefix) = inner.strip_suffix(":*") {
                    RuleMatcher::BashPrefix(prefix.trim().into())
                } else {
                    RuleMatcher::BashPrefix(inner.into())
                }
            }
            "Read" | "Write" | "Edit" => {
                let mut b = GlobSetBuilder::new();
                b.add(Glob::new(inner)?);
                RuleMatcher::PathGlob(b.build()?)
            }
            "Network" => {
                let mut b = GlobSetBuilder::new();
                b.add(Glob::new(inner)?);
                RuleMatcher::HostGlob(b.build()?)
            }
            _ => anyhow::bail!("unknown category {cat}"),
        };
        Ok(Rule {
            category: cat.into(),
            matcher,
        })
    }

    pub fn matches(&self, req: &PermissionRequest) -> bool {
        if self.category != req.category {
            return false;
        }
        match (&self.matcher, &req.action) {
            (RuleMatcher::Wildcard, _) => true,
            (RuleMatcher::BashPrefix(p), Action::Bash { cmd }) => {
                cmd.trim_start().starts_with(p)
            }
            (RuleMatcher::PathGlob(g), Action::Path { path }) => g.is_match(path),
            (RuleMatcher::HostGlob(g), Action::Network { host }) => g.is_match(host),
            _ => false,
        }
    }
}

pub struct PermissionChecker {
    mode: PermissionMode,
    allow: Vec<Rule>,
    deny: Vec<Rule>,
}

impl PermissionChecker {
    pub fn new(cfg: &PermissionConfig) -> Result<Self> {
        Ok(Self {
            mode: cfg.mode.unwrap_or(PermissionMode::Default),
            allow: cfg.allow.iter().map(|s| Rule::parse(s)).collect::<Result<_>>()?,
            deny: cfg.deny.iter().map(|s| Rule::parse(s)).collect::<Result<_>>()?,
        })
    }

    pub fn check(&self, req: &PermissionRequest) -> Decision {
        for r in &self.deny {
            if r.matches(req) {
                return Decision::Deny(format!("denied by rule {:?}", r.category));
            }
        }
        for r in &self.allow {
            if r.matches(req) {
                return Decision::Allow;
            }
        }
        if matches!(self.mode, PermissionMode::Bypass) {
            return Decision::Allow;
        }

        match (&req.action, self.mode) {
            (Action::Path { .. }, _) if req.category == "Read" => Decision::Allow,
            (Action::Path { .. }, PermissionMode::AcceptEdits)
                if matches!(req.category.as_str(), "Write" | "Edit") =>
            {
                Decision::Allow
            }
            _ => Decision::Ask(format!(
                "user confirmation required for {:?}",
                req.category
            )),
        }
    }
}

fn main() -> Result<()> {
    let cfg = PermissionConfig {
        mode: Some(PermissionMode::Default),
        allow: vec!["Bash(cargo test:*)".into(), "Read(**/*.rs)".into()],
        deny: vec!["Bash(rm *)".into(), "Read(**/.env)".into()],
    };
    let checker = PermissionChecker::new(&cfg)?;

    let cases = [
        ("Bash", Action::Bash { cmd: "cargo test --all".into() }, "allowed via allowlist"),
        ("Bash", Action::Bash { cmd: "rm -rf /tmp/x".into() }, "denied by deny"),
        ("Bash", Action::Bash { cmd: "ls -la".into() }, "ask (default)"),
        ("Read", Action::Path { path: "src/main.rs".into() }, "default allow for Read"),
        ("Read", Action::Path { path: ".env".into() }, "denied (secrets)"),
        ("Write", Action::Path { path: "src/foo.rs".into() }, "ask (default mode)"),
    ];

    for (cat, act, label) in cases {
        let req = PermissionRequest { category: cat.into(), action: act };
        let d = checker.check(&req);
        println!("{cat:5} {label:<30} => {d:?}");
    }
    Ok(())
}
