# Changelog

**[English](#english)** | **[日本語](#japanese)**

---

<a name="english"></a>
## English

### v1.0.7

- **Fixed**: mapping a key to `LSHIFT`/`RSHIFT`/`LCTRL`/`RCTRL`/`LALT`/`RALT` could be misread by games that read raw scan codes instead of virtual-key codes — confirmed on real hardware: `LSHIFT` arrived in Valorant as `RSHIFT`, and `RSHIFT` didn't register at all. These six keys now send the correct hardware scan code explicitly instead of relying on Windows' own (unreliable, for this specific left/right pair) translation. Every other key is unaffected.

### v1.0.6

- **Added**: `config.toml` hot-reload. A running `tartarus_driver` (normal or `tray` mode) now picks up changes saved via `configui` or made by hand within about a second — no restart needed. A missing or broken `config.toml` never resets you to hardcoded defaults; it keeps your last-known-good settings running and self-heals once the file is fixed. Verified on real hardware.
- **Changed**: the runtime log folder next to `tartarus_driver.exe` is now `logs\run.log` (was `tasks\run.log`) — same auto-created, auto-capped file, just a clearer name.
- **Added**: `configui`'s page now shows the running driver's version next to its title.
- **Added**: `research.md` — a write-up of the protocol reverse-engineering behind this driver (analog keys, the Synapse-free streaming handshake, RGB lighting, the still-unresolved profile-indicator LEDs) and the anti-cheat investigation from v1.0.5.1, for anyone curious how it works under the hood. Linked from README's "Documentation map"; not required reading to use the driver.

### v1.0.5.1

- **Documented**: anti-cheat-protected games may ignore this driver's input or refuse to launch while it's running (confirmed: Valorant/Vanguard works, Apex Legends/Easy Anti-Cheat does not, admin privileges don't change this) — added to README's "Known limitation" section and a new USAGE.md section, with an account-risk note. Docs-only patch release.
- **Documented (follow-up)**: no safe alternative was found either — switching to a virtual-controller-based output was tested and blocked the same as regular keyboard input, and hardware input adapters (Cronus Zen/Titan Two-class devices) aren't a safe substitute regardless of detection, since some publishers now ban that device class outright by policy (e.g. Apex Legends' Respawn, as of March 2026, permanently and without appeal). Added to the same README/USAGE sections.

### v1.0.5

- **Added**: Hyper Shift (the "Hyper Response" thumb button) is now fully configurable via `config.toml`'s new `[hypershift]` section (or `configui`'s new "Hyper Shift" panel). `mode` chooses between `"layer_switch"` (the button drives the analog keymap layers) or `"modifier_key"` (the button just sends a plain key, default Alt — no layer switching at all). When in `layer_switch` mode, `switch_style` chooses `"momentary"` (default, original behavior — held selects Layer1, released returns to Default) or `"toggle"` (every press advances one layer instead). `layer_count` (2 or 3, toggle mode only) adds a third layer, `[keys.layer2]`. The default config (`layer_switch` + `momentary`) behaves identically to every prior version.
- **Added**: 7 media/volume keys (Play/Pause, Stop, Next, Prev, Volume Up/Down/Mute) to the assignable key vocabulary, as their own "Media Control" category.
- **Changed**: `configui`'s key dropdowns are now a two-level picker — a small category selector ("Basic" / "Media Control") next to the key selector — instead of one long flat list, and the Analog Keys table gained a third "Layer2" column.
- **Changed**: `configui`'s "Hyper Shift" panel now sits above the Analog Keys table (it controls which of that table's columns are in play). Layer columns that aren't currently reachable given the selected mode/switch style/layer count (e.g. Layer2 while using momentary, or both Layer1/Layer2 in modifier-key mode) are shown hatched — still editable, just visually marked as inactive.
- **Fixed**: an already-held analog key would get force-released and immediately re-pressed under the new layer the instant Hyper Shift engaged (e.g. holding a key mapped to "1" on Default, then engaging Hyper Shift, would send "1" then immediately "6" if that key was "6" on Layer1) — both keys appeared in quick succession instead of a clean switch. The force-keyup-on-layer-change safety net now only fires on the transition back to Default (matching the original, hardware-verified momentary design), not on every transition.

### v1.0.4

- **Added**: a language switcher (English/日本語) to the `configui` web page, top right. Switching it re-translates the page immediately and saves right away, independent of the main "Save" button. The choice is remembered across restarts, in `config.toml`'s new `[configui]` section (defaults to English).
- **Added**: an `emulate` subcommand — a hardware-free debug mode for trying out the hysteresis/keymap/Hypershift-layer logic from a terminal, with no Tartarus Pro, Interception, or Razer control device needed. See `USAGE.md` for the command list.
- **Documented**: added `configui` screenshots to `README.md`.

### v1.0.3

- **Added**: a non-affiliation/trademark disclaimer to `README.md` ("Razer" and "Tartarus" are trademarks of Razer Inc.; this project is not affiliated with, endorsed by, or supported by Razer Inc.).

### v1.0.2

- **Fixed**: `config.toml` and `tasks/run.log` paths were resolved at compile time via `CARGO_MANIFEST_DIR`, which baked in the build machine's absolute path. A GitHub Actions-built release exe shipped with the CI runner's path hardcoded, so `config.toml` could never be found on a user's machine regardless of where the exe was placed. Both paths are now resolved at runtime relative to the running exe.
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

### v1.0.7

- **修正**: `LSHIFT`/`RSHIFT`/`LCTRL`/`RCTRL`/`LALT`/`RALT`を割り当てたキーが、仮想キーコードではなく生のスキャンコードを見るタイプのゲームで誤認識されることがあった問題を修正。実機で確認済み: `LSHIFT`を送るとValorant側では`RSHIFT`として認識され、`RSHIFT`は全く認識されなかった。この6キーだけ、Windows側の変換に頼らず正しいハードウェアスキャンコードを明示的に送信するように変更。それ以外のキーへの影響はない。

### v1.0.6

- **追加**: `config.toml`のホットリロード。動作中の`tartarus_driver`(通常起動/`tray`モードとも)は、`configui`での保存や手動編集による変更を約1秒以内に自動で反映する(再起動不要)。`config.toml`が見つからない・壊れている場合でもハードコードされた既定値には戻らず、直前の正常な設定のまま動作を継続し、ファイルが修正され次第自動的に復帰する。実機検証済み。
- **変更**: `tartarus_driver.exe`の隣に作られる実行時ログのフォルダ名を`tasks\run.log`から`logs\run.log`に変更(中身・自動生成・自動キャップの挙動はそのまま、名前だけ分かりやすく)。
- **追加**: `configui`のページにも、動作中のドライバのバージョンをタイトル横に表示するようにした。
- **追加**: `research.md` — 本ドライバのプロトコルリバースエンジニアリング記録(アナログキー・Synapse不要のストリーミング有効化ハンドシェイク・RGBライティング・未解決のプロファイルインジケータLED)と、v1.0.5.1のアンチチート調査をまとめたドキュメント。内部の仕組みに興味がある方向け。READMEの「ドキュメント構成」からリンク、使うだけなら読む必要はない。

### v1.0.5.1

- **ドキュメント追加**: アンチチート導入済みのゲームでは本ドライバの入力が無視される、またはゲーム自体が起動しないことがある旨を明記(実機確認: Valorant/Vanguardは動作するが、Apex Legends/Easy Anti-Cheatは動作せず、管理者権限でも変わらない)。README「既知の制約」節とUSAGE.mdの新設セクションに、アカウント停止リスクの注意とあわせて追記。ドキュメントのみのパッチリリース。
- **ドキュメント追加(続報)**: 安全な代替策も見つからなかった — 仮想コントローラー方式の出力もテストしたが通常のキーボード入力と同様にブロックされ、ハードウェア入力アダプタ(Cronus Zen/Titan Two等)も検知の有無にかかわらず安全な代替にはならない(一部パブリッシャーはこの種のデバイスを検知有無を問わずポリシーとして禁止しており、例えばApex LegendsはRespawnが2026年3月より恒久・異議申し立て不可でBANすると明言)。同じREADME/USAGEの節に追記。

### v1.0.5

- **追加**: ハイパーシフト(「Hyper Response」サムボタン)の動作を`config.toml`の新セクション`[hypershift]`(または`configui`の新しい「ハイパーシフト」パネル)から設定できるようにした。`mode`で`"layer_switch"`(ボタンがアナログキーのレイヤーを切り替える)か`"modifier_key"`(ボタンは単に既定Altなどのキーを送るだけで、レイアウト切替は一切行わない)を選べる。`layer_switch`モードでは、さらに`switch_style`で`"momentary"`(既定・従来通りの挙動 — 押している間はLayer1、離すと通常レイヤーに戻る)か`"toggle"`(押すたびに1レイヤーずつ進む)を選べる。`layer_count`(2または3、トグル時のみ)で第3レイヤー`[keys.layer2]`も使えるようになる。既定設定(`layer_switch`+`momentary`)は従来バージョンと完全に同一の挙動。
- **追加**: 割り当て可能なキーの語彙に、メディア/音量キー7種(再生/一時停止・停止・次へ・前へ・音量上げ/下げ/ミュート)を「メディア操作」カテゴリとして追加。
- **変更**: `configui`のキー選択が、1つの長いフラットな一覧から、「基本」/「メディア操作」のカテゴリ選択+実際のキー選択という2段構成のピッカーに変更。アナログキー表にも「Layer2」列を追加。
- **変更**: `configui`の「ハイパーシフト」パネルを「アナログキー」表より上に配置(この表のどの列が使われるかを制御するパネルのため)。選択中のモード/切替方式/レイヤー数では到達できないレイヤー列(例: モーメンタリ時のLayer2、修飾キーモード時のLayer1・Layer2両方)は、グレー+斜線ハッチング表示になる(編集は引き続き可能、見た目だけ非アクティブを示す)。
- **修正**: 既に押している最中のアナログキーが、Hyper Shiftが作動した瞬間に強制解放されて新しいレイヤーの割り当てで即座に再押下されてしまい(例: Default側で"1"に割り当てたキーを押しっぱなしのままHyper Shiftを作動させると、"1"の直後に"6"(Layer1側の割り当て)も送信されてしまう)、切り替わりのはずが両方のキーが立て続けに入力される不具合を修正。レイヤー変化時の強制KeyUp安全策は、通常レイヤーへ戻る遷移でのみ発火するよう戻した(元のモーメンタリ設計と同じ挙動)。

### v1.0.4

- **追加**: `configui`のWebページ右上に言語切り替え(English/日本語)を追加。切り替えるとその場でページ全体が翻訳され、下の「保存」ボタンとは独立して即座に保存される。選択した言語は`config.toml`の新セクション`[configui]`に記録され、次回起動時も覚えている(既定は英語)。
- **追加**: `emulate`サブコマンド — Tartarus Pro本体・Interception・Razerコントロールデバイスを一切使わず、ターミナルからヒステリシス判定・キーマップ・Hypershiftレイヤー切替のロジックを試せるデバッグ用モード。コマンド一覧は`USAGE.md`参照。
- **ドキュメント追加**: `README.md`に`configui`のスクリーンショットを追加。

### v1.0.3

- **追加**: `README.md`にRazer社との無関係・商標に関する免責事項を追加(「Razer」「Tartarus」はRazer社の商標であり、本プロジェクトはRazer社とは無関係・非公認・非支援であることを明記)。

### v1.0.2

- **修正**: `config.toml`と`tasks/run.log`のパスが、コンパイル時に`CARGO_MANIFEST_DIR`で解決されており、ビルドしたマシンの絶対パスが埋め込まれていた。GitHub Actionsでビルドしたリリース版exeにはCIランナーのパスが固定されてしまい、ユーザーのPCではどこに置いても`config.toml`が見つからなかった。両方とも、実行中のexeの場所を基準に、実行時に解決するよう変更。
- **修正**: ログ出力の性能問題。アナログキーの押下/解放、十字キー・ホイールのイベントのたびに、その入力をできるだけ速く転送する処理を担当している同じスレッド上で、同期的なロック付きファイル書き込みが発生していた。ログ出力はチャネル経由のバックグラウンドスレッド方式に変更し、どれだけ速くキーを押しても、キー入力・十字キー/ホイールのリマップ・Hypershiftの反応速度に影響しなくなった。`tasks/run.log`自体も約5MiBで頭出しして書き続ける方式にし(`tray`モードで何日も動かし続けても肥大化しない)。
- **ドキュメント修正**: カーネルドライバのインストール(`install-interception.exe /install`)が成功しても、`interception.dll`自体が見つかるとは限らない。これはライセンス上の理由でこのプロジェクトが同梱していない別のユーザーランドDLLのため。README「既知の制約」に手動コピー手順を追記し、使用しているInterceptionのバージョン(v1.0.1)も明記。
- **追加**: 起動のたびに、最初の行として`tartarus_driver vX.Y.Z`を出力・ログ記録するようにした。動いているバージョンをいつでも確認できる。
- **追加**: `run-tray.bat` — trayモードをダブルクリックで起動するランチャー。exe自体をダブルクリックすると通常(コンソール)モードで起動するため。
- **追加**: タスクトレイの右クリックメニューに、動いているバージョンの表示と、公開リポジトリのReleasesページを開くだけの「アップデートを確認」項目を追加(自動チェックやバックグラウンド通信は一切なし)。
- **ドキュメント追加**: `tartarus_driver.exe`の隣に何が必要か(`config.toml`、`tasks/run.log`、`interception.dll`など)を示すフォルダ構成図を`USAGE.md`に追加。

### v1.0.1

初回公開リリース。
