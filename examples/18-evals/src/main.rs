//! 第 18 章：Eval Runner（离线可运行骨架 —— 布置 fixture、执行 checker）。
//! 真实场景会接入 Agent；这里演示数据结构、fixture 铺设、checker 与报告。

use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct EvalCase {
    pub id: String,
    pub input: String,
    #[serde(default)]
    pub fixtures: Fixtures,
    #[serde(default)]
    pub expectations: Vec<Expectation>,
    #[serde(default)]
    pub budget: Budget,
}

#[derive(Debug, Deserialize, Default)]
pub struct Fixtures {
    #[serde(default)]
    pub files: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Expectation {
    Bash { cmd: String, must_succeed: bool },
    ContainsFile { path: String, substring: String },
    NotContainsFile { path: String, substring: String },
}

#[derive(Debug, Deserialize)]
pub struct Budget {
    pub max_iterations: u32,
    pub max_usd: f64,
}

impl Default for Budget {
    fn default() -> Self {
        Self { max_iterations: 20, max_usd: 0.5 }
    }
}

#[derive(Debug)]
pub struct EvalResult {
    pub case_id: String,
    pub passed: bool,
    pub failures: Vec<String>,
}

pub async fn setup_fixtures(root: &Path, f: &Fixtures) -> Result<()> {
    for (rel, content) in &f.files {
        let full = root.join(rel);
        if let Some(dir) = full.parent() {
            tokio::fs::create_dir_all(dir).await?;
        }
        tokio::fs::write(full, content).await?;
    }
    Ok(())
}

pub async fn check(
    exp: &Expectation,
    cwd: &Path,
) -> Result<()> {
    match exp {
        Expectation::Bash { cmd, must_succeed } => {
            let out = tokio::process::Command::new("bash")
                .arg("-c")
                .arg(cmd)
                .current_dir(cwd)
                .output()
                .await?;
            if *must_succeed && !out.status.success() {
                anyhow::bail!(
                    "bash failed: {}\n{}",
                    cmd,
                    String::from_utf8_lossy(&out.stderr)
                );
            }
            Ok(())
        }
        Expectation::ContainsFile { path, substring } => {
            let body = tokio::fs::read_to_string(cwd.join(path)).await?;
            if !body.contains(substring) {
                anyhow::bail!("{} missing substring `{}`", path, substring);
            }
            Ok(())
        }
        Expectation::NotContainsFile { path, substring } => {
            let body = tokio::fs::read_to_string(cwd.join(path))
                .await
                .unwrap_or_default();
            if body.contains(substring) {
                anyhow::bail!("{} still contains `{}`", path, substring);
            }
            Ok(())
        }
    }
}

pub fn print_report(results: &[EvalResult]) {
    let pass = results.iter().filter(|r| r.passed).count();
    println!("===== EVAL REPORT =====");
    println!("passed: {}/{}", pass, results.len());
    for r in results {
        let status = if r.passed { "PASS" } else { "FAIL" };
        println!("[{status}] {}", r.case_id);
        for f in &r.failures {
            println!("    - {f}");
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let suite_yaml = r#"
- id: file-exists-after-touch
  input: "irrelevant"
  fixtures:
    files:
      "README.md": "hello\n"
  expectations:
    - type: bash
      cmd: "touch newfile.txt"
      must_succeed: true
    - type: contains_file
      path: "README.md"
      substring: "hello"
    - type: not_contains_file
      path: "README.md"
      substring: "ERROR"
  budget:
    max_iterations: 5
    max_usd: 0.0
"#;

    let cases: Vec<EvalCase> = serde_yaml::from_str(suite_yaml)?;
    let mut results = Vec::new();

    for case in &cases {
        let tmp = tempfile::tempdir()?;
        setup_fixtures(tmp.path(), &case.fixtures).await?;

        let mut failures = Vec::new();
        for exp in &case.expectations {
            if let Err(e) = check(exp, tmp.path()).await {
                failures.push(e.to_string());
            }
        }

        results.push(EvalResult {
            case_id: case.id.clone(),
            passed: failures.is_empty(),
            failures,
        });
    }

    print_report(&results);
    Ok(())
}
