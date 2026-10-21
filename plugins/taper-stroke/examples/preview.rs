use std::io::Write;
use taper_stroke::core::{Outline, Point, Style};

fn main() -> std::io::Result<()> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "preview.ppm".into());
    let (w, h) = (800, 440);
    let mut image = vec![0u8; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            let c = if (x / 16 + y / 16) % 2 == 0 { 32 } else { 40 };
            image[(y * w + x) * 3..(y * w + x) * 3 + 3].fill(c);
        }
    }
    for index in 0..3 {
        // A cubic with unequal handles, sampled densely for this host-independent preview.
        let points: Vec<_> = (0..=1400)
            .map(|j| {
                let t = j as f64 / 1400.0;
                let u = 1.0 - t;
                let base = 70.0 + index as f64 * 140.0;
                Point::new(
                    u * u * u * 65.0
                        + 3.0 * u * u * t * 250.0
                        + 3.0 * u * t * t * 490.0
                        + t * t * t * 735.0,
                    u * u * u * base
                        + 3.0 * u * u * t * (base - 100.0)
                        + 3.0 * u * t * t * (base + 110.0)
                        + t * t * t * base,
                )
            })
            .collect();
        let s = Style {
            width: 48.0,
            start: 0.20,
            end: if index == 1 { 0.35 } else { 0.0 },
            ease: if index == 2 { 2.0 } else { 1.0 },
            round: index != 2,
        };
        let outline = Outline::new(&points, s, 1.0, 1.0);
        let mut row = vec![0.0; w];
        for y in 0..h {
            outline.row(y as f64, 0.0, &mut row);
            for (x, &coverage) in row.iter().enumerate() {
                for c in 0..3 {
                    let at = (y * w + x) * 3 + c;
                    let fg = [240.0, 185.0, 95.0][c];
                    image[at] = (image[at] as f32 * (1.0 - coverage) + fg * coverage).round() as u8;
                }
            }
        }
    }
    let mut file = std::fs::File::create(path)?;
    write!(file, "P6\n{w} {h}\n255\n")?;
    file.write_all(&image)
}
