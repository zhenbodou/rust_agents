use anyhow::Result;
use globset::{Glob, GlobSet, GlobSetBuilder};
use mcc_config::PermissionConfig;

#[derive(Debug)]
pub enum Decision {
    Allow,
    Deny(String),
    Ask(String),
}

pub struct PermissionRequest {
    pub category: String,
    pub action: Action,
}

pub enum Action {
    Bash { cmd: String },
    Path { path: String },
}

pub struct PermissionChecker {
    allow_bash_prefix: Vec<String>,
    deny_bash_prefix: Vec<String>,
    allow_paths: GlobSet,
    deny_paths: GlobSet,
    mode: String,
}

impl PermissionChecker {
    pub fn new(cfg: &PermissionConfig) -> Result<Self> {
        let mut allow_bash = Vec::new();
        let mut deny_bash = Vec::new();
        let mut allow_paths = GlobSetBuilder::new();
        let mut deny_paths = GlobSetBuilder::new();

        for rule in &cfg.allow {
            route(rule, &mut allow_bash, &mut allow_paths)?;
        }
        for rule in &cfg.deny {
            route(rule, &mut deny_bash, &mut deny_paths)?;
        }
        Ok(Self {
            allow_bash_prefix: allow_bash,
            deny_bash_prefix: deny_bash,
            allow_paths: allow_paths.build()?,
            deny_paths: deny_paths.build()?,
            mode: cfg.mode.clone().unwrap_or_else(|| "default".into()),
        })
    }

    pub fn check(&self, req: &PermissionRequest) -> Decision {
        // deny first, never overridable
        match &req.action {
            Action::Bash { cmd } => {
                if self.deny_bash_prefix.iter().any(|p| cmd.trim_start().starts_with(p)) {
                    return Decision::Deny("denied by deny rule".into());
                }
            }
            Action::Path { path } => {
                if self.deny_paths.is_match(path) {
                    return Decision::Deny("denied by deny rule".into());
                }
            }
        }
        // allow
        match &req.action {
            Action::Bash { cmd } => {
                if self.allow_bash_prefix.iter().any(|p| cmd.trim_start().starts_with(p)) {
                    return Decision::Allow;
                }
            }
            Action::Path { path } => {
                if self.allow_paths.is_match(path) {
                    return Decision::Allow;
                }
            }
        }
        if self.mode == "bypassPermissions" {
            return Decision::Allow;
        }
        match (&req.action, req.category.as_str()) {
            (Action::Path { .. }, "Read") => Decision::Allow,
            (Action::Path { .. }, "Write" | "Edit") if self.mode == "acceptEdits" => {
                Decision::Allow
            }
            _ => Decision::Ask("confirmation required".into()),
        }
    }
}

fn route(
    rule: &str,
    bash_prefix: &mut Vec<String>,
    path_glob: &mut GlobSetBuilder,
) -> Result<()> {
    let (cat, inner) = rule
        .split_once('(')
        .and_then(|(c, i)| i.strip_suffix(')').map(|s| (c, s)))
        .ok_or_else(|| anyhow::anyhow!("bad rule: {rule}"))?;
    match cat {
        "Bash" => {
            let p = inner.trim_end_matches(":*").to_string();
            bash_prefix.push(p);
        }
        "Read" | "Write" | "Edit" => {
            path_glob.add(Glob::new(inner)?);
        }
        _ => {}
    }
    Ok(())
}
