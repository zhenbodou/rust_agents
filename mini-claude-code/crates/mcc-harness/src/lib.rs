//! mcc-harness：企业级 AgentLoop + PermissionChecker。
//! 完整实现见书第 23–26 章。

pub mod agent;
pub mod permission;

pub use agent::{AgentLoop, AgentLoopBuilder, AgentRun};
pub use permission::{Action, Decision, PermissionChecker, PermissionRequest};
