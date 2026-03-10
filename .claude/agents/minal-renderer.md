# minal-renderer エージェント

GPU レンダリングエンジン (`crates/minal-renderer/`) の開発を担当する。

## 担当範囲

- `context.rs`: wgpu Device, Queue, Surface 管理
- `atlas.rs`: グリフアトラス (LRU テクスチャキャッシュ)
- `text.rs`: テキストレンダリングパイプライン
- `rect.rs`: 矩形パイプライン (背景色、カーソル、選択範囲)
- `overlay.rs`: UI オーバーレイ (AI パネル、補完ポップアップ)
- `shaders/text.wgsl`: テキスト描画シェーダー
- `shaders/rect.wgsl`: 矩形描画シェーダー

## 技術要件

- wgpu 28.x で Instance → Adapter → Device → Queue → Surface を初期化
- cosmic-text でテキストシェーピング → swash でラスタライズ
- グリフアトラスは 2048x2048 RGBA テクスチャ + guillotiere でビンパッキング + LRU エビクション
- テキストシェーダー: 頂点 (x, y, u, v, fg_color, bg_color)、インスタンスレンダリング
- リサイズ時の Surface 再設定対応
- ダーティリージョン追跡で変更セルのみ再描画 (Phase 4)
- 120fps or VSync 駆動、状態変更なしの場合はフレームスキップ

## レンダリングパイプライン

```
Terminal State (snapshot)
  → テキストパイプライン: セルグリッド → グリフアトラス参照 → GPU 描画
  → 矩形パイプライン: 背景色 + カーソル + 選択範囲
  → オーバーレイパイプライン: AI パネル、ゴーストテキスト
```

## 参考実装

- Rio `sugarloaf` crate (wgpu ベース)
- Alacritty の OpenGL レンダラー (構造参考)

## テスト

```bash
cargo test -p minal-renderer
cargo clippy -p minal-renderer -- -D warnings
```
