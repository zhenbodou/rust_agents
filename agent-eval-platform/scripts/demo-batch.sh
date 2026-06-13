#!/usr/bin/env bash
# 提交一个演示批次（mock agent，无需 API key）。用法：./scripts/demo-batch.sh
set -euo pipefail

SERVER="${EVAL_SERVER_URL:-http://localhost:8080}"

BATCH=$(curl -sf "$SERVER/api/batches" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "demo-'"$(date +%H%M%S)"'",
    "profile": "mock-demo",
    "parallelism": 3,
    "idempotency_key": null,
    "cases": [
      {"case_id": "fix-div-by-zero",  "task": "修复 divide 函数的除零 panic"},
      {"case_id": "add-input-check",  "task": "给 login() 增加输入校验"},
      {"case_id": "broken-pipeline",  "task": "这个任务会 fail（演示失败 run）"},
      {"case_id": "refactor-parser",  "task": "重构 parser 模块，保持测试通过"},
      {"case_id": "flaky-test",       "task": "修复 fail 的不稳定测试（演示失败）"}
    ]
  }')

ID=$(echo "$BATCH" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)
echo "批次已提交: $ID"
echo "打开 http://localhost:5173/batches/$ID 观看实时执行"
