# 外側が太く、先端が丸いランダム集中線

1920×1080基準。FocusLines v0.1.2の既存パラメータで再現。GPUコアで768×432、24fps、3秒のGIFを生成。中心のサイズは固定。3フレームごと（毎秒8回）に線の配置・幅・長さを更新。

| Parameter | Value |
|---|---|
| Focus Point | 960, 540 |
| Center Size | 105% |
| Center Aspect | 1.22 |
| Line Count | 120 |
| Line Width | 60 px |
| Width Random | 75% |
| Tip Width | 35% |
| Tip Cap | Round |
| Taper Length | 50% |
| Taper Shape | 1.0 |
| Length Random | 50% |
| Seed | 12 |
| Angle Jitter | 8° |
| Clustering | 0 |
| Density Variation | 0 |
| Evolution | Hold Frames |
| Hold Frames | 3 |
| Change Amount | 100% |
| Stagger | 0% |
| Color | #ECA1B6 |
| Opacity | 90% |
| Intensity | 100% |
| Composite | Lines Only |
| Backend | GPU |
| Antialiasing | 16 |
| 背景レイヤー | #FFF9F5 |

その他は既定値。エクスプレッション不要。線の外側は太く、中心側は35%の幅まで細くなり、その先端に丸みを付ける。

Short Lines: Minimum Length 30px / Maximum Length 120px / Mix 40%。
