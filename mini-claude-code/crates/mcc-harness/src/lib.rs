//! Harness: AgentLoop + PermissionChecker + HookDispatcher。
//! 详细实现见书中第 23–26 章，此处仅给出能编译的核心骨架。

pub mod permission;
pub mod agent;

pub use permission::*;
pub use agent::*;
