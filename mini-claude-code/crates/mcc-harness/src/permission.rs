//! PermissionChecker — 工具调用权限门控。
//! 规则格式：`Bash(cmd_prefix:*)` / `Read(**/*.rs)` / `Write(src/**)` / `Edit(**)`
//! deny 优先于 allow，两者均无匹配时走 mode 默认策略。

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
                let c = cmd.trim_start();
                if self.deny_bash_prefix.iter().any(|p| bash_word_match(c, p)) {
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
                let c = cmd.trim_start();
                if self.allow_bash_prefix.iter().any(|p| bash_word_match(c, p)) {
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

/// Match a bash command against a prefix at a word boundary.
///
/// `Bash(rm:*)` generates prefix `"rm"`.
/// - `"rm -rf /"` → matches (rm followed by space)
/// - `"rmdir /tmp"` → does NOT match (rm is a prefix of a different command)
fn bash_word_match(cmd: &str, prefix: &str) -> bool {
    if !cmd.starts_with(prefix) {
        return false;
    }
    let after = &cmd[prefix.len()..];
    after.is_empty() || after.starts_with(|c: char| c.is_ascii_whitespace() || c == '/')
}

fn route(rule: &str, bash_prefix: &mut Vec<String>, path_glob: &mut GlobSetBuilder) -> Result<()> {
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

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use mcc_config::PermissionConfig;

    fn cfg(allow: &[&str], deny: &[&str], mode: Option<&str>) -> PermissionConfig {
        PermissionConfig {
            mode: mode.map(|s| s.to_string()),
            allow: allow.iter().map(|s| s.to_string()).collect(),
            deny: deny.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn test_deny_overrides_allow() {
        let c = cfg(&["Bash(git:*)"], &["Bash(rm:*)"], None);
        let checker = PermissionChecker::new(&c).unwrap();

        // Allowed
        let d = checker.check(&PermissionRequest {
            category: "Bash".into(),
            action: Action::Bash {
                cmd: "git status".into(),
            },
        });
        assert!(matches!(d, Decision::Allow), "{d:?}");

        // Denied (deny overrides)
        let d = checker.check(&PermissionRequest {
            category: "Bash".into(),
            action: Action::Bash {
                cmd: "rm -rf /".into(),
            },
        });
        assert!(matches!(d, Decision::Deny(_)), "{d:?}");
    }

    #[test]
    fn test_bypass_permissions_mode() {
        let c = cfg(&[], &[], Some("bypassPermissions"));
        let checker = PermissionChecker::new(&c).unwrap();
        let d = checker.check(&PermissionRequest {
            category: "Bash".into(),
            action: Action::Bash {
                cmd: "anything".into(),
            },
        });
        assert!(matches!(d, Decision::Allow));
    }

    #[test]
    fn test_read_always_allowed_by_default() {
        let c = cfg(&[], &[], None);
        let checker = PermissionChecker::new(&c).unwrap();
        let d = checker.check(&PermissionRequest {
            category: "Read".into(),
            action: Action::Path {
                path: "/some/file.rs".into(),
            },
        });
        assert!(matches!(d, Decision::Allow));
    }

    #[test]
    fn test_write_asks_in_default_mode() {
        let c = cfg(&[], &[], None);
        let checker = PermissionChecker::new(&c).unwrap();
        let d = checker.check(&PermissionRequest {
            category: "Write".into(),
            action: Action::Path {
                path: "/some/file.rs".into(),
            },
        });
        assert!(matches!(d, Decision::Ask(_)));
    }

    #[test]
    fn test_write_allowed_in_accept_edits_mode() {
        let c = cfg(&[], &[], Some("acceptEdits"));
        let checker = PermissionChecker::new(&c).unwrap();
        let d = checker.check(&PermissionRequest {
            category: "Write".into(),
            action: Action::Path {
                path: "/any/file.rs".into(),
            },
        });
        assert!(matches!(d, Decision::Allow));
    }

    #[test]
    fn test_path_allow_glob() {
        let c = cfg(&["Read(src/**/*.rs)"], &[], None);
        let checker = PermissionChecker::new(&c).unwrap();

        let allow = checker.check(&PermissionRequest {
            category: "Read".into(),
            action: Action::Path {
                path: "src/main/mod.rs".into(),
            },
        });
        assert!(matches!(allow, Decision::Allow));
    }

    #[test]
    fn test_unknown_bash_asks_in_default_mode() {
        let c = cfg(&[], &[], None);
        let checker = PermissionChecker::new(&c).unwrap();
        let d = checker.check(&PermissionRequest {
            category: "Bash".into(),
            action: Action::Bash {
                cmd: "curl https://example.com".into(),
            },
        });
        assert!(matches!(d, Decision::Ask(_)));
    }

    /// Security regression: `Bash(rm:*)` must NOT match `rmdir`.
    /// Before the word-boundary fix, `"rmdir /tmp".starts_with("rm")` was true,
    /// meaning `rmdir` was blocked by the `rm` deny rule — and more dangerously,
    /// a future `Bash(curl:*)` allow rule would have granted `curl-evil` too.
    #[test]
    fn test_bash_deny_requires_word_boundary() {
        let c = cfg(&[], &["Bash(rm:*)"], None);
        let checker = PermissionChecker::new(&c).unwrap();

        // "rm" alone — denied
        let d = checker.check(&PermissionRequest {
            category: "Bash".into(),
            action: Action::Bash {
                cmd: "rm -rf /".into(),
            },
        });
        assert!(matches!(d, Decision::Deny(_)), "bare rm should be denied");

        // "rmdir" — must NOT be denied by the rm rule
        let d = checker.check(&PermissionRequest {
            category: "Bash".into(),
            action: Action::Bash {
                cmd: "rmdir /tmp/foo".into(),
            },
        });
        assert!(
            !matches!(d, Decision::Deny(_)),
            "rmdir should not match rm prefix rule"
        );
    }

    #[test]
    fn test_bash_allow_requires_word_boundary() {
        let c = cfg(&["Bash(git:*)"], &[], None);
        let checker = PermissionChecker::new(&c).unwrap();

        let d = checker.check(&PermissionRequest {
            category: "Bash".into(),
            action: Action::Bash {
                cmd: "git-upload-pack".into(),
            },
        });
        // "git-upload-pack" starts with "git" but has '-' after — not a word boundary
        assert!(
            !matches!(d, Decision::Allow),
            "git-upload-pack should not match git allow rule"
        );
    }
}
