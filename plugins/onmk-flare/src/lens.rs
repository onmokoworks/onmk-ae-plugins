use std::path::Path;

#[derive(Clone, Debug)]
pub struct Surface {
    pub radius: f64,
    pub thickness: f64,
    pub ior: f64,
    pub abbe: f64,
    pub semi_aperture: f64,
    pub coating: i32,
    pub is_stop: bool,
    pub z: f64,
}

#[derive(Clone, Debug)]
pub struct LensSystem {
    pub name: String,
    pub focal_length: f64,
    pub surfaces: Vec<Surface>,
    pub sensor_z: f64,
}

impl LensSystem {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let text = std::fs::read_to_string(path.as_ref())
            .map_err(|e| format!("failed to read {}: {e}", path.as_ref().display()))?;
        Self::parse(&text)
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let mut name = String::new();
        let mut focal_length = 0.0;
        let mut surfaces = Vec::new();
        let mut in_surfaces = false;
        for (line_no, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            if let Some(value) = line.strip_prefix("name:") {
                name = value.trim().to_owned();
                continue;
            }
            if let Some(value) = line.strip_prefix("focal_length:") {
                focal_length = value
                    .trim()
                    .parse()
                    .map_err(|_| format!("line {}: invalid focal length", line_no + 1))?;
                continue;
            }
            if line == "surfaces:" {
                in_surfaces = true;
                continue;
            }
            if !in_surfaces {
                continue;
            }
            let cols: Vec<_> = line.split_whitespace().collect();
            if cols.len() < 6 {
                return Err(format!(
                    "line {}: expected six surface columns",
                    line_no + 1
                ));
            }
            let is_stop = cols[0].eq_ignore_ascii_case("stop");
            let radius = if is_stop {
                0.0
            } else {
                cols[0]
                    .parse()
                    .map_err(|_| format!("line {}: invalid radius", line_no + 1))?
            };
            let number = |i: usize, label: &str| {
                cols[i]
                    .parse::<f64>()
                    .map_err(|_| format!("line {}: invalid {label}", line_no + 1))
            };
            surfaces.push(Surface {
                radius,
                thickness: number(1, "thickness")?,
                ior: number(2, "ior")?,
                abbe: number(3, "abbe")?,
                semi_aperture: number(4, "semi aperture")?,
                coating: cols[5]
                    .parse()
                    .map_err(|_| format!("line {}: invalid coating", line_no + 1))?,
                is_stop,
                z: 0.0,
            });
        }
        if focal_length <= 0.0 || surfaces.is_empty() {
            return Err("lens requires focal_length and surfaces".into());
        }
        let mut z = 0.0;
        for surface in &mut surfaces {
            surface.z = z;
            z += surface.thickness;
        }
        Ok(Self {
            name,
            focal_length,
            surfaces,
            sensor_z: z,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_bundled_lenses() {
        let cases = [
            (include_str!("../lenses/cooketriplet.lens"), 7usize),
            (include_str!("../lenses/doublegauss.lens"), 11),
            (
                include_str!("../lenses/arri-zeiss-master-prime-t1.3-50mm.lens"),
                24,
            ),
            (include_str!("../lenses/canon-ef-200-400-f4.lens"), 44),
        ];
        for (text, expected) in cases {
            let lens = LensSystem::parse(text).unwrap();
            assert_eq!(lens.surfaces.len(), expected, "{}", lens.name);
            assert!(lens.sensor_z > 0.0);
        }
    }
}
