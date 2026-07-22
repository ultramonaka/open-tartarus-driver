# Changelog

**[English](#english)** | **[日本語](#japanese)**

---

<a name="english"></a>
## English

### v1.0.4

- **Added**: a language switcher (English/日本語) to the `configui` web page, top right. Switching it re-translates the page immediately and saves right away, independent of the main "Save" button. The choice is remembered across restarts, in `config.toml`'s new `[configui]` section (defaults to English).
- **Added**: an `emulate` subcommand — a hardware-free debug mode for trying out the hysteresis/keymap/Hypershift-layer logic from a terminal, with no Tartarus Pro, Interception, or Razer control device needed. See `USAGE.md` for the command list.
- **Documented**: added `configui` screenshots to `README.md`; refreshed `Purpose.md`'s architecture writeup (thread/process layout, and every feature added since the last update) to match the current codebase.
- Internal cleanup (`config.rs`, `main.rs`, `dpad.rs`, `configui.rs`, `lighting.rs`): removed several duplicated code blocks with no behavior change, and added unit test coverage for `emulate.rs`'s command parsing/dispatch.

### v1.0.3

- **Added**: a non-affiliation/trademark disclaimer to `README.md` ("Razer" and "Tartarus" are trademarks of Razer Inc.; this project is not affiliated with, endorsed by, or supported by Razer Inc.).

### v1.0.2

- **Fixed**: `config.toml` and `tasks/run.log` paths were resolved at compile time via `CARGO_MANIFEST_DIR`, which baked in the build machine's absolute path. A GitHub Actions-built release exe shipped with the CI runner's path hardcoded, so `config.toml` could never be found on a user's machine regardless of where the exe was placed. Both paths are now resolved at runtime relative to the running exe (or the repo root, for a dev build).
- **Fixed**: logging performance. Every analog key press/release and every D-pad/wheel event was doing a synchronous, locked file write on the same thread responsible for forwarding that same input as fast as possible. Logging now goes through a background thread via a channel — it can never add latency to key presses, D-pad/wheel remapping, or Hypershift, no matter how fast you press. `tasks/run.log` is also capped at ~5 MiB (it truncates and keeps going) since `tray` mode is meant to run for days at a time.
- **Documented**: the kernel driver installing successfully (`install-interception.exe /install`) doesn't mean `interception.dll` itself can be found — that's a separate userland DLL this project deliberately doesn't bundle (a licensing choice). Added the missing manual copy step to README's "Known limitation" section, and noted the exact Interception version (v1.0.1) this project links against.
- **Added**: `tartarus_driver vX.Y.Z` is now printed (and logged) as the first line at every startup, so the running version is always confirmable.
- **Added**: `run-tray.bat` — a double-clickable launcher for `tray` mode, since double-clicking the exe itself starts normal (console) mode.
- **Added**: the tray icon's right-click menu now shows the running version and a "Check for updates" item that opens the public repo's Releases page (just a URL shortcut — no automatic check, no background network calls).
- **Documented**: a folder-layout diagram in `USAGE.md` showing exactly what belongs next to `tartarus_driver.exe` (`config.toml`, `tasks/run.log`, `interception.dll`, etc.).

### v1.0.1

Initial public release.

---

<a name="japanese"></a>
## 日本語

### v1.0.4

- **追加**: `configui`のWebページ右上に言語切り替え(English/日本語)を追加。切り替えるとその場でページ全体が翻訳され、下の「保存」ボタンとは独立して即座に保存される。選択した言語は`config.toml`の新セクション`[configui]`に記録され、次回起動時も覚えている(既定は英語)。
- **追加**: `emulate`サブコマンド — Tartarus Pro本体・Interception・Razerコントロールデバイスを一切使わず、ターミナルからヒステリシス判定・キーマップ・Hypershiftレイヤー切替のロジックを試せるデバッグ用モード。コマンド一覧は`USAGE.md`参照。
- **ドキュメント追加**: `README.md`に`configui`のスクリーンショットを追加。`Purpose.md`のアーキテクチャ記述(スレッド/プロセス構成、前回更新以降に追加した各機能)を現在のコードに合わせて全面的に最新化。
- 内部整理(`config.rs`/`main.rs`/`dpad.rs`/`configui.rs`/`lighting.rs`): 挙動に影響しない重複コードの整理をいくつか実施し、`emulate.rs`のコマンド解析・振り分けにユニットテストを追加。

### v1.0.3

- **追加**: `README.md`にRazer社との無関係・商標に関する免責事項を追加(「Razer」「Tartarus」はRazer社の商標であり、本プロジェクトはRazer社とは無関係・非公認・非支援であることを明記)。

### v1.0.2

- **修正**: `config.toml`と`tasks/run.log`のパスが、コンパイル時に`CARGO_MANIFEST_DIR`で解決されており、ビルドしたマシンの絶対パスが埋め込まれていた。GitHub Actionsでビルドしたリリース版exeにはCIランナーのパスが固定されてしまい、ユーザーのPCではどこに置いても`config.toml`が見つからなかった。両方とも、実行中のexeの場所(開発ビルドの場合はリポジトリルート)を基準に、実行時に解決するよう変更。
- **修正**: ログ出力の性能問題。アナログキーの押下/解放、十字キー・ホイールのイベントのたびに、その入力をできるだけ速く転送する処理を担当している同じスレッド上で、同期的なロック付きファイル書き込みが発生していた。ログ出力はチャネル経由のバックグラウンドスレッド方式に変更し、どれだけ速くキーを押しても、キー入力・十字キー/ホイールのリマップ・Hypershiftの反応速度に影響しなくなった。`tasks/run.log`自体も約5MiBで頭出しして書き続ける方式にし(`tray`モードで何日も動かし続けても肥大化しない)。
- **ドキュメント修正**: カーネルドライバのインストール(`install-interception.exe /install`)が成功しても、`interception.dll`自体が見つかるとは限らない。これはライセンス上の理由でこのプロジェクトが同梱していない別のユーザーランドDLLのため。README「既知の制約」に手動コピー手順を追記し、使用しているInterceptionのバージョン(v1.0.1)も明記。
- **追加**: 起動のたびに、最初の行として`tartarus_driver vX.Y.Z`を出力・ログ記録するようにした。動いているバージョンをいつでも確認できる。
- **追加**: `run-tray.bat` — trayモードをダブルクリックで起動するランチャー。exe自体をダブルクリックすると通常(コンソール)モードで起動するため。
- **追加**: タスクトレイの右クリックメニューに、動いているバージョンの表示と、公開リポジトリのReleasesページを開くだけの「アップデートを確認」項目を追加(自動チェックやバックグラウンド通信は一切なし)。
- **ドキュメント追加**: `tartarus_driver.exe`の隣に何が必要か(`config.toml`、`tasks/run.log`、`interception.dll`など)を示すフォルダ構成図を`USAGE.md`に追加。

### v1.0.1

初回公開リリース。
