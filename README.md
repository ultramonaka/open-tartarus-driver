# Tartarus Pro Standalone Driver

**[English](#english)** | **[日本語](#japanese)**

[![License: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-blue.svg)](LICENSE)
![Version](https://img.shields.io/badge/version-1.0.0-informational.svg)
![Author](https://img.shields.io/badge/author-ultramonaka-lightgrey.svg)

---

<a name="english"></a>
## English

**`tartarus_driver`** is a from-scratch Rust driver for the Razer Tartarus Pro (the left-hand analog gaming keypad) that runs on Windows **without Razer Synapse at all** — no background service, no telemetry, no vendor software required.

### Features

- **Full analog key support** — reads the raw 0-255 depth of all 20 keys directly over HID and converts it to keystrokes with hysteresis-based actuation (no chattering)
- **Fully remappable, no recompiling** — every key, the D-pad, the wheel, and middle-click can be reassigned from a browser-based config page (`configui`), including a **live sensitivity calibration view** and **per-key actuation thresholds**
- **D-pad / wheel / middle-click remap** at the kernel level (via [Interception](https://github.com/oblitum/Interception)) — a real keyboard/mouse plugged in at the same time is never affected
- **Hypershift**: a temporary second key layer while the "Hyper Response" thumb button is held, with no side effects on a real keyboard's Alt+Tab
- **LED lighting control** — static color, breathing, spectrum, wave, and reactive effects
- **Runs in the background indefinitely**, optionally from a system tray icon (`tray` mode) with no console window

### Requirements

- Windows 10/11
- A Razer Tartarus Pro
- The [Interception](https://github.com/oblitum/Interception) driver — optional, but required for D-pad/wheel/middle-click remapping and for Hypershift to avoid affecting a real keyboard's Alt key (see "Known limitation" below)

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
- **`config.example.toml`** — documented schema/defaults for the key-remap config file (`config.toml`, gitignored — everyone's layout differs)

### Known limitation / extra setup step

**Fully suppressing the D-pad (arrow keys) turned out to be impossible with Windows user-mode APIs alone** (low-level hooks + Raw Input). `WH_KEYBOARD_LL` is structurally guaranteed by Windows to fire before the matching `WM_INPUT` (needed for device identification) — no amount of thread/timing tuning can change that. Razer Synapse itself uses a kernel driver (`RzFilter.sys`) for the same reason.

To solve this, D-pad/wheel/middle-click remapping (and Hypershift's Alt detection) goes through the third-party kernel-mode driver [Interception](https://github.com/oblitum/Interception) (via the `interception` crate). **The Interception driver itself needs a separate, one-time install**:

1. Download `Interception.zip` from [Releases](https://github.com/oblitum/Interception/releases/latest) and extract it
2. From an **administrator** console, run `install-interception.exe /install` inside the extracted `command line installer` folder
3. Reboot (required for the kernel driver install)

If it isn't installed, `tartarus_driver` prints a warning and disables only the D-pad/wheel/middle-click remap; a real keyboard's Alt+Tab will be blocked while the driver runs. Everything else (analog keys, key remapping) keeps working normally either way (confirmed on real hardware, no crash).

### License

Licensed under the **[GNU General Public License v3.0 (GPL-3.0)](LICENSE)**.

**Author**: [ultramonaka](https://github.com/ultramonaka)

---

<a name="japanese"></a>
## 日本語

**`tartarus_driver`** は、Razer Tartarus Pro(左手用アナログキースイッチ搭載デバイス)を、Razer Synapseを一切使わずに動かすための、Windows向け自作Rustドライバです。バックグラウンドサービスもテレメトリも、ベンダー製ソフトウェアも一切不要です。

### 主な機能

- **アナログキー20個をフルサポート** — HID経由で各キーの押し込み深度(0-255)を直接読み取り、ヒステリシス判定でチャタリングなくキー入力に変換
- **再コンパイル不要のフルリマップ** — 全キー、十字キー、ホイール、中クリックをブラウザの設定画面(`configui`)から再割り当て可能。**リアルタイム感度キャリブレーション**と**キー個別の感度設定**にも対応
- **十字キー・ホイール・中クリックのカーネルレベルリマップ**([Interception](https://github.com/oblitum/Interception)経由) — 同時に接続している実キーボード・実マウスには一切影響しない
- **Hypershift** — 「Hyper Response」サムボタンを押している間だけ有効になる一時的な第2レイヤー。実キーボードのAlt+Tabに副作用なし
- **LEDライティング制御** — 単色・呼吸・スペクトラム・ウェーブ・リアクティブの各エフェクト
- **無期限のバックグラウンド実行**、コンソール窓を出さないタスクトレイモード(`tray`)にも対応

### 動作環境

- Windows 10/11
- Razer Tartarus Pro本体
- [Interception](https://github.com/oblitum/Interception)ドライバ — 任意だが、十字キー/ホイール/中クリックのリマップと、Hypershiftが実キーボードのAltに影響しないようにするために必要(下記「既知の制約」参照)

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
- **`config.example.toml`** — キー割り当てファイル(`config.toml`、gitignore対象)のスキーマとデフォルト値の例

### 既知の制約 / セットアップ追加手順

**十字キー(D-pad)の完全な抑止はWindowsのユーザーモードAPI(低レベルフック + Raw Input)だけでは不可能と判明した。** `WH_KEYBOARD_LL`(低レベルフック)は常に`WM_INPUT`(Raw Input、デバイス判別に必要)より先に呼ばれるという、Windows自体の構造的な順序保証があり、スレッド構成やタイミング調整では解決できない。Razer Synapse自身も同様の理由でカーネルドライバ(`RzFilter.sys`など)を使っている。

この制約を解消するため、十字キー・ホイール・ホイールクリックのリマップ(およびHypershiftのAlt検知)はサードパーティのカーネルモードドライバ[Interception](https://github.com/oblitum/Interception)経由の実装に置き換え済み。**Interceptionドライバ本体の別途インストール(初回のみ)が必要**:

1. [Releases](https://github.com/oblitum/Interception/releases/latest)から`Interception.zip`をダウンロードして展開
2. 管理者権限のコンソールで、展開した`command line installer`フォルダ内の`install-interception.exe /install`を実行
3. PCを再起動(カーネルドライバのインストールに必須)

未インストールの場合、`tartarus_driver`は警告を表示してD-pad/ホイール/ホイールクリックのリマップだけを無効化し、実キーボードのAlt+Tabもドライバ動作中はブロックされる。それ以外(アナログキー・キーリマップ)はどちらの場合も正常に動作を続ける(クラッシュしない、実機で確認済み)。

### ライセンス

**[GNU General Public License v3.0 (GPL-3.0)](LICENSE)** の下で公開しています。

**作者**: [ultramonaka](https://github.com/ultramonaka)
