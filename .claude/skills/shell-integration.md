# Shell Integration スキル

シェル統合スクリプト (OSC 133) の作成・更新を行う。

## 対象ファイル

- `shell-integration/minal.zsh`: Zsh 統合 (precmd/preexec フック)
- `shell-integration/minal.bash`: Bash 統合 (PROMPT_COMMAND フック)
- `shell-integration/minal.fish`: Fish 統合 (fish_prompt/fish_preexec)

## OSC 133 プロトコル

```
OSC 133;A ST  → プロンプト開始
OSC 133;B ST  → コマンド入力開始
OSC 133;C ST  → コマンド実行開始
OSC 133;D;{exit_code} ST  → コマンド終了
```

## Zsh テンプレート

```zsh
# Minal Shell Integration for Zsh
if [[ "$TERM_PROGRAM" != "minal" ]]; then
  return
fi

__minal_precmd() {
  local exit_code=$?
  # コマンド終了を通知
  printf '\e]133;D;%d\a' "$exit_code"
  # プロンプト開始を通知
  printf '\e]133;A\a'
}

__minal_preexec() {
  # コマンド実行開始を通知
  printf '\e]133;C\a'
}

precmd_functions+=(__minal_precmd)
preexec_functions+=(__minal_preexec)

# 初回プロンプト前に A を送信
printf '\e]133;A\a'
```

## ターミナル側の処理

- `crates/minal-core/src/handler.rs` の `osc_dispatch` で OSC 133 を解析
- `ShellIntegration` 構造体でプロンプト/コマンド/出力の状態追跡
- コマンド完了時に `CommandRecord` を生成 → AI コンテキストに自動追加
