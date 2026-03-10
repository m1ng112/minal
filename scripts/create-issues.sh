#!/bin/bash
# Minal - GitHub Issues 一括作成スクリプト
# 使い方:
#   1. gh auth login でGitHub CLIにログイン
#   2. ./scripts/create-issues.sh を実行
#
# または GITHUB_TOKEN 環境変数を設定:
#   GITHUB_TOKEN=ghp_xxx ./scripts/create-issues.sh

set -euo pipefail

REPO="m1ng112/minal"
ISSUES_FILE="$(dirname "$0")/../issues.json"

if ! command -v gh &> /dev/null && [ -z "${GITHUB_TOKEN:-}" ]; then
    echo "Error: gh CLI が見つかりません。'gh auth login' でログインしてください。"
    echo "または GITHUB_TOKEN 環境変数を設定してください。"
    exit 1
fi

if ! command -v jq &> /dev/null; then
    echo "Error: jq が必要です。'brew install jq' or 'apt install jq' でインストールしてください。"
    exit 1
fi

TOTAL=$(jq length "$ISSUES_FILE")
echo "=== Minal GitHub Issues 作成 ==="
echo "合計: $TOTAL issues"
echo ""

# ラベルを事前作成
echo "--- ラベルを作成中 ---"
LABELS=(
    "phase-1:0E8A16:Phase 1: MVP"
    "phase-2:1D76DB:Phase 2: ターミナル機能充実"
    "phase-3:D93F0B:Phase 3: AI 統合"
    "phase-4:5319E7:Phase 4: 磨き込み"
    "priority-critical:B60205:Critical priority"
    "priority-high:D93F0B:High priority"
    "priority-medium:FBCA04:Medium priority"
    "priority-low:0E8A16:Low priority"
    "minal-core:C2E0C6:minal-core crate"
    "minal-renderer:C5DEF5:minal-renderer crate"
    "minal-ai:E99695:minal-ai crate"
    "minal-config:FEF2C0:minal-config crate"
    "minal-app:D4C5F9:Main application"
    "infrastructure:BFDADC:Infrastructure & CI/CD"
    "shell-integration:F9D0C4:Shell integration"
    "platform-macos:000000:macOS specific"
    "performance:006B75:Performance"
    "accessibility:0075CA:Accessibility"
    "architecture:5319E7:Architecture"
    "distribution:FBCA04:Distribution"
)

for label_info in "${LABELS[@]}"; do
    IFS=: read -r name color description <<< "$label_info"
    gh label create "$name" --color "$color" --description "$description" --repo "$REPO" 2>/dev/null || true
done
echo "ラベル作成完了"
echo ""

# Issues を作成
echo "--- Issues を作成中 ---"
for i in $(seq 0 $((TOTAL - 1))); do
    TITLE=$(jq -r ".[$i].title" "$ISSUES_FILE")
    BODY=$(jq -r ".[$i].body" "$ISSUES_FILE")
    LABELS_JSON=$(jq -r ".[$i].labels | join(\",\")" "$ISSUES_FILE")

    echo "[$((i + 1))/$TOTAL] $TITLE"

    if [ -n "${GITHUB_TOKEN:-}" ]; then
        # Use API directly
        LABELS_ARRAY=$(jq -c ".[$i].labels" "$ISSUES_FILE")
        curl -s -X POST "https://api.github.com/repos/$REPO/issues" \
            -H "Authorization: token $GITHUB_TOKEN" \
            -H "Content-Type: application/json" \
            -d "$(jq -n --arg title "$TITLE" --arg body "$BODY" --argjson labels "$LABELS_ARRAY" \
                '{title: $title, body: $body, labels: $labels}')" > /dev/null
    else
        # Use gh CLI
        gh issue create \
            --repo "$REPO" \
            --title "$TITLE" \
            --body "$BODY" \
            --label "$LABELS_JSON" 2>/dev/null
    fi

    # Rate limit 対策
    sleep 1
done

echo ""
echo "=== 完了: $TOTAL issues を作成しました ==="
echo "確認: https://github.com/$REPO/issues"
