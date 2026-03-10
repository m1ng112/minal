# minal-app エージェント

メインアプリケーション (`src/`) の開発を担当する。

## 担当範囲

- `main.rs`: エントリーポイント (設定読込 → App::run())
- `app.rs`: メインイベントループ (winit EventLoop)
- `event.rs`: イベント型定義 + ディスパッチ
- `window.rs`: winit Window ラッパー + macOS ネイティブ統合

## 技術要件

### 3スレッドアーキテクチャ (Ghostty 参考)

**Main Thread (winit EventLoop)**:
- winit `EventLoop::run()` をメインスレッドで実行
- キーボード/マウスイベントを I/O スレッドへ crossbeam channel で転送
- ウィンドウリサイズ → レンダラー + PTY (TIOCSWINSZ) に通知
- タブ/ペイン管理

**I/O Thread (tokio Runtime)**:
- `std::thread::spawn` → tokio Runtime 構築
- PTY master fd を `tokio::io::AsyncFd` で監視
- PTY 読取 → vte パース → Terminal State 更新
- AI 非同期リクエスト処理

**Renderer Thread (wgpu)**:
- `std::thread::spawn` で起動
- 120fps / VSync 駆動
- Terminal State snapshot → wgpu 描画
- ダーティフラグでフレームスキップ

### スレッド間通信

```
Main → I/O: KeyEvent, Resize (crossbeam channel)
Main → Renderer: Resize (crossbeam channel)
I/O → Renderer: Redraw, AiResult (crossbeam channel)
共有: Arc<Mutex<TerminalState>> → 将来 double-buffering
```

### イベント型

- WindowEvent (resize, focus, close)
- KeyEvent → PTY 書込 or AI トリガー
- PtyEvent (output ready)
- AiEvent (completion ready, chat response)

## キーバインド (デフォルト)

- `Ctrl+Shift+A`: AI Chat パネル Toggle
- `Ctrl+Shift+E`: Error Summary パネル Toggle
- `Tab` (on ghost text): AI 補完を確定 → PTY write

## テスト

```bash
cargo test
cargo clippy -- -D warnings
```
