//! CPU/WGSL で共有する予定の決定論ノイズ参照実装。

pub fn hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^ (x >> 16)
}

pub fn u01(k: u32, seed: u32, salt: u32) -> f32 {
    hash(k ^ hash(seed ^ salt.wrapping_mul(0x9e37_79b9))) as f32 / 4_294_967_296.0
}

fn smooth5(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn lattice(x: i32, y: i32, z: i32, seed: u32) -> f32 {
    let h = hash((x as u32) ^ hash((y as u32) ^ hash((z as u32) ^ seed)));
    h as f32 / 4_294_967_296.0
}

/// 3D value noise in the inclusive practical range 0..1.
pub fn value_noise(p: [f32; 3], seed: u32) -> f32 {
    let base = [
        p[0].floor() as i32,
        p[1].floor() as i32,
        p[2].floor() as i32,
    ];
    let t = [
        smooth5(p[0] - base[0] as f32),
        smooth5(p[1] - base[1] as f32),
        smooth5(p[2] - base[2] as f32),
    ];
    let mut corners = [[[0.0_f32; 2]; 2]; 2];
    for dz in 0..2 {
        for dy in 0..2 {
            for dx in 0..2 {
                corners[dz][dy][dx] = lattice(
                    base[0] + dx as i32,
                    base[1] + dy as i32,
                    base[2] + dz as i32,
                    seed,
                );
            }
        }
    }
    let lerp = |a: f32, b: f32, x: f32| a + (b - a) * x;
    let x00 = lerp(corners[0][0][0], corners[0][0][1], t[0]);
    let x10 = lerp(corners[0][1][0], corners[0][1][1], t[0]);
    let x01 = lerp(corners[1][0][0], corners[1][0][1], t[0]);
    let x11 = lerp(corners[1][1][0], corners[1][1][1], t[0]);
    lerp(lerp(x00, x10, t[1]), lerp(x01, x11, t[1]), t[2])
}

pub fn fbm(p: [f32; 3], octaves: u8, seed: u32) -> f32 {
    let octaves = octaves.clamp(1, 4);
    let mut sum = 0.0;
    let mut weight = 1.0;
    let mut norm = 0.0;
    for octave in 0..octaves {
        let scale = 2_f32.powi(octave as i32);
        sum += weight * (value_noise([p[0] * scale, p[1] * scale, p[2] * scale], seed) * 2.0 - 1.0);
        norm += weight;
        weight *= 0.5;
    }
    sum / norm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_known_values() {
        assert_eq!(hash(0), 0);
        assert_eq!(hash(1), 1_753_845_952);
        assert_eq!(hash(0xdead_beef), 3_861_431_939);
    }

    #[test]
    fn fbm_is_bounded() {
        for p in [[0.0, 0.0, 0.0], [1.2, -3.4, 5.6], [-10.5, 99.3, 0.1]] {
            assert!((-1.0..=1.0).contains(&fbm(p, 4, 1)));
        }
    }
}
