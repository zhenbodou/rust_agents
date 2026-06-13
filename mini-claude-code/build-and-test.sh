#!/usr/bin/env bash
# mini-claude-code 企业级验证脚本
# 使用方式：在 rust_agents/ 根目录运行
#   cd /path/to/rust_agents
#   bash mini-claude-code/build-and-test.sh

set -euo pipefail
cd "$(dirname "$0")/.."   # 回到 workspace 根目录

echo "═══════════════════════════════════════════════════════"
echo " mini-claude-code 企业级编译 + 测试验证"
echo "═══════════════════════════════════════════════════════"

# ── 1. 编译检查（所有 mcc-* crate）────────────────────────
echo ""
echo "▶ [1/4] 编译 mcc-* 所有 crate（debug 模式）..."
cargo build \
  -p mcc-core \
  -p mcc-config \
  -p mcc-session \
  -p mcc-tools \
  -p mcc-llm \
  -p mcc-harness \
  -p mcc-tui \
  -p mcc-cli \
  2>&1

echo "✓ 编译成功"

# ── 2. 单元测试 ────────────────────────────────────────────
echo ""
echo "▶ [2/4] 运行单元测试..."
cargo test \
  -p mcc-tools \
  -p mcc-harness \
  -- --nocapture \
  2>&1

echo "✓ 单元测试通过"

# ── 3. Release 编译 ────────────────────────────────────────
echo ""
echo "▶ [3/4] Release 编译..."
cargo build --release -p mcc-cli 2>&1
echo "✓ Release 编译成功"

BINARY=target/release/mcc

# ── 4. 端到端冒烟测试 ──────────────────────────────────────
echo ""
echo "▶ [4/4] 端到端冒烟测试..."

# 4a. 版本
echo ""
echo "  mcc version:"
$BINARY version

# 4b. 配置打印
echo ""
echo "  mcc config (首次 = 默认值):"
$BINARY config | python3 -c "import json,sys; d=json.load(sys.stdin); \
  print(f'  model.main   = {d[\"model\"][\"main\"]}'); \
  print(f'  budget.max_iterations = {d[\"budget\"][\"max_iterations\"]}')"

# 4c. headless + 工具调用（需要 ANTHROPIC_API_KEY）
if [[ -n "${ANTHROPIC_API_KEY:-}" ]]; then
  echo ""
  echo "  mcc -p 'list current directory' (headless + real LLM):"
  $BINARY -p "list the files in the current directory and return only the count" \
    --quiet --cwd "$(pwd)/mini-claude-code" \
    2>/dev/null | head -5
  echo "  ✓ headless 模式响应正常"
else
  echo ""
  echo "  [跳过] ANTHROPIC_API_KEY 未设置，跳过真实 LLM 调用测试"
  echo "  设置 ANTHROPIC_API_KEY 后重新运行可完整验证"
fi

echo ""
echo "═══════════════════════════════════════════════════════"
echo " ✅ 全部验证通过！"
echo "   二进制路径: $(pwd)/$BINARY"
echo "   运行方式:   $BINARY --help"
echo "═══════════════════════════════════════════════════════"
