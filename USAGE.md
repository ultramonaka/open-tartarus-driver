# Usage

**[English](#english)** | **[日本語](#japanese)**

---

<a name="english"></a>
## English

### 1. Prerequisites

- Razer Synapse must **not be running**. Right-click its tray icon and quit (its background services can stay running, that's fine). If Synapse is running at the same time, it independently injects its own key bindings for the same input, causing double input.
- To use D-pad/wheel/middle-click remapping, the [Interception](https://github.com/oblitum/Interception) driver must be installed (one-time). See "Known limitation / extra setup step" in `README.md`. Without it, everything else (analog keys, Hypershift) still works fine.

### 1.5 Folder layout

Everything is resolved relative to wherever `tartarus_driver.exe` itself is — put it in any folder, and the rest lives alongside it:

```
your-folder\
├── tartarus_driver.exe    ← the driver
├── run-tray.bat           ← double-click for tray mode
├── interception.dll       ← copy this in yourself (see "Known limitation" in README.md); optional
├── config.toml            ← optional; created by configui, or copy config.example.toml and edit it
├── config.example.toml
└── tasks\
    └── run.log            ← created automatically the first time you run it
```

`config.toml` and `tasks\run.log` don't need to exist beforehand — the driver creates/updates them on its own. `interception.dll` is the only one you have to place manually.

### 2. Running the driver

If you downloaded the pre-built exe from Releases, run it directly:

```powershell
.\tartarus_driver.exe          # no args: runs indefinitely until Ctrl+C (normal usage)
.\tartarus_driver.exe 30       # a number: runs that many seconds then exits (for testing)
.\tartarus_driver.exe tray     # no console window, runs from a system tray icon instead
```

Building from source instead, run the equivalent via cargo:

```powershell
cd tartarus_driver
cargo run --release
cargo run --release -- 30
cargo run --release -- tray
```

- The first line printed (and logged) at startup is always `tartarus_driver vX.Y.Z`, so you can confirm which version you're running (e.g. against the Releases page) without checking the exe's properties.
- On startup, it automatically sends the init command needed to stream analog data without Synapse.
- While running, pressing keys logs to stdout (and only to `tasks/run.log` in `tray` mode). The actual disk write happens on its own background thread — logging never adds latency to key presses, D-pad/wheel remapping, or Hypershift, no matter how fast you press. `run.log` is also capped at ~5 MiB (it truncates and keeps going rather than growing forever), since `tray` mode is meant to be left running for days.
- With no arguments, it runs until Ctrl+C or the console window is closed. Unless force-killed (e.g. via Task Manager), any held key is automatically released on shutdown.
- **Double-clicking the exe from Explorer runs normal mode (no args)**, so a console window stays open — this is expected. To get `tray` mode instead, either run `tartarus_driver.exe tray` from a terminal, or double-click **`run-tray.bat`** (included in the release zip), which does the same thing.

#### System tray mode (`tray`)

```powershell
.\tartarus_driver.exe tray
```

- Detaches the console window at startup (when possible), so it can run in the background.
- Also starts configui's web server automatically (no need to launch `configui` separately).
- Right-click the tray icon for a menu: "Open settings (configui)", a grayed-out "Version: vX.Y.Z" line, "Check for updates (GitHub)", and "Quit". "Open settings" opens the browser where you can edit and save key assignments (restarting `tray` mode itself is still needed to apply changes, same as normal mode). "Check for updates" just opens the public repo's Releases page in your browser — there's no automatic update check (no background network calls by design). "Quit" performs the same safe shutdown as Ctrl+C (releasing any held key, etc.) before exiting.
- No console output — check `tasks/run.log` instead.

#### Debug emulator (`emulate`), no hardware required

```powershell
.\tartarus_driver.exe emulate
```

Loads `config.toml` exactly like normal mode, but never touches HID, Interception, or the Razer control device — no Tartarus Pro needed at all. Useful for trying out a keymap/actuation change, or just poking at the hysteresis/Hypershift logic, when the device isn't at hand.

Type commands and press Enter:

| Command | Effect |
|---|---|
| `5` | tap key 5: DOWN then UP |
| `5 down` / `5 up` | hold / release key 5 |
| `5 140` | set key 5's raw depth directly to 140 (0-255) — handy for testing an exact `t_on`/`t_off` threshold |
| `hyper` | toggle Hypershift, as if the Hyper Response button were pressed/released |
| `help` | show the command list again |
| `quit` | exit (also releases any key still held) |

It sends real `SendInput` keystrokes just like normal mode, so whatever window has focus will actually receive them — keep focus on a scratch window (Notepad, etc.) while trying it out, not something you don't want to type into.

### 3. Changing key assignments

Key assignments live in `config.toml` (repo root). If it doesn't exist, the driver falls back to the built-in placeholder keymap (same content as `config.example.toml`).

Two ways to edit it:

#### Using the browser config UI (`configui`) — recommended

```powershell
cd tartarus_driver
cargo run --release -- configui
```

Open the URL shown in the console (`http://127.0.0.1:7878/`) in your browser. You'll see dropdowns for the 20 analog keys (Default layer and Layer1 each), the D-pad, the wheel, and middle-click — pick the keys you want and click "Save".

The page itself has a language switcher (top right, English/日本語). Switching it re-translates the page immediately and is saved right away (independent of the "Save" button below) — it's remembered the next time you open `configui`, in `config.toml`'s `[configui]` section. Defaults to English.

**Notes**:
- `configui` is not the driver itself — it's only a config editor. It never reads HID data or sends keystrokes.
- Saving overwrites `config.toml` wholesale. To apply the change, restart the normally-running `tartarus_driver` (step 2) — there's no live auto-reload.
- It's easiest to close the config page (Ctrl+C) before running the normal driver (running both at once is harmless — `configui` just waits as a web server — but keeping them separate avoids confusion).

#### Editing `config.toml` directly

Copy `config.example.toml` to `config.toml` and edit the values. Valid key names (case-insensitive):

- Single digits: `"0"`–`"9"`
- Single letters: `"A"`–`"Z"`
- Function keys: `"F1"`–`"F24"`
- D-pad directions: `"LEFT"` / `"UP"` / `"RIGHT"` / `"DOWN"`
- Others: `"SPACE"` `"ENTER"` `"TAB"` `"ESCAPE"` `"BACKSPACE"` `"LSHIFT"` `"RSHIFT"` `"LCTRL"` `"RCTRL"` `"LALT"` `"RALT"` `"HOME"` `"END"` `"PAGEUP"` `"PAGEDOWN"` `"INSERT"` `"DELETE"`

An unrecognized key name falls back to the built-in default for that one key, with a warning in `tasks/run.log` (the driver never crashes over this).

### 4. Adjusting sensitivity (actuation point)

Use configui's "Sensitivity" panel, or `config.toml`'s `[actuation]` section, to change the depth at which a key registers as ON (`t_on`) and OFF (`t_off`) — raw 0-255 values, default `t_on=100`/`t_off=80`. `t_off` must be less than `t_on`; a combination that violates this falls back to the built-in defaults for both. This has no effect on responsiveness (it's just a threshold — the cost of the comparison itself doesn't change).

### 5. Changing lighting (LED)

Configure via configui's "Lighting (LED)" panel, or `config.toml`'s `[lighting]` section.

```toml
[lighting]
effect = "static"       # none (don't change) | off | static | breathing | spectrum | wave | reactive
color = "FF6A00"         # RRGGBB hex, used by static/breathing/reactive
brightness = 255         # 0-255
wave_direction = "left"  # left | right, used by wave
reactive_speed = 2       # 1-4, used by reactive
```

With `effect = "none"` (the default, same as omitting the section), the driver never sends any lighting command, leaving the device's current state untouched (whatever effect was last set is stored on the device itself). Any other value gets (re-)sent once every time the driver starts (no effect on responsiveness — it's sent once or twice at startup, unrelated to the key-input main loop).

**Hardware verification status (2026-07-21)**: `off`/`static`/`spectrum` visually confirmed. `breathing`/`wave`/`reactive` use the same command shape so likely work, but aren't individually confirmed yet. If it doesn't light up as expected, check for a `Lighting: ...` line in `tasks/run.log`, and whether `WARNING: failed to send lighting ...` appears. Per-key individual color control is not implemented.

### 6. Hypershift layer

Holding the "Hyper Response" thumb button (next to the D-pad, labeled "D" in Razer's manual) switches all 20 analog keys to the `[keys.layer1]` assignments in `config.toml`. Releasing it switches back to `[keys.default]`. While held, Alt itself is never sent to the OS (the button physically sends an Alt keycode, and this is deliberately blocked).

**Fixed 2026-07-21**: this Alt detection used to be implemented with a `WH_KEYBOARD_LL` hook that couldn't tell which keyboard an Alt press came from, so a real keyboard's Alt (Alt+Tab, Alt+F4, etc.) also stopped reaching the OS while the driver ran. It's now unified with the same Interception-based device-aware handling used for the D-pad — **only the Tartarus's own Alt (Hyper Response) is affected; a real keyboard's Alt+Tab etc. is untouched** (confirmed on real hardware).

**Known limitation (only when Interception isn't installed)**: if the Interception driver isn't available, the driver automatically falls back to the old `WH_KEYBOARD_LL` hook. In that case device discrimination isn't possible, so the "real keyboard's Alt gets blocked too" limitation returns (look for `falling back to the hook-based Hypershift detection` in the startup log). If there's no reason not to, installing Interception (see `README.md`) is recommended.

### 7. Troubleshooting

| Symptom | Check |
|---|---|
| Analog keys do nothing | Confirm Synapse's GUI is really closed (`Get-Process \| Where ProcessName -match 'Razer\|Synapse'` should show no GUI-looking process). Unplugging/replugging the device can also help |
| D-pad/wheel still act as native arrow keys/scroll too (double input) | Check the startup log for `WARNING: interception.dll could not be loaded` or `Interception::new() returned None`. The former usually means the kernel driver is installed but `interception.dll` itself wasn't copied next to `tartarus_driver.exe` (step 4 in README's "Known limitation") — this is easy to miss since the kernel driver install alone doesn't produce any error, just this fallback |
| Saved in `configui` but nothing changed | Confirm you restarted the normal (or `tray`-mode) driver — there's no auto-reload |
| No tray icon in `tray` mode | Check `tasks/run.log` for `[tray] WARNING: ...` (window class registration / window creation / icon add failure). Right after an Explorer restart, try relaunching `tray` |
| Some keys in `config.toml` stay at their default | Check `tasks/run.log` for `WARNING: config.toml [...] is not a recognized key name`. See section 3 above for valid key names |
| A real keyboard/mouse started behaving oddly | Interception/the hook only ever targets the Tartarus Pro's hardware ID (VID 0x1532/PID 0x0244) — fail-open by design. If this still happens, it's a bug; please save `tasks/run.log` for investigation |
| Lighting was configured but nothing changes | Check `tasks/run.log` for a `Lighting: effect set to "..."` line (if missing, `[lighting]`'s `effect` is still `"none"`, or the config wasn't loaded). A `WARNING: failed to send lighting ...` means the HID write itself failed |

---

<a name="japanese"></a>
## 日本語

### 1. 前提条件

- Razer Synapseは**起動していないこと**。タスクトレイのアイコンを右クリックして終了しておく(バックグラウンドサービス自体は残っていて問題ない)。同時に動いていると、Synapse自身も同じ入力に対して独自のキー割り当てを注入するため、二重入力になる。
- 十字キー・ホイール・ホイールクリックのリマップを使うには、[Interception](https://github.com/oblitum/Interception)ドライバのインストールが必要(1回だけ)。手順は`README.md`の「既知の制約 / セットアップ追加手順」を参照。未インストールでも、それ以外の機能(アナログキー・Hypershift)は問題なく動く。

### 1.5 フォルダ構成

すべてのファイルは`tartarus_driver.exe`自身の場所を基準に解決される。どこのフォルダに置いてもよく、必要なものは全部その隣に並ぶ:

```
好きなフォルダ\
├── tartarus_driver.exe    ← 本体
├── run-tray.bat           ← ダブルクリックでtrayモード起動
├── interception.dll       ← 手動でコピーする(README.mdの「既知の制約」参照)。任意
├── config.toml            ← 任意。configuiで作成するか、config.example.tomlをコピーして編集
├── config.example.toml
└── tasks\
    └── run.log            ← 初回起動時に自動作成
```

`config.toml`と`tasks\run.log`は事前に用意しなくてよい(ドライバが自動で作成・更新する)。手動で用意が必要なのは`interception.dll`だけ。

### 2. ドライバを起動する

Releasesからビルド済みexeをダウンロードした場合は、直接実行する:

```powershell
.\tartarus_driver.exe          # 引数なし: Ctrl+Cを押すまで無期限に動く(通常の使い方)
.\tartarus_driver.exe 30       # 数字を渡すとその秒数だけ動いて自動終了(テスト用)
.\tartarus_driver.exe tray     # コンソール窓を出さず、タスクトレイアイコンで動かす
```

ソースからビルドする場合は、cargo経由で同等のコマンドを実行する:

```powershell
cd tartarus_driver
cargo run --release
cargo run --release -- 30
cargo run --release -- tray
```

- 起動時に最初に表示・記録される行は必ず`tartarus_driver vX.Y.Z`なので、exeのプロパティを確認しなくても、今動いているバージョンをReleasesページと照合できる。
- 起動直後に、Synapseなしでアナログデータを流すための初期化コマンドを自動送信する。
- 実行中はキーを押すとログが標準出力(`tray`モードでは`tasks/run.log`のみ)に出る。実際のディスク書き込みは専用のバックグラウンドスレッドで行われるため、どれだけ速くキーを押しても、キー入力・十字キー/ホイールのリマップ・Hypershiftの反応速度にログ処理が影響することはない。`run.log`自体も約5MiBで頭出しして書き続ける仕組み(無限に肥大化しない)なので、`tray`モードで何日も動かし続けても問題ない。
- 引数なしの場合はCtrl+Cを押すか、コンソール窓を閉じるまで動き続ける。強制終了(タスクマネージャーでの「タスクの終了」など)でない限り、終了時に押しっぱなしのキーがあれば自動的に離す処理が入る。
- **エクスプローラーからexeをダブルクリックすると、引数なしの通常モードで起動する**ため、コンソール窓が出たままになるのは正常な動作。`tray`モードにしたい場合は、ターミナルから`tartarus_driver.exe tray`を実行するか、リリースzipに同梱されている**`run-tray.bat`**をダブルクリックする(同じ動作をする)。

#### タスクトレイモード (`tray`)

```powershell
.\tartarus_driver.exe tray
```

- 起動時にコンソール窓を自動的に切り離す(可能な場合)ので、バックグラウンドで動かせる。
- 同時に`configui`のWebサーバーも自動で起動している(別途`configui`を起動する必要はない)。
- タスクトレイのアイコンを右クリックすると「設定を開く (configui)」「バージョン: vX.Y.Z」(グレー表示、クリック不可)「アップデートを確認 (GitHub)」「終了」のメニューが出る。「設定を開く」でブラウザが開き、そのままキー割り当てを編集・保存できる(反映には`tray`モード自体の再起動が必要、通常起動時と同様)。「アップデートを確認」は公開リポジトリのReleasesページをブラウザで開くだけで、自動での更新チェックは行わない(バックグラウンドでの通信は一切しない設計)。「終了」を選ぶと、Ctrl+Cと同じ安全な終了処理(押しっぱなしキーの解放など)を行ってから終了する。
- ログはコンソールに出ないため`tasks/run.log`を確認する。

#### デバッグ用エミュレータ (`emulate`)、実機不要

```powershell
.\tartarus_driver.exe emulate
```

通常モードと同じく`config.toml`を読み込むが、HID・Interception・Razerコントロールデバイスには一切触れない — Tartarus Proが手元になくても動く。キーマップ/感度設定を変えて試したいときや、ヒステリシス・Hypershiftのロジックだけ触って確認したいときに使う。

コマンドを入力してEnter:

| コマンド | 効果 |
|---|---|
| `5` | key5をタップ(DOWN→UP) |
| `5 down` / `5 up` | key5を押しっぱなし/離す |
| `5 140` | key5の生の深度を直接140(0-255)に設定 — `t_on`/`t_off`のしきい値ちょうどをテストしたいときに便利 |
| `hyper` | Hyper Responseボタンを押した/離したときと同じようにHypershiftを切り替える |
| `help` | コマンド一覧を再表示 |
| `quit` | 終了(押しっぱなしのキーがあれば解放してから終了) |

通常モードと同じく本物の`SendInput`キー入力を送るため、その時フォーカスされているウィンドウに実際にキーが入力される。試す間はメモ帳など、打ち込まれても困らないウィンドウにフォーカスを置いておくこと。

### 3. キー割り当てを変える

`config.toml`(リポジトリルート)にキー割り当てが書かれている。存在しない場合はビルトインのプレースホルダーキーマップ(`config.example.toml`と同じ内容)にフォールバックする。

編集方法は2つ:

#### ブラウザ設定画面(`configui`)を使う — 推奨

```powershell
cd tartarus_driver
cargo run --release -- configui
```

コンソールに表示されるURL(`http://127.0.0.1:7878/`)をブラウザで開く。20個のアナログキー(通常レイヤー・Layer1それぞれ)、十字キー、ホイール、ホイールクリックのプルダウンが並んでいるので、割り当てたいキーを選んで「保存」を押す。

ページ右上に言語切り替え(English/日本語)がある。切り替えるとその場でページ全体が翻訳され、即座に保存される(下の「保存」ボタンとは独立)。次回`configui`を開いたときも覚えている(`config.toml`の`[configui]`セクションに記録)。既定は英語。

**注意点**:
- `configui`はドライバ本体ではない。設定画面を出すだけで、HID読み取りやキー送信は一切行わない。
- 保存すると`config.toml`を丸ごと書き換える。反映するには、動かしている通常起動の`tartarus_driver`(手順2)を再起動する必要がある(自動リロードはしない)。
- 設定画面は終了(Ctrl+C)してから、通常起動のドライバを動かす、という順番が分かりやすい(同時に動かしても害はないが、`configui`側は単にWebサーバーとして待機するだけ)。

#### `config.toml`を直接編集する

`config.example.toml`をコピーして`config.toml`にリネームし、値を書き換えてもよい。使える値(キー名)は以下のいずれか(大文字小文字は区別しない):

- 数字1文字: `"0"`〜`"9"`
- アルファベット1文字: `"A"`〜`"Z"`
- ファンクションキー: `"F1"`〜`"F24"`
- 十字キー用: `"LEFT"` / `"UP"` / `"RIGHT"` / `"DOWN"`
- その他: `"SPACE"` `"ENTER"` `"TAB"` `"ESCAPE"` `"BACKSPACE"` `"LSHIFT"` `"RSHIFT"` `"LCTRL"` `"RCTRL"` `"LALT"` `"RALT"` `"HOME"` `"END"` `"PAGEUP"` `"PAGEDOWN"` `"INSERT"` `"DELETE"`

存在しないキー名を書いた場合、そのキーだけビルトインの既定値にフォールバックし、`tasks/run.log`に警告が出る(ドライバが落ちることはない)。

### 4. 感度(アクチュエーションポイント)を変える

`configui`の「感度」パネル、または`config.toml`の`[actuation]`セクションで、キーがONと判定される深さ(`t_on`)とOFFと判定される深さ(`t_off`)を変更できる(0-255の生値、既定は`t_on=100`/`t_off=80`)。`t_off`は`t_on`より小さい値にする必要があり、この制約に違反する組み合わせは両方ともビルトインの既定値にフォールバックする。反応速度には影響しない(この設定は「どのくらい押し込んだら反応するか」のしきい値であり、判定処理自体の負荷は変わらない)。

### 5. ライティング(LED)を変える

`configui`の「ライティング (LED)」パネル、または`config.toml`の`[lighting]`セクションで設定する。

```toml
[lighting]
effect = "static"       # none(変更しない) | off | static | breathing | spectrum | wave | reactive
color = "FF6A00"         # RRGGBB 16進数、static/breathing/reactiveで使用
brightness = 255         # 0-255
wave_direction = "left"  # left | right、waveで使用
reactive_speed = 2       # 1-4、reactiveで使用
```

`effect = "none"`(既定・セクション省略時も同じ)の場合、ドライバはライティングコマンドを一切送信せず、デバイス側の現在の状態(前回設定した効果は本体に保存されている)をそのまま維持する。それ以外を指定すると、ドライバ起動時に毎回そのコマンドを送信する(反応速度への影響なし — 起動時に1〜2回送るだけで、キー入力のメインループとは無関係)。

**実機検証状況(2026-07-21)**: `off`/`static`/`spectrum`は目視確認済み。`breathing`/`wave`/`reactive`は同じコマンド形状のため動作する可能性が高いが未個別確認。期待通りに光らない場合は`tasks/run.log`の`Lighting: ...`行、および`WARNING: failed to send lighting ...`が出ていないか確認。per-key単位で1キーずつ違う色を指定する機能は未実装。

### 6. Hypershiftレイヤー

本体左上の「Hyper Response」ボタン(D-padの隣、Razer公式の名称は"D")を押している間だけ、20個のアナログキーが`config.toml`の`[keys.layer1]`側の割り当てに切り替わる。離すと`[keys.default]`に戻る。押している間、Alt自体はOSに送られない(元のボタンがAltキーコードを送る仕様のため、意図的にブロックしている)。

**2026-07-21修正**: 以前はこのAlt検知がソースデバイスを判別しない`WH_KEYBOARD_LL`フックで実装されていたため、`tartarus_driver`が動いている間は実キーボードのAlt(Alt+Tab、Alt+F4等)も一緒にOSに届かなくなる問題があった。現在はD-pad同様Interception経由のデバイス判別処理に統一されており、**Tartarus本体のAlt(Hyper Response)だけが対象になる。実キーボードのAlt+Tab等は影響を受けない**(実機確認済み)。

**既知の制約(Interception未インストール時のみ)**: Interceptionドライバが利用できない場合に限り、旧`WH_KEYBOARD_LL`フックへ自動的にフォールバックする。この場合はデバイス判別ができないため、上記の「実キーボードのAltも道連れでブロックされる」制約が復活する(起動時ログに`falling back to the hook-based Hypershift detection`と出ていれば該当)。Interceptionを未インストールのままにする理由がなければ、`README.md`の手順でインストールしておくことを推奨。

### 7. トラブルシューティング

| 症状 | 確認すること |
|---|---|
| アナログキーが何も反応しない | Synapseのタスクトレイアイコンが本当に閉じているか確認(`Get-Process \| Where ProcessName -match 'Razer\|Synapse'`でGUIプロセスが出ないこと)。デバイスの抜き差しも有効な場合がある |
| 十字キー/ホイールが元の矢印キー・スクロールとしても動いてしまう(二重入力) | 起動時ログに`WARNING: interception.dll could not be loaded`または`Interception::new() returned None`と出ていないか確認。前者は多くの場合、カーネルドライバは入っているが`interception.dll`自体が`tartarus_driver.exe`と同じフォルダにコピーされていない状態(README「既知の制約」の手順4)。カーネルドライバのインストールだけではエラーが出ないため見落としやすい |
| `configui`で保存したのに反映されない | 通常起動(または`tray`モード)のドライバを再起動したか確認(自動リロードはしない) |
| `tray`モードでタスクトレイにアイコンが出ない | `tasks/run.log`に`[tray] WARNING: ...`が出ていないか確認(ウィンドウクラス登録・ウィンドウ作成・アイコン追加のいずれかの失敗)。Explorerの再起動直後などは再度`tray`を起動し直す |
| `config.toml`の一部のキーだけ既定値のままになる | `tasks/run.log`に`WARNING: config.toml [...] は認識できないキー名です`が出ていないか確認。使えるキー名の一覧は本ファイルの3節を参照 |
| 実キーボード/実マウスの動きが変になった | Interception/フックはTartarus Pro(VID 0x1532/PID 0x0244)のハードウェアID一致だけを対象にしている(fail-open設計)。それでも問題が起きた場合はバグなので、`tasks/run.log`を保存して調査する |
| ライティングを設定したのに光り方が変わらない | `tasks/run.log`に`Lighting: effect set to "..."`が出ているか確認(出ていなければ`[lighting]`の`effect`が`"none"`のまま、または設定が読み込まれていない)。`WARNING: failed to send lighting ...`が出ていればHID書き込み自体が失敗している |

