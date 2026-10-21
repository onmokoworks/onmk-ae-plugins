# Particle Kit パラメータ仕様

この文書は、After Effectsエフェクト「Particle Kit」のクラシックUIに表示されるパラメータと、その実際の挙動をまとめたものです。

- 表示名: `Particle Kit`
- Match Name: `ParticleKit`
- 数値の「範囲」は入力可能な範囲です。スライダー表示範囲はそれより狭い場合があります。
- `Variation` は、特記がなければ基準値に対するパーティクル単位のランダム変化率です。
- 既存AEプロジェクトとの互換性を守るため、ホストパラメータは追加のみを原則とします。開発上の制約は [PARAMETER_COMPATIBILITY.md](./PARAMETER_COMPATIBILITY.md) を参照してください。

## Presets

| パラメータ | 内容 |
|---|---|
| Save Preset | 現在の設定をJSONプリセットとして保存します。 |
| Load Preset | 保存済みプリセットを読み込みます。 |
| Delete Preset | 保存済みプリセットを削除します。 |
| Open Folder | プリセット保存フォルダを開きます。 |

## Emitter

| パラメータ | 範囲 / 選択肢 | 初期値 | 内容 |
|---|---|---:|---|
| Emitter Type | Point / Box / Sphere / Grid / Layer Alpha / Path | Point | 発生源の形状を選びます。 |
| Emit Mode | Continuous / All at Start | Continuous | 継続発生か、開始時の一括発生かを選びます。 |
| Position | AEポイント | 50%, 50% | エミッターのXY位置です。 |
| Position Z | -10000–10000 | 0 | エミッターのZ位置です。 |
| Emitter Source Layer | レイヤー | なし | Layer AlphaおよびPathの参照元です。 |
| Refresh Image Cache | ボタン | — | 参照画像のキャッシュを更新します。 |
| Path Density | 1–100 | 10 | Pathを発生源にした場合のサンプリング密度です。 |
| Uniform Box Size | On / Off | On | BoxのX・Y・Zサイズを連動させます。 |
| Emitter Size X/Y/Z | 0–10000 | 0 | Box、Sphere、Gridの発生領域サイズです。 |
| Grid Res X/Y | 1–50 | 8 | GridのX・Y方向の個数です。 |
| Grid Res Z | 1–10 | 1 | GridのZ方向の個数です。 |
| Birth Rate | 0–10000 | 180 | Continuousでは1秒あたりの発生数です。All at Startでは下記の特別ルールに従います。 |
| Lifespan (sec) | 0.01–120 | 1.6 | 親パーティクルの寿命です。 |
| Lifespan Variation | 0–1 | 0.15 | 寿命のランダム変化率です。 |

### Emit Modeの挙動

- `Continuous`: `Birth Rate`を1秒あたりの発生数として、時間に沿って生成します。
- `All at Start` + Point / Box / Sphere / Layer Alpha / Path: 時刻0で、`Birth Rate`の値を総数として一度だけ生成します。
- `All at Start` + Grid: 時刻0で、`Grid Res X × Grid Res Y × Grid Res Z`個を一度だけ生成します。この組み合わせでは`Birth Rate`は使用しません。
- 開始後に生成されたパーティクルは、それぞれの`Lifespan`を超えると消えます。

## Motion

| パラメータ | 範囲 | 初期値 | 内容 |
|---|---:|---:|---|
| Speed | 0–5000 | 240 | 発生時の初速です。 |
| Speed Variation | 0–1 | 0 | 初速のランダム変化率です。 |
| Direction X | -1–1 | 0 | 初期進行方向のX成分です。 |
| Direction Y | -1–1 | -1 | 初期進行方向のY成分です。 |
| Direction Z | -1–1 | 0 | 初期進行方向のZ成分です。 |
| Spread (deg) | 0–180 | 18 | 初期進行方向のばらつき角度です。 |
| Particle Size | 0.1–500 | 9 | 発生時の基本サイズです。 |
| Size Variation | 0–1 | 0.2 | 基本サイズのランダム変化率です。 |
| Initial Rotation | 0–360 | 0 | 発生時の回転角です。 |
| Rotation Speed | -1000–1000 | 0 | 回転速度です。負値で逆回転します。 |
| Rotation Variation | 0–360 | 0 | 発生時の回転角に加えるランダム幅です。 |

`Direction X/Y/Z`がすべて0の場合は、内部的に上方向を使用します。方向ベクトルは正規化されるため、成分の大きさではなく比率が向きを決めます。

## Physics

| パラメータ | 範囲 | 初期値 | 内容 |
|---|---:|---:|---|
| Gravity | -2000–2000 | 160 | Y方向の加速度です。負値で反対方向になります。 |
| Wind X / Y | -2000–2000 | 0 | X・Y方向の加速度です。 |
| Turbulence | 0–1000 | 12 | Turbulenceが加える移動速度の大きさです。 |
| Turbulence Scale | 0.01–100 | 0.75 | 空間的な流れのスケールです。値が大きいほど近い位置の粒子が似た方向へ流れます。 |
| Turbulence Speed | 0–10 | 1 | 流れの方向が時間変化する速さです。0ではTurbulenceによる移動を停止します。 |
| Air Resistance | 0–20 | 0.3 | 初速を時間経過で減衰させます。 |
| Bounce | On / Off | Off | 床面での反射を有効にします。 |
| Bounce Damping | 0–1 | 0.5 | 反射後に保持する速度の割合です。 |

### Turbulenceの仕様

Turbulenceは加速度ではありません。一定の速度成分として働き、粒子を前へ進めながら進行方向を滑らかに変化させます。

- 経過時間に比例してTurbulence速度が増えることはありません。
- 発生地点へ周期的に引き戻す往復運動ではありません。
- 出生地点からの総移動距離は時間とともに増えますが、1秒あたりの移動量は増加しません。
- `Gravity`と`Wind`は従来どおり加速度なので、それらを有効にした場合は全体速度が変化します。

## Appearance

| パラメータ | 範囲 / 選択肢 | 初期値 | 内容 |
|---|---|---:|---|
| Color Mode | Single / Gradient | Gradient | 単色または寿命に沿った色補間を選びます。 |
| Color Start / End | 色 | 白 | 発生時と寿命末端の色です。 |
| Opacity Variation | 0–1 | 0 | 発生時の不透明度を粒子ごとに変化させます。 |
| Opacity Curve | Custom / Constant / Fade Out / Fade In-Out / Ease Out / Quick Fade | Fade Out | 寿命に沿った不透明度カーブです。 |
| Opacity Start / 33% / 66% / End | 0–100% | 100 / 90 / 45 / 0 | Custom時の4点カーブです。 |
| Size Curve | Custom / Constant / Shrink / Grow-Shrink / Grow / Pop-Shrink | Shrink | 寿命に沿ったサイズカーブです。 |
| Size 0% / 33% / 66% / 100% | 0–5 | 1 / 1 / 0.65 / 0.35 | Custom時の基本サイズに対する倍率です。 |

Custom以外のカーブを選ぶと4点の値はプリセット値へ更新され、個別編集は無効になります。

## Rendering

| パラメータ | 範囲 / 選択肢 | 初期値 | 内容 |
|---|---|---:|---|
| Shape | Circle / Square / Triangle / Star / Line / Image | Circle | パーティクル形状です。 |
| Sprite Source | レイヤー | なし | ShapeがImageの場合の画像ソースです。 |
| Time Sampling | Current Time / Birth Time / Random - Still / Random - Play / Cycle | Current Time | 動画・連番ソースの時間参照方法です。 |
| Frame Count | 1–100 | 1 | スプライトとして扱うフレーム数です。 |
| Image Proxy | Full / /2 / /4 / /8 | /4 | 画像キャッシュの縮小率です。 |
| Edge Softness | 0–1 | 0 | 図形の輪郭を柔らかくします。 |
| Image Color | Tint / Source | Tint | Imageを粒子色で着色するか、元画像色を使うかを選びます。 |
| Image Fit | Contain / Stretch | Contain | スプライト枠内への画像の収め方です。 |
| Use Source Alpha | On / Off | On | ソース画像のアルファを使用します。 |
| Source Premultiplied | On / Off | On | ソースをプリマルチプライ済みとして扱います。 |
| Alpha Clip | 0–1 | 0.01 | この値以下の画像アルファを破棄します。 |
| Blend Mode | Normal / Add / Screen | Normal | パーティクル同士の描画合成です。 |
| Motion Blur | 0–1 | 0.2 | 速度方向のブラー量です。 |
| Depth of Field | On / Off | Off | Z距離によるボケを有効にします。 |
| DOF Focal Distance | 0–10000 | 0 | ピントが合うZ距離です。 |
| DOF Aperture | 0–100 | 5 | 焦点面から離れた粒子のボケ量です。 |
| Size Multiplier | 0.01–10 | 1.15 | 最終描画サイズの一括倍率です。 |
| Composite on Original | On / Off | On | 入力画像の上へ結果を合成します。 |
| Apply Mode | On Transparent / Normal / Add / Screen | Normal | 最終出力への適用方法です。 |

Image関連の項目は`Shape = Image`のときだけ有効です。`Source Premultiplied`と`Alpha Clip`は、さらに`Use Source Alpha`が有効な場合に使用します。

## Child Particles

| パラメータ | 範囲 | 初期値 | 内容 |
|---|---:|---:|---|
| Child Particles | On / Off | Off | 子パーティクルを有効にします。 |
| Child Count | 0–20 | 3 | 親1個あたりの子の数です。 |
| Child Inherit Velocity | 0–1 | 0.65 | 親の速度を引き継ぐ割合です。 |
| Child Lifespan | 0.01–10 | 0.5 | 子の寿命です。 |
| Child Speed | 0–10000 | 80 | 子に追加する初速です。 |
| Child Spread (deg) | 0–180 | 110 | 子の放出方向のばらつきです。 |
| Child Size Scale | 0.01–5 | 0.4 | 親の基本サイズに対する子の倍率です。 |

## System

| パラメータ | 範囲 | 初期値 | 内容 |
|---|---:|---:|---|
| Random Seed | 0–99999 | 12345 | 発生位置、ばらつき、回転などの疑似乱数系列を切り替えます。同じ設定とSeedでは再現可能です。 |

## Node Graph関連

Particle KitにはクラシックUIと並行して、開発中のNode Graph、Published Controls、Node UI Sidecarがあります。これらはプロジェクト状態の書き出し・読み込みや、一部の値をAE側へ公開するための開発機能です。通常のパーティクル制作ではクラシックUIだけで完結します。

内部構造とデータの流れは [ARCHITECTURE.md](./ARCHITECTURE.md)、今後の移行方針は [PARTICLELAB_EVOLUTION.md](./PARTICLELAB_EVOLUTION.md) を参照してください。
