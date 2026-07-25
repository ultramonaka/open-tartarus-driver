# Razer Tartarus Pro — Reverse-Engineering Notes

**[English](#english)** | **[日本語](#japanese)**

---

<a name="english"></a>
## English

This is a write-up of the protocol reverse-engineering behind `tartarus_driver` — how the Razer Tartarus Pro's analog keys, RGB lighting, and profile-indicator LEDs were figured out with no official documentation, and a separate investigation into why this driver's input doesn't work in anti-cheat-protected games like Apex Legends. It's shared in case it's useful to anyone else working with this device, or a similar Razer analog-input product, or just curious how a project like this comes together. See `README.md`/`USAGE.md` for how to actually use the driver.

### Contents

1. [Device basics and HID interfaces](#1-device-basics-and-hid-interfaces)
2. [Enabling analog streaming without Synapse](#2-enabling-analog-streaming-without-synapse)
3. [RGB lighting protocol](#3-rgb-lighting-protocol)
4. [Profile indicator LEDs — an open question](#4-profile-indicator-leds--an-open-question)
5. [Why doesn't this work in Apex Legends?](#5-why-doesnt-this-work-in-apex-legends)

---

### 1. Device basics and HID interfaces

VID `0x1532` / PID `0x0244` identifies a Razer Tartarus Pro. (Watch out: the Tartarus V2 is `0x0217` and the original Tartarus is `0x010B` — easy to mix up if you're searching for prior art.)

There's essentially no official protocol documentation, and OpenRazer — the most complete open-source Razer driver project — only implements lighting for this device, not the analog key data itself. Its Report Descriptor is recorded as unavailable in several of its own GitHub issues. Getting anywhere required capturing and manually decoding raw USB traffic.

The device exposes 3 HID interfaces:

| Interface | Endpoint | Purpose |
|---|---|---|
| 0 | `0x81` | Boot keyboard — D-pad and the "Hyper Response" thumb button come through here as ordinary keycodes. Windows reserves this collection; user-mode HID libraries can't open it. |
| **1** | **`0x82`** | **The analog key data.** Report ID `0x06`, and `byte[1]`..`byte[20]` are depth values (`0x00`–`0xFF`) that map 1:1 to the key numbers printed on the keycaps — confirmed by pressing keys one at a time in a controlled test and watching which byte moved. |
| 2 | `0x83` | Razer's proprietary control channel ("Razer Control Device"). Quirk: it enumerates under HID usage_page `0x0001` / usage `0x0002`, the same usage as a generic mouse — an easy way to accidentally filter it out if you're excluding boot-mouse collections by usage. |

The D-pad, scroll wheel, and wheel-click are out of scope for this driver — they arrive as completely standard boot keyboard/mouse input (arrow keys, a normal wheel, a middle button) and already work in Windows without any Synapse or custom driver involvement.

---

### 2. Enabling analog streaming without Synapse

Interface 1 stays silent — zero analog reports — unless something first tells the device to enter "streaming mode." Naively, it looks like Razer Synapse itself has to be running for this to work, which would defeat the entire point of a Synapse-free driver.

It doesn't. Capturing USB traffic around Synapse's own startup (quit Synapse completely, start a capture, then launch it) shows it sending a single command to Interface 2 early on:

| Offset | Field | Value |
|---|---|---|
| `[0]` | status | `0x00` |
| `[1]` | transaction_id | any (e.g. `0x01`) |
| `[2:4]` | remaining_packets (big-endian u16) | `0x0000` |
| `[4]` | protocol_type | `0x00` |
| `[5]` | data_size | `0x02` |
| `[6]` | command_class | `0x00` |
| `[7]` | command_id | `0x04` (set device mode) |
| `[8]` | arg0 = mode | `0x03` (streaming/driver mode; `0x00` returns to normal mode) |
| `[9]` | arg1 | `0x00` |
| `[10:88]` | unused | `0x00` |
| `[88]` | CRC (XOR of `[2..88]`) | computed |
| `[89]` | reserved | `0x00` |

Send this once as a Feature Report to Interface 2, and a few milliseconds later Interface 1 emits one "standby" report (all depths zero), after which it streams real data on every keypress from then on — no need for Synapse to stay running, and no periodic heartbeat required.

This matches an independent report on a completely different Razer analog keyboard (OpenRazer PR #1868: "device mode `0x03` enables it"), which was reassuring confirmation that this wasn't device-specific luck.

One documented risk worth knowing about: OpenRazer's own Tartarus Pro support PR (#2710) reports that sending this same command can trigger a firmware reset / reconnect loop on some units, which is why OpenRazer disabled driver mode for this specific device entirely. That failure mode has never been observed in extensive testing here — possibly a firmware revision difference — but it's worth being aware of if you hit unexplained disconnects building something similar.

---

### 3. RGB lighting protocol

Once streaming mode is enabled, the same Interface 2 control channel also accepts lighting commands, using `command_class = 0x0F` ("extended matrix" in Razer's own driver source) and a fixed `transaction_id = 0x1F` (older Tartarus models use `0xFF` — a frequent source of confusion when cross-referencing OpenRazer source for other devices).

| Effect | Cmd | data_size | Args |
|---|---|---|---|
| Off | `0x02` | `0x06` | `[0]`=VARSTORE(`0x01`), `[1]`=BACKLIGHT_LED(`0x05`), `[2]`=`0x00` |
| Static color | `0x02` | `0x09` | `[2]`=`0x01`, `[5]`=`0x01`, `[6-8]`=R,G,B |
| Breathing (1 color) | `0x02` | `0x09` | `[2]`=`0x02`, `[3]`=`0x01`, `[5]`=`0x01`, `[6-8]`=R,G,B |
| Spectrum | `0x02` | `0x06` | `[2]`=`0x03` |
| Wave | `0x02` | `0x06` | `[2]`=`0x04`, `[3]`=direction, `[4]`=speed |
| Reactive | `0x02` | `0x09` | `[2]`=`0x05`, `[4]`=speed, `[6-8]`=R,G,B |
| Brightness | `0x04` | `0x03` | `[2]`=brightness (0-255) |

(All args after `[0]`/`[1]` = VARSTORE/BACKLIGHT_LED, unless noted otherwise; full byte-level detail including breathing's dual-color and starlight variants, and the per-key custom-frame upload command, is more than fits here.) `off`/`static`/`spectrum` are confirmed working against real hardware; the others share the same command shape and very likely work but haven't been individually checked yet.

Per-key arbitrary color (uploading a full 21-key RGB frame instead of one effect for the whole board) is a separate, larger command (`command_id = 0x03`, 63 bytes of RGB data) that's documented but not yet implemented in this driver.

---

### 4. Profile indicator LEDs — an open question

The Tartarus Pro has 3 small fixed-color indicator LEDs on its side (red/green/blue), separate from the main per-key backlight, presumably meant for things like showing which profile is active. The command protocol for controlling them (`command_class = 0x03`, individual on/off/effect/brightness commands per LED ID `0x0C`/`0x0D`/`0x0E`) is fully documented in Razer's own driver source — for *other*, similar devices (Tartarus Chroma, Tartarus V2, Orbweaver). OpenRazer has never implemented it specifically for the Tartarus Pro, and its own merge notes say support is simply missing.

Sending the documented commands to a real Tartarus Pro — with either of the two plausible `transaction_id` values (`0xFF`, matching sibling devices, or `0x1F`, matching this device's main lighting) — completes without any error from the device, but produces no visible change. Whether these side LEDs are even physically present and wired up on this particular unit is genuinely unclear. This is left as an open question rather than something actively being pursued further.

---

### 5. Why doesn't this work in Apex Legends?

Separately from the protocol work above: this driver sends keystrokes via Windows' `SendInput` API, the same mechanism virtually every keyboard remapping/macro tool uses. Some anti-cheat engines specifically detect and discard input carrying `SendInput`'s "synthetic input" marker. Confirmed on real hardware: Valorant (Riot Vanguard) accepts this driver's input normally; Apex Legends (Easy Anti-Cheat) does not, with or without administrator privileges.

Out of curiosity about how Razer's own Synapse software avoids this, we read through its actual installed driver package on a real machine (signed kernel driver `RzDev_0244.sys` and its INF files). It turns out Synapse doesn't use `SendInput` either — it installs a **signed kernel-mode filter driver directly onto the Tartarus Pro's real USB interfaces**, and from there synthesizes several virtual devices, including (per the installer's own registry flags) a **virtual Xbox 360 controller**. From Windows' point of view, that's indistinguishable from genuine hardware I/O — a fundamentally different, much lower-level path than `SendInput`.

Building an equivalent for this project — our own signed kernel driver — isn't realistic for a hobby open-source project (EV code-signing certificates and WDK driver development have real, ongoing costs). As a cheaper approximation, we prototyped routing input through [ViGEmBus](https://github.com/ViGEm/ViGEmBus), an existing, widely-used open-source virtual-controller framework (the same one tools like DS4Windows rely on), to create a virtual Xbox 360 controller and send button presses through it instead of through the keyboard. It worked perfectly at the OS level — Windows enumerated it as a completely normal Xbox 360 controller, and its button state reached the real `XInputGetState` API that games use to read controller input.

It still didn't work in Apex Legends. With the game's own controller-rebinding screen in its most permissive state (actively waiting for "any" button press), a button held via the virtual controller produced no reaction at all.

We also checked how established commercial tools fare here, rather than trying to out-engineer this ourselves. reWASD — a mature, actively-maintained commercial remapping tool with the same kind of virtual-controller feature — has ongoing, unresolved reports of exactly this kind of anti-cheat recognition trouble in its own community forums; even a dedicated commercial team hasn't solved it. And hardware conversion adapters (Cronus Zen, Titan Two, and similar devices, which impersonate a controller at the USB hardware level rather than through any software driver at all) are, as of a March 2026 policy update from Apex Legends' publisher, explicitly banned outright — permanently, without appeal — regardless of whether they're technically detected.

None of the three approaches we're aware of (`SendInput` keystrokes, a ViGEmBus virtual controller, or hardware adapters) reliably and safely gets input into an EAC-protected game like Apex Legends. Going further from here — spoofing device identity, impersonating a trusted signed driver — crosses from "legitimate remapping tool" into anti-cheat-evasion engineering, which isn't something this project is interested in pursuing. In practice: this driver works fine in Valorant and most other games; for Apex Legends specifically, the recommendation is simply to quit it before playing.

---

<a name="japanese"></a>
## 日本語

`tartarus_driver`を作る過程で行った、Razer Tartarus Proのプロトコルリバースエンジニアリングの記録です。公式ドキュメントが一切無い状態から、アナログキー・RGBライティング・プロファイルインジケータLEDの挙動をどう突き止めたか、そして本ドライバの入力がApex Legendsのようなアンチチート導入ゲームでなぜ効かないのかを調べた、別件の調査もまとめています。同じデバイス、あるいは類似のRazerアナログ入力製品を触っている方の参考になれば、また単純に開発の過程に興味がある方に向けて公開します。実際の使い方は`README.md`/`USAGE.md`を参照してください。

### 目次

1. [デバイス基本情報とHIDインターフェース](#1-デバイス基本情報とhidインターフェース)
2. [Synapse無しでアナログストリーミングを有効化する](#2-synapse無しでアナログストリーミングを有効化する)
3. [RGBライティングプロトコル](#3-rgbライティングプロトコル)
4. [プロファイルインジケータLED — 未解決の課題](#4-プロファイルインジケータled--未解決の課題)
5. [なぜApex Legendsでは動かないのか](#5-なぜapex-legendsでは動かないのか)

---

### 1. デバイス基本情報とHIDインターフェース

VID `0x1532` / PID `0x0244`がRazer Tartarus Proの識別子です(Tartarus V2は`0x0217`、無印Tartarusは`0x010B`と別PIDなので、前例を探す際は混同注意)。

公式のプロトコルドキュメントは実質皆無で、最も充実したオープンソースのRazerドライバであるOpenRazerでさえ、この機種についてはライティング制御のみを実装しており、アナログキーデータ自体は扱っていません。Report Descriptorも複数のOpenRazer自身のissueで「取得不可」と記録されています。結局、生のUSB通信をキャプチャして手作業で解析するしかありませんでした。

デバイスは3つのHIDインターフェースを持ちます:

| Interface | エンドポイント | 用途 |
|---|---|---|
| 0 | `0x81` | ブートキーボード — 十字キーと「Hyper Response」サムボタンが普通のキーコードとしてここから出てくる。Windowsが予約しているcollectionで、ユーザーモードのHIDライブラリからは開けない |
| **1** | **`0x82`** | **アナログキーのデータ本体。** レポートID`0x06`、`byte[1]`〜`byte[20]`がキーキャップ印字の番号と1:1対応する深度値(`0x00`〜`0xFF`) — 1つずつキーを押す制御されたテストでどのバイトが動くかを確認して確定 |
| 2 | `0x83` | Razer独自の制御チャンネル("Razer Control Device")。癖: HID usage_page `0x0001` / usage `0x0002`で列挙され、これは一般的なマウスと同じusageなので、ブートマウスcollectionをusageで除外するフィルタを書くと誤って一緒に除外してしまいやすい |

十字キー・スクロールホイール・ホイールクリックは本ドライバのスコープ外です — これらは標準的なブートキーボード/マウス入力(矢印キー・普通のホイール・中ボタン)としてそのまま届き、Synapseや独自ドライバなしでもWindows上ですでに機能しています。

---

### 2. Synapse無しでアナログストリーミングを有効化する

Interface 1は、何かが先に「ストリーミングモードに入れ」と伝えない限り沈黙したまま(アナログレポートが一切流れない)です。素朴に考えると、これにはRazer Synapse自体が起動している必要があるように見え、そうだとするとSynapse不要ドライバという目的そのものが崩れてしまいます。

実際にはそうではありませんでした。Synapse自身の起動シーケンス周辺のUSB通信をキャプチャする(Synapseを完全終了→キャプチャ開始→起動)と、起動直後にInterface 2へ以下のコマンドを1回送っていることが分かります:

| オフセット | 内容 | 値 |
|---|---|---|
| `[0]` | status | `0x00` |
| `[1]` | transaction_id | 任意(例: `0x01`) |
| `[2:4]` | remaining_packets(BE u16) | `0x0000` |
| `[4]` | protocol_type | `0x00` |
| `[5]` | data_size | `0x02` |
| `[6]` | command_class | `0x00` |
| `[7]` | command_id | `0x04`(デバイスモード切替) |
| `[8]` | 引数0 = モード | `0x03`(ストリーミング/ドライバモード。`0x00`で通常モードに戻る) |
| `[9]` | 引数1 | `0x00` |
| `[10:88]` | 未使用 | `0x00` |
| `[88]` | CRC(`[2..88]`のXOR) | 計算値 |
| `[89]` | reserved | `0x00` |

これをFeature ReportとしてInterface 2に1回送ると、数ミリ秒後にInterface 1から「待機用」レポート(深度全ゼロ)が1回届き、以降はキーを押すたびに実データがストリーミングされます。Synapseを起動し続ける必要も、定期的なハートビート送信も不要です。

これは全く別のRazerアナログキーボードについての独立した報告(OpenRazer PR #1868: 「device mode `0x03`で有効化される」)とも一致しており、この機種固有の偶然ではないという裏付けになりました。

知っておく価値のあるリスクが1つあります: OpenRazer自身のTartarus Pro対応PR(#2710)は、この同じコマンド送信が一部の個体でファームウェアリセット/再接続ループを引き起こすと報告しており、これが理由でOpenRazerはこの機種のドライバモード対応自体を諦めています。この現象は今回の実機での広範なテストでは一度も発生していません(ファームウェアのリビジョン差の可能性)が、同様のものを自作する際に原因不明の切断に遭遇したら、まずこれを疑う価値があります。

---

### 3. RGBライティングプロトコル

ストリーミングモードを有効化した後、同じInterface 2の制御チャンネルはライティングコマンドも受け付けます。`command_class = 0x0F`(Razer自身のドライバソースでは"extended matrix"と呼ばれる)と、固定の`transaction_id = 0x1F`を使います(旧Tartarusシリーズは`0xFF`を使うため、他機種向けのOpenRazerソースを参照する際に混同しがちです)。

| エフェクト | Cmd | data_size | 引数 |
|---|---|---|---|
| 消灯 | `0x02` | `0x06` | `[0]`=VARSTORE(`0x01`), `[1]`=BACKLIGHT_LED(`0x05`), `[2]`=`0x00` |
| 単色 | `0x02` | `0x09` | `[2]`=`0x01`, `[5]`=`0x01`, `[6-8]`=R,G,B |
| 呼吸(単色) | `0x02` | `0x09` | `[2]`=`0x02`, `[3]`=`0x01`, `[5]`=`0x01`, `[6-8]`=R,G,B |
| スペクトラム | `0x02` | `0x06` | `[2]`=`0x03` |
| ウェーブ | `0x02` | `0x06` | `[2]`=`0x04`, `[3]`=方向, `[4]`=速度 |
| リアクティブ | `0x02` | `0x09` | `[2]`=`0x05`, `[4]`=速度, `[6-8]`=R,G,B |
| 明るさ | `0x04` | `0x03` | `[2]`=明るさ(0-255) |

(`[0]`/`[1]`はいずれもVARSTORE/BACKLIGHT_LED固定。呼吸の2色版・スターライト系のバリエーション、per-keyカスタムフレーム転送コマンドまでの完全なバイト単位の詳細はここには収まりきりません。)`off`/`static`/`spectrum`は実機で動作確認済み、それ以外は同じコマンド形状のため動作する可能性が高いものの個別確認はまだです。

per-key単位で任意の色を指定する機能(盤面全体1つのエフェクトではなく、21キー分のRGBフレームを丸ごとアップロードする)は別の大きめのコマンド(`command_id = 0x03`、63バイトのRGBデータ)としてプロトコル自体は判明していますが、本ドライバではまだ実装していません。

---

### 4. プロファイルインジケータLED — 未解決の課題

Tartarus Proの側面には、メインのキー照明とは別に、赤/緑/青の小さな単色インジケータLEDが3つあり、おそらくプロファイルの表示などを想定したものと思われます。これを制御するコマンドプロトコル(`command_class = 0x03`、LED ID `0x0C`/`0x0D`/`0x0E`ごとの個別on/off/エフェクト/明るさコマンド)は、Razer自身のドライバソースに*他の*類似機種(Tartarus Chroma、Tartarus V2、Orbweaver)向けとして完全に文書化されています。しかしOpenRazerはTartarus Proについてはこの機能を実装しておらず、マージ済みのPRのノート自体が「サポートが単に欠けている」と明言しています。

実際のTartarus Proに、ドキュメント通りのコマンドを、考えられる2つの`transaction_id`候補(兄弟機種と同じ`0xFF`、またはこの機種のメイン照明と同じ`0x1F`)のどちらで送っても、デバイス側からエラーは返らず処理は成功しますが、見た目には何も変化しません。この側面LEDがそもそもこの個体に物理的に実装され配線されているのかどうか自体、正直なところ判然としません。これ以上積極的に追及するのではなく、未解決の課題として残しています。

---

### 5. なぜApex Legendsでは動かないのか

上記のプロトコル調査とは別件です: 本ドライバはWindowsの`SendInput` API(キーボードリマップ・マクロツールのほぼ全てが使う仕組み)でキー入力を送信していますが、一部のアンチチートエンジンは`SendInput`が付与する「合成入力」マーカーを検知して意図的に無視します。実機で確認済み: Valorant(Riot Vanguard)は本ドライバの入力を正常に受け付けますが、Apex Legends(Easy Anti-Cheat)は管理者権限の有無にかかわらず受け付けません。

Razer自身のSynapseがなぜこれを回避できているのか興味があったので、実機にインストールされている実際のドライバ一式(署名済みカーネルドライバ`RzDev_0244.sys`とそのINFファイル)を読んでみました。すると、Synapseも`SendInput`を使っていないことが分かりました — 代わりに、**署名済みのカーネルモードフィルタドライバをTartarus Pro本体の実USBインターフェースに直接インストール**し、そこから複数の仮想デバイスを合成しています。インストーラー自身のレジストリフラグによれば、その中には**仮想Xbox 360コントローラー**も含まれます。Windowsから見るとこれは本物のハードウェア入出力と区別が付かず、`SendInput`とは根本的に別の、はるかに低レベルな経路です。

この構成を自分たちのプロジェクトで再現する(独自の署名済みカーネルドライバを作る)のは、趣味のオープンソースプロジェクトとしては現実的ではありません(EV code signing証明書やWDKでのドライバ開発には相応の継続的コストがかかります)。より安価な近似として、既存の広く使われているオープンソースの仮想コントローラーフレームワーク[ViGEmBus](https://github.com/ViGEm/ViGEmBus)(DS4Windowsなどのツールも使っている)を使い、キーボードの代わりに仮想Xbox 360コントローラー経由でボタン入力を送る、というプロトタイプを試しました。OSレベルでは完璧に動作し、Windowsはこれを全く普通のXbox 360コントローラーとして認識し、ボタン状態はゲームが実際に使う`XInputGetState` APIにも正しく届きました。

それでもApex Legendsでは動きませんでした。ゲーム自身のコントローラー割り当て画面を、あらゆる入力を積極的に拾おうとする最も緩い状態にした上で、仮想コントローラー経由でボタンを押しっぱなしにしても、ゲーム側は一切反応しませんでした。

自分たちで無理に工夫するのではなく、既存の商用ツールがこの問題をどう扱っているかも確認しました。reWASD — 同種の仮想コントローラー機能を持つ、成熟した商用リマップツール — の公式コミュニティフォーラムには、全く同じ種類のアンチチート認識トラブルについての未解決の報告が継続的に上がっており、専任チームを抱える商用製品でも解決していません。また、ハードウェア変換アダプタ(Cronus Zen、Titan Two等、ソフトウェアドライバを介さずUSBハードウェアレベルでコントローラーになりすます方式)は、2026年3月のApex Legends運営元のポリシー更新により、技術的に検知されるかどうかに関わらず、恒久的かつ異議申し立て不可で明確に禁止されています。

私たちが把握している3つの方式(`SendInput`によるキー入力・ViGEmBus仮想コントローラー・ハードウェアアダプタ)のいずれも、Apex LegendsのようなEAC保護下のゲームに確実かつ安全に入力を通す方法にはなりませんでした。ここからさらに踏み込む(デバイス識別情報を偽装する、署名済みの信頼されたドライバになりすます、等)ことは、正当なリマップツールの範囲を超えてアンチチート回避エンジニアリングに入り込むため、本プロジェクトが追求する方向ではありません。実用上は、本ドライバはValorantをはじめほとんどのゲームで問題なく動作します。Apex Legendsに限っては、プレイ前に本ドライバを終了しておくことをお勧めします。
