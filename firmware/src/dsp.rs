//! 24 kcomplex-samples/s -> 12 kHz mono SSB, without allocation.
//! Blackman-windowed complex FIR selects +300..2700 Hz (USB) or its conjugate
//! (LSB), and supplies the anti-alias filter before decimation by two.
use crate::cat::{Mode, Tuning};
use crate::dsp_tables::{FIR_IM, FIR_RE, SINE};
const TAPS: usize = FIR_RE.len();

pub struct Demodulator {
    history: [(i16, i16); TAPS],
    head: usize,
    odd: bool,
    phase: u32,
    increment: u32,
    mode: Mode,
    pub clipped: u32,
}
impl Demodulator {
    pub fn new(tuning: Tuning) -> Self {
        // Translate the hardware's coarse carrier to the requested dial frequency.
        let offset = i64::from(tuning.frequency) - i64::from(tuning.coarse_frequency());
        Self {
            history: [(0, 0); TAPS],
            head: 0,
            odd: false,
            phase: 0,
            increment: ((-offset * (1_i64 << 32)) / 24_000) as u32,
            mode: tuning.mode,
            clipped: 0,
        }
    }

    pub fn process(&mut self, i: i16, q: i16) -> Option<i16> {
        let (i, q) = if self.increment == 0 {
            (i, q)
        } else {
            let idx = (self.phase >> 22) as usize;
            let sin = i64::from(SINE[idx]);
            let cos = i64::from(SINE[(idx + 256) & 1023]);
            self.phase = self.phase.wrapping_add(self.increment);
            // Scale both components by 1/2 before rotation to retain headroom
            // for arbitrary full-scale I/Q; compensate after filtering below.
            (
                ((i64::from(i) * cos - i64::from(q) * sin) >> 16) as i16,
                ((i64::from(i) * sin + i64::from(q) * cos) >> 16) as i16,
            )
        };
        self.history[self.head] = (i, q);
        let newest = self.head;
        self.head = (self.head + 1) % TAPS;
        self.odd = !self.odd;
        if self.odd {
            return None;
        }
        let mut sum = 0i64;
        let mut idx = newest;
        for tap in 0..TAPS {
            let (i, q) = self.history[idx];
            let quadrature = i64::from(FIR_IM[tap]) * i64::from(q);
            sum += i64::from(FIR_RE[tap]) * i64::from(i)
                + if self.mode == Mode::Usb {
                    -quadrature
                } else {
                    quadrature
                };
            idx = if idx == 0 { TAPS - 1 } else { idx - 1 };
        }
        let pcm = sum >> if self.increment == 0 { 15 } else { 14 };
        if pcm < i64::from(i16::MIN) || pcm > i64::from(i16::MAX) {
            self.clipped = self.clipped.saturating_add(1);
        }
        Some(pcm.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::f64::consts::TAU;
    fn amplitude(mode: Mode, dial: u32, frequency: f64) -> f64 {
        let mut dsp = Demodulator::new(Tuning {
            frequency: dial,
            mode,
        });
        let mut energy = 0.;
        let mut count = 0;
        for n in 0..12000 {
            let p = TAU * frequency * n as f64 / 24000.;
            if let Some(v) = dsp.process((12000. * p.cos()) as i16, (12000. * p.sin()) as i16)
                && n > 1000
            {
                energy += f64::from(v).powi(2);
                count += 1;
            }
        }
        (energy / count as f64).sqrt()
    }
    #[test]
    fn selects_both_sidebands_and_rejects_aliases() {
        for (mode, sign) in [(Mode::Usb, 1.), (Mode::Lsb, -1.)] {
            for hz in [450., 1000., 1500., 2500.] {
                let wanted = amplitude(mode, 14_200_000, sign * hz);
                assert!((7500. ..9000.).contains(&wanted), "{mode:?} {hz}: {wanted}");
                let image = amplitude(mode, 14_200_000, -sign * hz);
                assert!(image < wanted / 1000., "image {hz}: {image}");
            }
            for hz in [0., 4000., 7000., 10500.] {
                assert!(amplitude(mode, 14_200_000, sign * hz) < 12.);
            }
        }
    }
    #[test]
    fn fine_tuning_and_decimation() {
        // A 49 Hz digital shift places this tone exactly at 1500 Hz.
        let mut dsp = Demodulator::new(Tuning {
            frequency: 14_200_049,
            mode: Mode::Usb,
        });
        let mut re = 0.;
        let mut im = 0.;
        let mut count = 0;
        for n in 0..24000 {
            let p = TAU * 1549. * n as f64 / 24000.;
            if let Some(v) = dsp.process((12000. * p.cos()) as i16, (12000. * p.sin()) as i16) {
                if n > 1000 {
                    let angle = TAU * 1500. * count as f64 / 12000.;
                    re += f64::from(v) * angle.cos();
                    im += f64::from(v) * angle.sin();
                }
                count += 1;
            }
        }
        assert_eq!(count, 12000);
        assert!((re * re + im * im).sqrt() / 11500. > 5900.);
        assert!(amplitude(Mode::Lsb, 14_200_049, -1451.) > 8000.);
    }
    #[test]
    fn reset_discards_history_and_extremes_do_not_wrap() {
        let tuning = Tuning::default();
        let mut dsp = Demodulator::new(tuning);
        for _ in 0..1000 {
            dsp.process(i16::MIN, i16::MAX);
        }
        let mut dsp = Demodulator::new(tuning);
        for _ in 0..1000 {
            assert!(dsp.process(0, 0).is_none_or(|x| x == 0));
        }
        for frequency in [14_200_000, 14_200_049, 14_200_051] {
            let mut dsp = Demodulator::new(Tuning {
                frequency,
                ..tuning
            });
            for n in 0..4000 {
                let p = TAU * 1500. * n as f64 / 24000.;
                dsp.process(
                    if p.cos() > 0. { i16::MAX } else { i16::MIN },
                    if p.sin() > 0. { i16::MAX } else { i16::MIN },
                );
            }
            assert!(dsp.clipped > 0);
        }
    }
}
