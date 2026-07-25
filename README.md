# Tartarus Pro Standalone Driver

**[English](#english)** | **[日本語](#japanese)**

[![License: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-blue.svg)](LICENSE)
![Version](https://img.shields.io/badge/version-1.0.5.1-informational.svg)
![Author](https://img.shields.io/badge/author-ultramonaka-lightgrey.svg)

---

<a name="english"></a>
## English

**`tartarus_driver`** is a from-scratch Rust driver for the Razer Tartarus Pro (the left-hand analog gaming keypad) that runs on Windows **without Razer Synapse at all** — no background service, no telemetry, no vendor software required.

> **Disclaimer**: This is an independent, community-developed project, not affiliated with, endorsed by, or supported by Razer Inc. "Razer" and "Tartarus" are trademarks of Razer Inc. Provided as-is, with no warranty (see the [License](#license) section).

### Features

- **Full analog key support** — reads the raw 0-255 depth of all 20 keys directly over HID and converts it to keystrokes with hysteresis-based actuation (no chattering)
- **Fully remappable, no recompiling** — every key, the D-pad, the wheel, and middle-click can be reassigned from a browser-based config page (`configui`), including a **live sensitivity calibration view**, **per-key actuation thresholds**, and **media/volume keys** as a separate picker category
- **D-pad / wheel / middle-click remap** at the kernel level (via [Interception](https://github.com/oblitum/Interception)) — a real keyboard/mouse plugged in at the same time is never affected
- **Hyper Shift**: the "Hyper Response" thumb button, fully configurable — either a layer-switch trigger (momentary hold, or toggle cycling through 2-3 layers) or a plain modifier key passthrough (default Alt), with no side effects on a real keyboard's Alt+Tab
- **LED lighting control** — static color, breathing, spectrum, wave, and reactive effects
- **Runs in the background indefinitely**, optionally from a system tray icon (`tray` mode) with no console window

### Screenshots

`configui`, the browser-based config page — remap every analog key across up to 3 layers (Default/Layer1/Layer2), configure Hyper Shift's mode, and pick media/volume keys from their own category:

![configui: key remap settings](img/configgui_1.png)

Live sensitivity calibration — watch each key's raw depth in real time while dialing in per-key `t_on`/`t_off` overrides:

![configui: live calibration](img/configgui_2.png)

Media/volume keys as their own category in the key picker — here the wheel and middle-click are remapped to Volume Up/Down and Play/Pause:

![configui: media control key picker](img/configgui_3.png)

### Requirements

- Windows 10/11
- A Razer Tartarus Pro
- The [Interception](https://github.com/oblitum/Interception) driver — optional, but required for D-pad/wheel/middle-click remapping and for Hyper Shift to avoid affecting a real keyboard's Alt key (see "Known limitation" below)
- **Razer Synapse must be fully closed (task-killed) before running `tartarus_driver`, every time** — right-click its tray icon and quit, or end its GUI process (`RzSynapse`/`RazerCentral`-type process) via Task Manager; its background services can stay running, that's fine. If Synapse's GUI is still running at the same time, it independently injects its own key bindings for the exact same physical input, causing double input (see `USAGE.md`'s Troubleshooting section).

### Download

Pre-built executables are published on the repo's **Releases** page for every tagged version. Alternatively, build from source:

```powershell
cd tartarus_driver
cargo build --release
```

### Quick start

```powershell
cd tartarus_driver
cargo run --release          # runs until Ctrl+C
```

No Synapse required — the driver sends the analog-stream unlock command itself at startup. While running, everything is also logged to `tasks/run.log` (in addition to stdout), independent of how the process was launched.

To change key assignments, run `cargo run --release -- configui` and open the URL it prints in your browser. See **[`USAGE.md`](USAGE.md)** for the full walkthrough (starting the driver, remapping keys, sensitivity, lighting, troubleshooting).

### Documentation map

- **[`USAGE.md`](USAGE.md)** — day-to-day usage
- **[`CHANGELOG.md`](CHANGELOG.md)** — what changed in each version
- **`config.example.toml`** — documented schema/defaults for the key-remap config file (`config.toml`, gitignored — everyone's layout differs)

### Known limitation / extra setup step

**Fully suppressing the D-pad (arrow keys) turned out to be impossible with Windows user-mode APIs alone** (low-level hooks + Raw Input). `WH_KEYBOARD_LL` is structurally guaranteed by Windows to fire before the matching `WM_INPUT` (needed for device identification) — no amount of thread/timing tuning can change that. Razer Synapse itself uses a kernel driver (`RzFilter.sys`) for the same reason.

To solve this, D-pad/wheel/middle-click remapping (and Hypershift's Alt detection) goes through the third-party kernel-mode driver [Interception](https://github.com/oblitum/Interception) (via the `interception` crate, which uses **Interception v1.0.1**). **Interception needs a separate, one-time setup — both a kernel driver install AND a DLL placement step**, since this project deliberately doesn't bundle Interception's own DLL (a licensing choice — see `interception`'s LGPL terms):

1. Download `Interception.zip` from [Releases](https://github.com/oblitum/Interception/releases/latest) (use **v1.0.1** to match what this project links against) and extract it
2. From an **administrator** console, run `install-interception.exe /install` inside the extracted `command line installer` folder
3. Reboot (required for the kernel driver install)
4. Copy `library\x64\interception.dll` from the extracted zip into the **same folder as `tartarus_driver.exe`** (this step is easy to miss — without it, the kernel driver installs fine but `tartarus_driver` still can't load `interception.dll` and silently falls back)

If any of this isn't done, `tartarus_driver` prints a warning and disables only the D-pad/wheel/middle-click remap; a real keyboard's Alt+Tab will be blocked while the driver runs. Everything else (analog keys, key remapping) keeps working normally either way (confirmed on real hardware, no crash).

**Anti-cheat-protected games may ignore this driver's input, or refuse to launch at all.** This driver sends keystrokes via Windows' `SendInput` API — the same mechanism virtually every keyboard remapping/macro tool uses — which carries an OS-level "synthetic input" signal that some anti-cheat engines specifically detect and discard, independent of administrator privileges (this is a deliberate anti-cheat policy decision, not a Windows permission issue admin rights can override). Third-party kernel drivers like Interception have also been reported to trigger some anti-cheat engines' launch-block checks. **Confirmed on real hardware**: Valorant (Riot Vanguard) accepts this driver's input normally; Apex Legends (Easy Anti-Cheat) does not. There is no reliable way around this for an EAC-protected title — it's intentional anti-cheat design, and reports even of stripping the OS-level injected-input flag before it reaches the game were still blocked. Because anti-cheat systems generally can't distinguish a legitimate hardware remapper from a macro/cheat tool at this level (the underlying technique is identical either way), using this driver with anti-cheat-protected competitive games may also carry account-suspension risk — use your own judgment per title.

### License

Licensed under the **[GNU General Public License v3.0 (GPL-3.0)](LICENSE)**.

**Author**: [ultramonaka](https://github.com/ultramonaka)

---

<a name="japanese"></a>
## 日本語

**`tartarus_driver`** は、Razer Tartarus Pro(左手用アナログキースイッチ搭載デバイス)を、Razer Synapseを一切使わずに動かすための、Windows向け自作Rustドライバです。バックグラウンドサービスもテレメトリも、ベンダー製ソフトウェアも一切不要です。

> **免責事項**: 本プロジェクトはRazer社とは無関係の、個人・非公式のプロジェクトです。Razer社による公認・支援は受けていません。「Razer」「Tartarus」はRazer社の商標です。本ソフトウェアは無保証で提供されます(詳細は[ライセンス](#ライセンス)を参照)。

### 主な機能

- **アナログキー20個をフルサポート** — HID経由で各キーの押し込み深度(0-255)を直接読み取り、ヒステリシス判定でチャタリングなくキー入力に変換
- **再コンパイル不要のフルリマップ** — 全キー、十字キー、ホイール、中クリックをブラウザの設定画面(`configui`)から再割り当て可能。**リアルタイム感度キャリブレーション**、**キー個別の感度設定**、**メディア/音量キー**(専用カテゴリから選択)にも対応
- **十字キー・ホイール・中クリックのカーネルレベルリマップ**([Interception](https://github.com/oblitum/Interception)経由) — 同時に接続している実キーボード・実マウスには一切影響しない
- **ハイパーシフト** — 「Hyper Response」サムボタンの動作を設定可能。レイアウト切替(モーメンタリ=押している間、またはトグル=押すたびに2〜3レイヤーを巡回)か、普通の修飾キー(既定Alt)としてそのまま送信するかを選べる。実キーボードのAlt+Tabに副作用なし
- **LEDライティング制御** — 単色・呼吸・スペクトラム・ウェーブ・リアクティブの各エフェクト
- **無期限のバックグラウンド実行**、コンソール窓を出さないタスクトレイモード(`tray`)にも対応

### スクリーンショット

`configui`(ブラウザの設定画面)— 全アナログキーを最大3レイヤー(通常/Layer1/Layer2)分リマップし、ハイパーシフトのモードやメディア/音量キーもカテゴリから選択できる:

![configui: キー割り当て設定](img/configgui_1.png)

リアルタイム感度キャリブレーション — 各キーの生の押し込み深度を見ながら、キーごとの`t_on`/`t_off`を調整できる:

![configui: ライブキャリブレーション](img/configgui_2.png)

キーピッカーの「メディア操作」カテゴリ — ここではホイールと中クリックを音量上げ/下げ・再生一時停止に割り当てている:

![configui: メディア操作キーピッカー](img/configgui_3.png)

### 動作環境

- Windows 10/11
- Razer Tartarus Pro本体
- [Interception](https://github.com/oblitum/Interception)ドライバ — 任意だが、十字キー/ホイール/中クリックのリマップと、ハイパーシフトが実キーボードのAltに影響しないようにするために必要(下記「既知の制約」参照)
- **`tartarus_driver`を起動する前に、毎回必ずRazer Synapseを完全に終了(タスクキル)しておくこと** — タスクトレイのアイコンを右クリックして終了するか、タスクマネージャーでGUIプロセス(`RzSynapse`/`RazerCentral`系)を終了する(バックグラウンドサービス自体は残っていて問題ない)。Synapseのアプリ本体が起動したままだと、同じ物理入力に対してSynapse自身も独自のキー割り当てを注入してしまい、二重入力になる(詳細は`USAGE.md`のトラブルシューティング参照)。

### ダウンロード

タグ付きバージョンごとに、ビルド済み実行ファイルをリポジトリの**Releases**ページで配布しています。ソースからビルドする場合:

```powershell
cd tartarus_driver
cargo build --release
```

### クイックスタート

```powershell
cd tartarus_driver
cargo run --release          # Ctrl+Cを押すまで無期限に動く
```

Synapseは不要。起動時に自動でアナログストリームの有効化コマンドを送信する。実行中は必ず`tasks/run.log`にもログが出力される(標準出力と同時、シェルのパイプに依存しない)。

キー割り当てを変更したい場合は`cargo run --release -- configui`を実行し、表示されたURLをブラウザで開く。起動方法・キー変更・感度・ライティング・トラブルシューティングの詳細は**[`USAGE.md`](USAGE.md)**を参照。

### ドキュメント構成

- **[`USAGE.md`](USAGE.md)** — 日常の使い方
- **[`CHANGELOG.md`](CHANGELOG.md)** — 各バージョンの変更点
- **`config.example.toml`** — キー割り当てファイル(`config.toml`、gitignore対象)のスキーマとデフォルト値の例

### 既知の制約 / セットアップ追加手順

**十字キー(D-pad)の完全な抑止はWindowsのユーザーモードAPI(低レベルフック + Raw Input)だけでは不可能と判明した。** `WH_KEYBOARD_LL`(低レベルフック)は常に`WM_INPUT`(Raw Input、デバイス判別に必要)より先に呼ばれるという、Windows自体の構造的な順序保証があり、スレッド構成やタイミング調整では解決できない。Razer Synapse自身も同様の理由でカーネルドライバ(`RzFilter.sys`など)を使っている。

この制約を解消するため、十字キー・ホイール・ホイールクリックのリマップ(およびHypershiftのAlt検知)はサードパーティのカーネルモードドライバ[Interception](https://github.com/oblitum/Interception)(`interception`クレート経由、**Interception v1.0.1**を使用)経由の実装に置き換え済み。**Interceptionは初回のみ、カーネルドライバのインストールに加えて、DLLの配置も必要**(このプロジェクトではライセンス上の理由からInterception本体のDLLを同梱しない方針のため — `interception`のLGPL条項を参照):

1. [Releases](https://github.com/oblitum/Interception/releases/latest)から`Interception.zip`(このプロジェクトがリンクしているバージョンと合わせて**v1.0.1**を推奨)をダウンロードして展開
2. 管理者権限のコンソールで、展開した`command line installer`フォルダ内の`install-interception.exe /install`を実行
3. PCを再起動(カーネルドライバのインストールに必須)
4. 展開したzip内の`library\x64\interception.dll`を、**`tartarus_driver.exe`と同じフォルダ**にコピーする(見落としやすい手順。これをしないと、カーネルドライバは入っていても`tartarus_driver`が`interception.dll`を読み込めず、気づかないうちにフォールバック動作になる)

いずれかが未完了の場合、`tartarus_driver`は警告を表示してD-pad/ホイール/ホイールクリックのリマップだけを無効化し、実キーボードのAlt+Tabもドライバ動作中はブロックされる。それ以外(アナログキー・キーリマップ)はどちらの場合も正常に動作を続ける(クラッシュしない、実機で確認済み)。

**アンチチート導入済みのゲームでは、本ドライバの入力が無視される、またはゲーム自体が起動しないことがある。** 本ドライバはWindowsの`SendInput` API(キーボードリマップ・マクロツールのほぼ全てが使う仕組み)でキー入力を送信しているが、これにはOSレベルで「合成された入力である」ことを示す情報が付随しており、一部のアンチチートエンジンはこれを検知して意図的に無視する — これは管理者権限では回避できない(Windowsの権限問題ではなく、アンチチート側の意図的な設計判断のため)。サードパーティのカーネルドライバ(Interception等)自体がアンチチートの起動ブロック判定のトリガーになったという報告もある。**実機で確認済み**: Valorant(Riot Vanguard)は本ドライバの入力を正常に受け付けるが、Apex Legends(Easy Anti-Cheat)は受け付けない。EAC保護下のタイトルに対する確実な回避策は無い(意図的なアンチチート設計であり、OSレベルの「注入フラグ」を剥がす試みですらブロックされ続けたという報告もある)。アンチチート側は「正規のハードウェアリマップツール」と「マクロ/チートツール」をこのレベルでは区別できないため(内部的に使う技術が同一のため)、アンチチート導入済みの競技性の高いゲームで本ドライバを使うことはアカウント停止のリスクも伴い得る — タイトルごとにご自身の判断で利用してください。

### ライセンス

**[GNU General Public License v3.0 (GPL-3.0)](LICENSE)** の下で公開しています。

**作者**: [ultramonaka](https://github.com/ultramonaka)
