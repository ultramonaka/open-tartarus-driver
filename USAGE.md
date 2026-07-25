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
└── logs\
    └── run.log            ← created automatically the first time you run it
```

`config.toml` and `logs\run.log` don't need to exist beforehand — the driver creates/updates them on its own. `interception.dll` is the only one you have to place manually.

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
- While running, pressing keys logs to stdout (and only to `logs/run.log` in `tray` mode). The actual disk write happens on its own background thread — logging never adds latency to key presses, D-pad/wheel remapping, or Hypershift, no matter how fast you press. `run.log` is also capped at ~5 MiB (it truncates and keeps going rather than growing forever), since `tray` mode is meant to be left running for days.
- With no arguments, it runs until Ctrl+C or the console window is closed. Unless force-killed (e.g. via Task Manager), any held key is automatically released on shutdown.
- **Double-clicking the exe from Explorer runs normal mode (no args)**, so a console window stays open — this is expected. To get `tray` mode instead, either run `tartarus_driver.exe tray` from a terminal, or double-click **`run-tray.bat`** (included in the release zip), which does the same thing.

#### System tray mode (`tray`)

```powershell
.\tartarus_driver.exe tray
```

- Detaches the console window at startup (when possible), so it can run in the background.
- Also starts configui's web server automatically (no need to launch `configui` separately).
- Right-click the tray icon for a menu: "Open settings (configui)", a grayed-out "Version: vX.Y.Z" line, "Check for updates (GitHub)", and "Quit". "Open settings" opens the browser where you can edit and save key assignments (applied automatically within about a second, same as normal mode — no restart needed). "Check for updates" just opens the public repo's Releases page in your browser — there's no automatic update check (no background network calls by design). "Quit" performs the same safe shutdown as Ctrl+C (releasing any held key, etc.) before exiting.
- No console output — check `logs/run.log` instead.

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
| `hyper` | tap the Hyper Response button: press then release |
| `hyper down` / `hyper up` | hold / release the Hyper Response button |
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

Open the URL shown in the console (`http://127.0.0.1:7878/`) in your browser. You'll see pickers for the 20 analog keys (Default/Layer1/Layer2 each), a "Hyper Shift" panel (mode/switch style/layer count/modifier key), the D-pad, the wheel, and middle-click — pick the keys you want and click "Save". Each key picker has a small category dropdown next to it ("Basic" or "Media Control") to narrow the list before picking the actual key.

The page itself has a language switcher (top right, English/日本語). Switching it re-translates the page immediately and is saved right away (independent of the "Save" button below) — it's remembered the next time you open `configui`, in `config.toml`'s `[configui]` section. Defaults to English.

**Notes**:
- `configui` is not the driver itself — it's only a config editor. It never reads HID data or sends keystrokes.
- Saving overwrites `config.toml` wholesale. If `tartarus_driver` is already running, it picks up the change automatically within about a second — no restart needed.
- It's easiest to close the config page (Ctrl+C) before running the normal driver (running both at once is harmless — `configui` just waits as a web server — but keeping them separate avoids confusion).

#### Editing `config.toml` directly

Copy `config.example.toml` to `config.toml` and edit the values. Valid key names (case-insensitive):

- Single digits: `"0"`–`"9"`
- Single letters: `"A"`–`"Z"`
- Function keys: `"F1"`–`"F24"`
- D-pad directions: `"LEFT"` / `"UP"` / `"RIGHT"` / `"DOWN"`
- Others: `"SPACE"` `"ENTER"` `"TAB"` `"ESCAPE"` `"BACKSPACE"` `"LSHIFT"` `"RSHIFT"` `"LCTRL"` `"RCTRL"` `"LALT"` `"RALT"` `"HOME"` `"END"` `"PAGEUP"` `"PAGEDOWN"` `"INSERT"` `"DELETE"`

An unrecognized key name falls back to the built-in default for that one key, with a warning in `logs/run.log` (the driver never crashes over this).

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

**Hardware verification status (2026-07-21)**: `off`/`static`/`spectrum` visually confirmed. `breathing`/`wave`/`reactive` use the same command shape so likely work, but aren't individually confirmed yet. If it doesn't light up as expected, check for a `Lighting: ...` line in `logs/run.log`, and whether `WARNING: failed to send lighting ...` appears. Per-key individual color control is not implemented.

### 6. Hyper Shift (the "Hyper Response" thumb button)

**New in v1.0.5**: what the "Hyper Response" thumb button (next to the D-pad, labeled "D" in Razer's manual) does is now configurable via `config.toml`'s `[hypershift]` section (or `configui`'s "Hyper Shift" panel):

| `mode` | Effect |
|---|---|
| `"layer_switch"` (default) | The button switches which of `[keys.default]`/`[keys.layer1]`/`[keys.layer2]` is active — see `switch_style` below for exactly how |
| `"modifier_key"` | Layer switching is disabled entirely; the button just sends `modifier_key` (default `LALT`) on press/release, like any other key |

When `mode = "layer_switch"`, `switch_style` controls the trigger behavior:

| `switch_style` | Effect |
|---|---|
| `"momentary"` (default, original v1.0.0-v1.0.4 behavior) | Held selects Layer1, released returns to Default. Always exactly 2 layers — `layer_count` is ignored |
| `"toggle"` | Every press advances one layer, wrapping around (`layer_count = 2`: Default ↔ Layer1; `layer_count = 3`: Default → Layer1 → Layer2 → Default → …). Release does nothing |

While a non-Default layer is active, Alt itself is never sent to the OS (the button physically sends an Alt keycode, which the driver deliberately intercepts to detect the press/release — regardless of `mode`/`switch_style`).

**Fixed 2026-07-21**: this Alt detection used to be implemented with a `WH_KEYBOARD_LL` hook that couldn't tell which keyboard an Alt press came from, so a real keyboard's Alt (Alt+Tab, Alt+F4, etc.) also stopped reaching the OS while the driver ran. It's now unified with the same Interception-based device-aware handling used for the D-pad — **only the Tartarus's own Alt (Hyper Response) is affected; a real keyboard's Alt+Tab etc. is untouched** (confirmed on real hardware).

**Known limitation (only when Interception isn't installed)**: if the Interception driver isn't available, the driver automatically falls back to the old `WH_KEYBOARD_LL` hook. In that case device discrimination isn't possible, so the "real keyboard's Alt gets blocked too" limitation returns (look for `falling back to the hook-based Hypershift detection` in the startup log). If there's no reason not to, installing Interception (see `README.md`) is recommended.

### 7. Anti-cheat-protected games

Some games ignore this driver's input entirely, or refuse to launch while it's running — **confirmed on real hardware**: Valorant (Riot Vanguard) works fine, Apex Legends (Easy Anti-Cheat) does not accept any input from this driver. Administrator privileges don't change this. The cause is intentional anti-cheat behavior, not a bug in this driver: `SendInput` (how this driver, and virtually every remapping/macro tool, sends keystrokes) carries an OS-level "synthetic input" signal that some anti-cheat engines specifically detect and discard; third-party kernel drivers like Interception have also been reported to trigger some engines' launch-block checks. There is no reliable workaround for an EAC-protected title — this is deliberate anti-cheat design (see README's "Known limitation" section for more detail, including an account-risk note). Don't reach for a hardware adapter or virtual-controller-based workaround instead — that's not a safe substitute either, and for some titles it's a bannable-by-policy device regardless of whether it's technically detected (e.g. Apex Legends' Respawn explicitly bans Cronus Zen/Titan Two-class adapters as of March 2026, permanently and without appeal). If a specific game doesn't respond to this driver, this is almost certainly why — it's not worth spending time debugging further.

### 8. Troubleshooting

| Symptom | Check |
|---|---|
| Analog keys do nothing | Confirm Synapse's GUI is really closed (`Get-Process \| Where ProcessName -match 'Razer\|Synapse'` should show no GUI-looking process). Unplugging/replugging the device can also help |
| Analog keys work in Notepad/most apps but not in one specific game | Likely that game's anti-cheat blocking synthetic input — see section 7 above. Not fixable from this driver's side |
| D-pad/wheel still act as native arrow keys/scroll too (double input) | Check the startup log for `WARNING: interception.dll could not be loaded` or `Interception::new() returned None`. The former usually means the kernel driver is installed but `interception.dll` itself wasn't copied next to `tartarus_driver.exe` (step 4 in README's "Known limitation") — this is easy to miss since the kernel driver install alone doesn't produce any error, just this fallback |
| Saved in `configui` but nothing changed | Wait ~1s — the running driver picks up `config.toml` changes automatically. Check `logs/run.log` for `config.toml reloaded` (or a `WARNING: ... keeping the previous settings` if the file has a syntax error) |
| No tray icon in `tray` mode | Check `logs/run.log` for `[tray] WARNING: ...` (window class registration / window creation / icon add failure). Right after an Explorer restart, try relaunching `tray` |
| Some keys in `config.toml` stay at their default | Check `logs/run.log` for `WARNING: config.toml [...] is not a recognized key name`. See section 3 above for valid key names |
| A real keyboard/mouse started behaving oddly | Interception/the hook only ever targets the Tartarus Pro's hardware ID (VID 0x1532/PID 0x0244) — fail-open by design. If this still happens, it's a bug; please save `logs/run.log` for investigation |
| Lighting was configured but nothing changes | Check `logs/run.log` for a `Lighting: effect set to "..."` line (if missing, `[lighting]`'s `effect` is still `"none"`, or the config wasn't loaded). A `WARNING: failed to send lighting ...` means the HID write itself failed |

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
└── logs\
    └── run.log            ← 初回起動時に自動作成
```

`config.toml`と`logs\run.log`は事前に用意しなくてよい(ドライバが自動で作成・更新する)。手動で用意が必要なのは`interception.dll`だけ。

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
- 実行中はキーを押すとログが標準出力(`tray`モードでは`logs/run.log`のみ)に出る。実際のディスク書き込みは専用のバックグラウンドスレッドで行われるため、どれだけ速くキーを押しても、キー入力・十字キー/ホイールのリマップ・Hypershiftの反応速度にログ処理が影響することはない。`run.log`自体も約5MiBで頭出しして書き続ける仕組み(無限に肥大化しない)なので、`tray`モードで何日も動かし続けても問題ない。
- 引数なしの場合はCtrl+Cを押すか、コンソール窓を閉じるまで動き続ける。強制終了(タスクマネージャーでの「タスクの終了」など)でない限り、終了時に押しっぱなしのキーがあれば自動的に離す処理が入る。
- **エクスプローラーからexeをダブルクリックすると、引数なしの通常モードで起動する**ため、コンソール窓が出たままになるのは正常な動作。`tray`モードにしたい場合は、ターミナルから`tartarus_driver.exe tray`を実行するか、リリースzipに同梱されている**`run-tray.bat`**をダブルクリックする(同じ動作をする)。

#### タスクトレイモード (`tray`)

```powershell
.\tartarus_driver.exe tray
```

- 起動時にコンソール窓を自動的に切り離す(可能な場合)ので、バックグラウンドで動かせる。
- 同時に`configui`のWebサーバーも自動で起動している(別途`configui`を起動する必要はない)。
- タスクトレイのアイコンを右クリックすると「設定を開く (configui)」「バージョン: vX.Y.Z」(グレー表示、クリック不可)「アップデートを確認 (GitHub)」「終了」のメニューが出る。「設定を開く」でブラウザが開き、そのままキー割り当てを編集・保存できる(通常起動時と同様、約1秒以内に自動反映される。再起動不要)。「アップデートを確認」は公開リポジトリのReleasesページをブラウザで開くだけで、自動での更新チェックは行わない(バックグラウンドでの通信は一切しない設計)。「終了」を選ぶと、Ctrl+Cと同じ安全な終了処理(押しっぱなしキーの解放など)を行ってから終了する。
- ログはコンソールに出ないため`logs/run.log`を確認する。

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
| `hyper` | Hyper Responseボタンをタップ(押して離す) |
| `hyper down` / `hyper up` | Hyper Responseボタンを押しっぱなし/離す |
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

コンソールに表示されるURL(`http://127.0.0.1:7878/`)をブラウザで開く。20個のアナログキー(通常レイヤー・Layer1・Layer2それぞれ)、「ハイパーシフト」パネル(モード・切替方式・レイヤー数・修飾キー)、十字キー、ホイール、ホイールクリックのピッカーが並んでいるので、割り当てたいキーを選んで「保存」を押す。各キーピッカーには「基本」「メディア操作」のカテゴリ選択が付いており、先にカテゴリを絞ってから実際のキーを選べる。

ページ右上に言語切り替え(English/日本語)がある。切り替えるとその場でページ全体が翻訳され、即座に保存される(下の「保存」ボタンとは独立)。次回`configui`を開いたときも覚えている(`config.toml`の`[configui]`セクションに記録)。既定は英語。

**注意点**:
- `configui`はドライバ本体ではない。設定画面を出すだけで、HID読み取りやキー送信は一切行わない。
- 保存すると`config.toml`を丸ごと書き換える。`tartarus_driver`が既に動いている場合、約1秒以内に自動で変更を反映する(再起動は不要)。
- 設定画面は終了(Ctrl+C)してから、通常起動のドライバを動かす、という順番が分かりやすい(同時に動かしても害はないが、`configui`側は単にWebサーバーとして待機するだけ)。

#### `config.toml`を直接編集する

`config.example.toml`をコピーして`config.toml`にリネームし、値を書き換えてもよい。使える値(キー名)は以下のいずれか(大文字小文字は区別しない):

- 数字1文字: `"0"`〜`"9"`
- アルファベット1文字: `"A"`〜`"Z"`
- ファンクションキー: `"F1"`〜`"F24"`
- 十字キー用: `"LEFT"` / `"UP"` / `"RIGHT"` / `"DOWN"`
- その他: `"SPACE"` `"ENTER"` `"TAB"` `"ESCAPE"` `"BACKSPACE"` `"LSHIFT"` `"RSHIFT"` `"LCTRL"` `"RCTRL"` `"LALT"` `"RALT"` `"HOME"` `"END"` `"PAGEUP"` `"PAGEDOWN"` `"INSERT"` `"DELETE"`

存在しないキー名を書いた場合、そのキーだけビルトインの既定値にフォールバックし、`logs/run.log`に警告が出る(ドライバが落ちることはない)。

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

**実機検証状況(2026-07-21)**: `off`/`static`/`spectrum`は目視確認済み。`breathing`/`wave`/`reactive`は同じコマンド形状のため動作する可能性が高いが未個別確認。期待通りに光らない場合は`logs/run.log`の`Lighting: ...`行、および`WARNING: failed to send lighting ...`が出ていないか確認。per-key単位で1キーずつ違う色を指定する機能は未実装。

### 6. ハイパーシフト(「Hyper Response」ボタン)

**v1.0.5で新規**: 本体左上の「Hyper Response」ボタン(D-padの隣、Razer公式の名称は"D")の動作は、`config.toml`の`[hypershift]`セクション(または`configui`の「ハイパーシフト」パネル)で設定できるようになった:

| `mode` | 動作 |
|---|---|
| `"layer_switch"`(既定) | このボタンで`[keys.default]`/`[keys.layer1]`/`[keys.layer2]`のどれが有効かを切り替える — 詳細は下の`switch_style`を参照 |
| `"modifier_key"` | レイアウト切替は一切行わず、ボタンの押下/解放時に`modifier_key`(既定`LALT`)をそのまま送信する、通常のキーと同じ動作になる |

`mode = "layer_switch"`のとき、`switch_style`で切替方式を選べる:

| `switch_style` | 動作 |
|---|---|
| `"momentary"`(既定・v1.0.0〜v1.0.4までと同じ挙動) | 押している間はLayer1、離すと通常レイヤーに戻る。常に2レイアウトのみ — `layer_count`は無視される |
| `"toggle"` | 押すたびに1レイヤーずつ進み、末尾で先頭に戻る(`layer_count = 2`: 通常⇔Layer1、`layer_count = 3`: 通常→Layer1→Layer2→通常→…)。離す動作は何もしない |

通常レイヤー以外が有効な間、Alt自体はOSに送られない(元のボタンがAltキーコードを送る仕様のため、押下/解放の検知には使うが`mode`/`switch_style`によらず意図的にブロックしている)。

**2026-07-21修正**: 以前はこのAlt検知がソースデバイスを判別しない`WH_KEYBOARD_LL`フックで実装されていたため、`tartarus_driver`が動いている間は実キーボードのAlt(Alt+Tab、Alt+F4等)も一緒にOSに届かなくなる問題があった。現在はD-pad同様Interception経由のデバイス判別処理に統一されており、**Tartarus本体のAlt(Hyper Response)だけが対象になる。実キーボードのAlt+Tab等は影響を受けない**(実機確認済み)。

**既知の制約(Interception未インストール時のみ)**: Interceptionドライバが利用できない場合に限り、旧`WH_KEYBOARD_LL`フックへ自動的にフォールバックする。この場合はデバイス判別ができないため、上記の「実キーボードのAltも道連れでブロックされる」制約が復活する(起動時ログに`falling back to the hook-based Hypershift detection`と出ていれば該当)。Interceptionを未インストールのままにする理由がなければ、`README.md`の手順でインストールしておくことを推奨。

### 7. アンチチート導入済みのゲームについて

一部のゲームでは本ドライバの入力が全く効かない、またはドライバを動かしたままだとゲーム自体が起動しないことがある — **実機で確認済み**: Valorant(Riot Vanguard)は問題なく動作するが、Apex Legends(Easy Anti-Cheat)は本ドライバからの入力を一切受け付けない。管理者権限にしても変わらない。これは本ドライバの不具合ではなく、意図的なアンチチートの仕様: `SendInput`(本ドライバ、および事実上全てのリマップ/マクロツールが使うキー送信の仕組み)にはOSレベルで「合成された入力」であることを示す情報が付随しており、一部のアンチチートエンジンはこれを検知して意図的に無視する。Interceptionのようなサードパーティ製カーネルドライバ自体が、一部のエンジンの起動ブロック判定のトリガーになったという報告もある。EAC保護下のタイトルに対する確実な回避策は無い(意図的なアンチチート設計のため。詳細とアカウントリスクについての注意はREADME.mdの「既知の制約」参照)。ハードウェア変換アダプタや仮想コントローラー方式への切り替えも安全な代替にはならない — 技術的に検知されるかどうかとは別に、一部のタイトルでは検知有無を問わずポリシーとしてこの種のデバイスを禁止している(例: Apex LegendsはRespawnが2026年3月よりCronus Zen/Titan Two系アダプタを恒久・異議申し立て不可のBAN対象と明言)。特定のゲームだけ本ドライバに反応しない場合、ほぼ確実にこれが原因であり、それ以上デバッグしても解決しない。

### 8. トラブルシューティング

| 症状 | 確認すること |
|---|---|
| アナログキーが何も反応しない | Synapseのタスクトレイアイコンが本当に閉じているか確認(`Get-Process \| Where ProcessName -match 'Razer\|Synapse'`でGUIプロセスが出ないこと)。デバイスの抜き差しも有効な場合がある |
| メモ帳など大抵のアプリでは動くが特定のゲームだけ反応しない | そのゲームのアンチチートが合成入力をブロックしている可能性が高い — 上記7を参照。本ドライバ側での解決策は無い |
| 十字キー/ホイールが元の矢印キー・スクロールとしても動いてしまう(二重入力) | 起動時ログに`WARNING: interception.dll could not be loaded`または`Interception::new() returned None`と出ていないか確認。前者は多くの場合、カーネルドライバは入っているが`interception.dll`自体が`tartarus_driver.exe`と同じフォルダにコピーされていない状態(README「既知の制約」の手順4)。カーネルドライバのインストールだけではエラーが出ないため見落としやすい |
| `configui`で保存したのに反映されない | 約1秒待つ — 動作中のドライバは`config.toml`の変更を自動で拾う。`logs/run.log`に`config.toml reloaded`(構文エラーがある場合は`WARNING: ... keeping the previous settings`)が出ているか確認 |
| `tray`モードでタスクトレイにアイコンが出ない | `logs/run.log`に`[tray] WARNING: ...`が出ていないか確認(ウィンドウクラス登録・ウィンドウ作成・アイコン追加のいずれかの失敗)。Explorerの再起動直後などは再度`tray`を起動し直す |
| `config.toml`の一部のキーだけ既定値のままになる | `logs/run.log`に`WARNING: config.toml [...] は認識できないキー名です`が出ていないか確認。使えるキー名の一覧は本ファイルの3節を参照 |
| 実キーボード/実マウスの動きが変になった | Interception/フックはTartarus Pro(VID 0x1532/PID 0x0244)のハードウェアID一致だけを対象にしている(fail-open設計)。それでも問題が起きた場合はバグなので、`logs/run.log`を保存して調査する |
| ライティングを設定したのに光り方が変わらない | `logs/run.log`に`Lighting: effect set to "..."`が出ているか確認(出ていなければ`[lighting]`の`effect`が`"none"`のまま、または設定が読み込まれていない)。`WARNING: failed to send lighting ...`が出ていればHID書き込み自体が失敗している |

