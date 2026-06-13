//! 领域类型。TraceEvent 与 schemas/trace-event.schema.json 对齐（schema_version = 1）。
//! 生产做法是从 JSON Schema 代码生成（typify）；教学版手写并以测试钉住一致性。

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const TRACE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TraceEventBody {
    RunStarted {
        run_id: Uuid,
        case_id: String,
        task: String,
        #[serde(default)]
        model: Option<String>,
    },
    LlmRequest {
        turn: u32,
        #[serde(default)]
        input_tokens: u64,
    },
    LlmChunk {
        turn: u32,
        delta: String,
    },
    ToolCall {
        turn: u32,
        call_id: String,
        tool_name: String,
        #[serde(default)]
        args: serde_json::Value,
    },
    ToolResult {
        call_id: String,
        output: String,
        #[serde(default)]
        is_error: bool,
        #[serde(default)]
        duration_ms: u64,
    },
    RunFinished {
        status: RunOutcome,
        #[serde(default)]
        score: Option<f64>,
        #[serde(default)]
        cost_usd: Option<f64>,
        #[serde(default)]
        turns: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEvent {
    pub schema_version: u32,
    pub seq: u64,
    pub ts: f64,
    #[serde(flatten)]
    pub body: TraceEventBody,
}

impl TraceEvent {
    pub fn validate_for_run(&self, run_id: Uuid) -> Result<(), String> {
        if self.schema_version != TRACE_SCHEMA_VERSION {
            return Err(format!(
                "schema_version must be {TRACE_SCHEMA_VERSION}, got {}",
                self.schema_version
            ));
        }
        if let TraceEventBody::RunStarted {
            run_id: event_run_id,
            ..
        } = &self.body
        {
            if *event_run_id != run_id {
                return Err(format!(
                    "event run_id {event_run_id} does not match request run_id {run_id}"
                ));
            }
        }
        Ok(())
    }

    /// 批次页只看里程碑事件（ch50 的降采样扇出）
    pub fn is_milestone(&self) -> bool {
        matches!(
            self.body,
            TraceEventBody::RunStarted { .. } | TraceEventBody::RunFinished { .. }
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunOutcome {
    Passed,
    Failed,
    Error,
    Timeout,
}

impl RunOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Error => "error",
            Self::Timeout => "timeout",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_against_schema_fixture() {
        let line = r#"{"schema_version":1,"seq":3,"ts":1760000000.5,"type":"tool_call","turn":1,"call_id":"c1","tool_name":"bash","args":{"command":"ls"}}"#;
        let ev: TraceEvent = serde_json::from_str(line).unwrap();
        assert!(
            matches!(ev.body, TraceEventBody::ToolCall { ref tool_name, .. } if tool_name == "bash")
        );
        let back = serde_json::to_value(&ev).unwrap();
        assert_eq!(back["type"], "tool_call");
        assert_eq!(back["schema_version"], 1);
        assert!(ev.validate_for_run(Uuid::new_v4()).is_ok());
    }

    #[test]
    fn unknown_event_type_is_rejected() {
        // 前端会优雅降级渲染未知事件；后端作为事实源必须严格拒收
        let line = r#"{"schema_version":1,"seq":0,"ts":0,"type":"mystery"}"#;
        assert!(serde_json::from_str::<TraceEvent>(line).is_err());
    }

    #[test]
    fn schema_version_is_enforced() {
        let run_id = Uuid::new_v4();
        let line = format!(
            r#"{{"schema_version":2,"seq":0,"ts":0,"type":"run_started","run_id":"{run_id}","case_id":"c","task":"t"}}"#
        );
        let ev: TraceEvent = serde_json::from_str(&line).unwrap();
        assert!(ev.validate_for_run(run_id).is_err());
    }

    #[test]
    fn run_started_id_must_match_request_id() {
        let request_run_id = Uuid::new_v4();
        let event_run_id = Uuid::new_v4();
        let line = format!(
            r#"{{"schema_version":1,"seq":0,"ts":0,"type":"run_started","run_id":"{event_run_id}","case_id":"c","task":"t"}}"#
        );
        let ev: TraceEvent = serde_json::from_str(&line).unwrap();
        assert!(ev.validate_for_run(request_run_id).is_err());
    }
}
