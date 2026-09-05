## 目標

配信者向けのGUIソフトウェアです。

マイクが拾ってしまう雑音（キーボード、マウス、ゲームコントローラ、空調）をリダクションし、
配信者の声だけを選択的に出力します。

Windows/Linuxで動作します。

どのCPU、GPUを利用するか選択することが出来ます。
組み込みGPUも利用することが出来ます。
ノイズリダクションをしないパススルーも選択可能。

どのマイク音声をキャプチャするか選択することが出来ます。
Windows版はVB-CABLEを仮想マイクとして利用します。NoiseHoiHoiは`CABLE Input (VB-Audio Virtual Cable)`へ処理済み音声を書き、OBSなど録音ソフトは`CABLE Output (VB-Audio Virtual Cable)`を選択します。

チューニングは出来ず、デフォルトのままで高い性能を発揮することを目指します。

デバグ画面を見て、ノイズに対して本当にノイズリダクション出来ているか
目視で確認することも出来ます。

## プロジェクト構成

- Cargo.toml // Workspace
- NoiseNet // DeepFilterNetのburnでの実装
  - crates
- NoiseHoiHoi // NoiseNetを使った製品
  - crates

## NoiseNet

NoiseNetは、ノイズリダクションネットのburn実装です。
推論のみを行います。
初期には、DeepFilterNet3のみを実装します。

単体でテスト出来るようにすること。
これは、サンプル音声音源に対してキーボード音などを重畳したものがちゃんとリダクション出来るかをテストする。

CUDA/ROCm/WGPUのバックエンドで動作可能にすること。
これはburnがCubeCL基盤であることを考えれば可能なはず。

## NoiseHoiHoi

GUIはeguiを使う。

生成物は、out/に配置する。
（例: NoiseHoiHoi-v0.1.exe）

### モック

GUIのモックはdoc/GUI-mock.jpg

### デバグ画面

ボタンを押すと、信号モニター画面が現れます。
信号モニター画面では、入力、出力、入力と出力の差が波形として可視化されます。

## 開発計画

### v0.1

Windowsに限定。

マイクの音声を拾い、それをパススルーしてVB-CABLEの入力側へ出力する。

Windowsインストーラは公式VB-CABLEパッケージを同梱し、未導入の場合だけサイレントインストールする。独自のカーネルドライバは実装しない。

ノイズリダクションの選択項目はOFFで固定する。

NoiseNetの前段にresamplingが必要ならば、入れてもいい。

### v0.2

デバグ画面の実装をする。

### v0.3

NoiseNetを使ってフィルタリングを行う。
この時点でNoiseNetが完成している必要がある。

ただし、CPUを利用する。
CPU版は、WGPUのCPUエミュレーションを使う（burnのWgpuDevice::Cpuを使う）。

この時点でノイズリダクションはON/OFFが出来るようになる。

### v0.4

GPUによるリダクションが出来るようになる。

リダクションをONにトグルすると、
- processor: GPU、CPUの名前（例: RTX5070）
- runtime: processorに応じて選択可能なものが出る。この時点ではCUDA/ROCmは実装せず、WGPUのみ。（例: WGPU）
が選択出来るようになる。
