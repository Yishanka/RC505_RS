//! RBJ cookbook biquads (W3C Audio EQ Cookbook), transposed direct form II.
use std::f32::consts::TAU;

#[derive(Clone, Copy, Debug)]
pub struct Coeff {
    b: [f32; 3],
    a: [f32; 2],
}
impl Default for Coeff {
    fn default() -> Self {
        Self {
            b: [1.0, 0.0, 0.0],
            a: [0.0; 2],
        }
    }
}
impl Coeff {
    pub fn response_db(self, sr: f32, hz: f32) -> f32 {
        let w = TAU * hz / sr;
        let numerator = (self.b[0] + self.b[1] * w.cos() + self.b[2] * (2.0 * w).cos()).powi(2)
            + (self.b[1] * w.sin() + self.b[2] * (2.0 * w).sin()).powi(2);
        let denominator = (1.0 + self.a[0] * w.cos() + self.a[1] * (2.0 * w).cos()).powi(2)
            + (self.a[0] * w.sin() + self.a[1] * (2.0 * w).sin()).powi(2);
        10.0 * (numerator / denominator.max(1e-20)).max(1e-20).log10()
    }
}
#[derive(Clone, Copy, Default)]
pub struct Biquad {
    z: [f32; 2],
}
impl Biquad {
    pub fn next(&mut self, x: f32, c: Coeff) -> f32 {
        let y = x * c.b[0] + self.z[0];
        self.z[0] = x * c.b[1] - c.a[0] * y + self.z[1];
        self.z[1] = x * c.b[2] - c.a[1] * y;
        if self.z[0].abs() < 1e-25 {
            self.z[0] = 0.0;
        }
        if self.z[1].abs() < 1e-25 {
            self.z[1] = 0.0;
        }
        y
    }
}
pub fn peak(sr: f32, hz: f32, q: f32, db: f32) -> Coeff {
    let a = 10.0_f32.powf(db / 40.0);
    let w = TAU * hz.clamp(1.0, sr * 0.45) / sr;
    let alpha = w.sin() / (2.0 * q.max(0.1));
    normalize(
        [1.0 + alpha * a, -2.0 * w.cos(), 1.0 - alpha * a],
        [1.0 + alpha / a, -2.0 * w.cos(), 1.0 - alpha / a],
    )
}
pub fn shelf(sr: f32, hz: f32, db: f32, high: bool) -> Coeff {
    let a = 10.0_f32.powf(db / 40.0);
    let w = TAU * hz.clamp(1.0, sr * 0.45) / sr;
    let c = w.cos();
    let d = w.sin() * std::f32::consts::SQRT_2 * a.sqrt();
    if high {
        normalize(
            [
                a * ((a + 1.0) + (a - 1.0) * c + d),
                -2.0 * a * ((a - 1.0) + (a + 1.0) * c),
                a * ((a + 1.0) + (a - 1.0) * c - d),
            ],
            [
                (a + 1.0) - (a - 1.0) * c + d,
                2.0 * ((a - 1.0) - (a + 1.0) * c),
                (a + 1.0) - (a - 1.0) * c - d,
            ],
        )
    } else {
        normalize(
            [
                a * ((a + 1.0) - (a - 1.0) * c + d),
                2.0 * a * ((a - 1.0) - (a + 1.0) * c),
                a * ((a + 1.0) - (a - 1.0) * c - d),
            ],
            [
                (a + 1.0) + (a - 1.0) * c + d,
                -2.0 * ((a - 1.0) + (a + 1.0) * c),
                (a + 1.0) + (a - 1.0) * c - d,
            ],
        )
    }
}
fn normalize(b: [f32; 3], a: [f32; 3]) -> Coeff {
    Coeff {
        b: b.map(|v| v / a[0]),
        a: [a[1] / a[0], a[2] / a[0]],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unity_filters_and_shelf_gains() {
        for c in [
            peak(48000.0, 1000.0, 0.707, 0.0),
            shelf(48000.0, 100.0, 0.0, false),
            shelf(48000.0, 6000.0, 0.0, true),
        ] {
            let mut b = Biquad::default();
            for n in 0..1000 {
                let x = (n as f32 * 0.1).sin();
                assert!((b.next(x, c) - x).abs() < 1e-5);
            }
        }
        let mut b = Biquad::default();
        let c = shelf(48000.0, 200.0, 6.0, false);
        let mut y = 0.0;
        for _ in 0..20000 {
            y = b.next(0.1, c);
        }
        assert!((y - 0.199526).abs() < 0.0001);
    }
}
