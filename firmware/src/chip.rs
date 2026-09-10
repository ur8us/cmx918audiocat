//! CMX918 I2C control. Sources: D/918/2.0 pp.20–31,56–60; UM918/2.0
//! registers 03,08–0B,28–2A,5A–5C,B7–BA,D0–D1,D7–D9.
//! Uses documented defaults rather than replaying captured readbacks/poll counts.

use crate::{Config, Error};
use embedded_hal_async::{delay::DelayNs, i2c::I2c};

pub const ADDRESS: u8 = 0x55;
// UM918/2.0 p.65, $A2: RF_LNA_CTL_1. Bit 1 enables HF_IN; bit 0 enables
// VHF_IN. Below 2 MHz, Fc must also select the HF/VHF mixer/LO route.
const FORCED_HF_INPUT: u8 = 0x02;
// DS pp.10–12: LF/MF and HF/VHF have separate mixers, selected by band.
const HF_ROUTING_HZ: u32 = 2_000_000;
// D/918/2.0 Table 5: first 16 FIR1 and first 40 FIR2 coefficients.
const FIR: [i16; 56] = [
    -13, -137, -56, 573, -60, -1824, 1284, 8378, -61, -173, 249, 562, -1113, -1190, 4998, 9744, 28,
    44, 54, 38, -11, -82, -146, -163, -107, 16, 157, 244, 211, 43, -201, -402, -435, -236, 142,
    531, 716, 545, 28, -629, -1092, -1057, -424, 602, 1551, 1876, 1229, -310, -2162, -3410, -3145,
    -870, 3214, 8185, 12674, 15332,
];

pub struct Chip<I, D> {
    bus: I,
    delay: D,
}

impl<I: I2c, D: DelayNs> Chip<I, D> {
    pub fn new(bus: I, delay: D) -> Self {
        Self { bus, delay }
    }

    pub async fn read(&mut self, register: u8) -> Result<u8, Error> {
        let mut value = [0];
        self.bus
            .write_read(ADDRESS, &[register], &mut value)
            .await
            .map_err(|_error| {
                #[cfg(target_arch = "arm")]
                defmt::warn!(
                    "I2C read failed reg={=u8:#x}: {}",
                    register,
                    defmt::Debug2Format(&_error)
                );
                Error::I2c
            })?;
        #[cfg(target_arch = "arm")]
        defmt::trace!("I2C read reg={=u8:#x} value={=u8:#x}", register, value[0]);
        Ok(value[0])
    }

    async fn write(&mut self, register: u8, value: u8) -> Result<(), Error> {
        #[cfg(target_arch = "arm")]
        defmt::trace!("I2C write reg={=u8:#x} value={=u8:#x}", register, value);
        self.bus
            .write(ADDRESS, &[register, value])
            .await
            .map_err(|_error| {
                #[cfg(target_arch = "arm")]
                defmt::warn!(
                    "I2C write failed reg={=u8:#x}: {}",
                    register,
                    defmt::Debug2Format(&_error)
                );
                Error::I2c
            })
    }

    async fn update(&mut self, register: u8, mask: u8, value: u8) -> Result<(), Error> {
        let previous = self.read(register).await?;
        self.write(register, (previous & !mask) | (value & mask))
            .await
    }

    pub async fn mute(&mut self, mute: bool) -> Result<(), Error> {
        self.update(0x28, 8, if mute { 8 } else { 0 }).await?;
        for _ in 0..20 {
            if self.read(0x60).await? & 1 == u8::from(mute) {
                return Ok(());
            }
            self.delay.delay_ms(1).await;
        }
        Err(Error::Timeout)
    }

    pub async fn configure(&mut self, config: Config) -> Result<(), Error> {
        let config = config.validate()?;
        // Stop capture before entering this routine. Standby quiesces processing
        // even if a pre-existing stream's frame-synchronous mute cannot finish.
        self.write(0x03, 1).await?;
        self.write(0x97, config.xtal_control()).await?;
        self.delay.delay_ms(10).await;
        self.write(0x28, config.output_control(true)).await?;
        self.write(0x2a, 3).await?; // internal PLL, automatic divider, fractional DSM
        self.write(0x30, 0).await?; // reference divider /1; known baseline
        // Reset manual output-clock overrides; choose one frame per sample.
        self.write(0x5a, 0).await?; // documented 20 clocks/channel, 40/frame
        self.write(0x5b, 0x19).await?; // single frame, manual ratio x1
        self.write(0x5c, 0).await?; // no manual master-clock division
        self.update(0xb7, 0x80, 0).await?; // direct SCLK, sample on rising edge
        self.write(0xb8, 0x64).await?; // documented running I/Q compensation
        self.write(0xba, 0x0a).await?; // demodulation/DC correction, manual digital gain
        // Digital AGC is bypassed above; restore documented manual defaults.
        for (r, v) in [
            (0xbb, 0x3e),
            (0xbc, 0),
            (0xbd, 0x80),
            (0xbe, 0),
            (0xbf, 0x80),
            (0x0c, 8),
        ] {
            self.write(r, v).await?;
        }
        let fc = config.carrier();
        for (i, v) in fc.iter().enumerate() {
            self.write(0x08 + i as u8, *v).await?;
        }
        self.write(0x0b, config.bandwidth_code()).await?;
        for (index, coefficient) in FIR.iter().enumerate() {
            let [msb, lsb] = coefficient.to_be_bytes();
            self.write(0xd7, index as u8).await?;
            self.write(0xd8, msb).await?;
            self.write(0xd9, lsb).await?; // LSB commits coefficient, no auto-increment
        }
        self.write(0x03, 3).await?;
        self.write(0x29, 5).await?; // IF filter + automatic PLL and VCO calibration
        for attempt in 0..200 {
            if self.read(0x29).await? & 7 == 0 {
                break;
            }
            if attempt == 199 {
                return Err(Error::Timeout);
            }
            self.delay.delay_ms(5).await;
        }
        // Check the physical lock indicator separately from calibration completion.
        let mut locked = false;
        for _ in 0..100 {
            if self.read(0x06).await? & 8 != 0 {
                locked = true;
                break;
            }
            self.delay.delay_ms(5).await;
        }
        if !locked {
            return Err(Error::Timeout);
        }
        // DS p.28: writing Fc does not execute PLL changes. Below 2 MHz,
        // retain the actual-frequency calibration above, then select the HF
        // mixer/LO route with an HF-band Fc and the original IF polarity.
        // Do not run PLL_CALC again with this routing value. CAT and DSP keep
        // the actual dial/coarse frequency. See docs/LF-HF-INPUT-CHECK-2026-09-10.md.
        let mut rf_fc = fc;
        if config.frequency < HF_ROUTING_HZ {
            // Guard CTRL, N/F/R, programmed L and active L against any implicit
            // retune caused by changing Fc on this silicon revision.
            let mut pll = [0; 11];
            for (i, value) in pll.iter_mut().enumerate() {
                *value = self.read(0x2a + i as u8).await?;
            }
            rf_fc = Config {
                frequency: HF_ROUTING_HZ,
                ..config
            }
            .carrier();
            rf_fc[0] = (rf_fc[0] & 0x1f) | (fc[0] & 0xc0);
            for (i, v) in rf_fc.iter().enumerate() {
                self.write(0x08 + i as u8, *v).await?;
            }
            for (i, expected) in pll.iter().enumerate() {
                if self.read(0x2a + i as u8).await? != *expected {
                    return Err(Error::Readback);
                }
            }
        }
        // The normal-mode automatic policy selects LF/MF, HF, or VHF from Fc.
        // Override that policy after PLL/VCO calibration for the VCO experiment.
        self.write(0xa2, FORCED_HF_INPUT).await?;
        if self.read(0xa2).await? != FORCED_HF_INPUT || self.read(0x06).await? & 8 == 0 {
            return Err(Error::Readback);
        }
        for (r, v) in [
            (8, rf_fc[0]),
            (9, rf_fc[1]),
            (10, rf_fc[2]),
            (11, config.bandwidth_code()),
            (0x28, config.output_control(true)),
            (0x5a, 0),
            (0x5b, 0x19),
            (0x5c, 0),
            (0x97, config.xtal_control()),
            (0xa2, FORCED_HF_INPUT),
        ] {
            let actual = self.read(r).await?;
            if actual != v {
                #[cfg(target_arch = "arm")]
                defmt::warn!(
                    "readback reg={=u8:#x} expected={=u8:#x} actual={=u8:#x}",
                    r,
                    v,
                    actual
                );
                return Err(Error::Readback);
            }
        }
        #[cfg(target_arch = "arm")]
        {
            let status = self.read(0x06).await?;
            let l_hi = self.read(0x33).await?;
            let l_lo = self.read(0x34).await?;
            let input = self.read(0xa2).await?;
            defmt::info!(
                "RF status={=u8:#x} L={} A2={=u8:#x} Fc={=[u8]:#x}",
                status,
                (u16::from(l_hi & 7) << 8) | u16::from(l_lo),
                input,
                rf_fc
            );
        }
        Ok(())
    }

    pub async fn reset(&mut self) -> Result<(), Error> {
        // UM918/2.0 p.11, $04: software reset is host-cleared, retains I2C.
        self.write(0x04, 1).await?;
        self.write(0x04, 0).await?;
        self.delay.delay_ms(10).await;
        if self.read(0x04).await? != 0 {
            return Err(Error::Readback);
        }
        Ok(())
    }

    /// Returns raw STATUS and signed RSSI in 1/16 dB units. Absolute RSSI
    /// calibration is board-dependent and is not established by this firmware.
    pub async fn status(&mut self) -> Result<(u8, i16), Error> {
        let status = self.read(0x06).await?;
        let mut raw = [0; 2];
        self.bus
            .write_read(ADDRESS, &[0xd0], &mut raw)
            .await
            .map_err(|_| Error::I2c)?;
        let value = u16::from_be_bytes(raw) & 0xfff;
        Ok((status, ((value << 4) as i16) >> 4))
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use embedded_hal::i2c::{ErrorType, Operation};
    use std::vec::Vec;
    struct Bus {
        regs: [u8; 256],
        writes: Vec<(u8, u8)>,
        busy: bool,
        fail: bool,
        retune_on_route: bool,
    }
    impl Bus {
        fn new() -> Self {
            let mut b = Self {
                regs: [0; 256],
                writes: Vec::new(),
                busy: false,
                fail: false,
                retune_on_route: false,
            };
            b.regs[6] = 8;
            b
        }
    }
    impl ErrorType for Bus {
        type Error = embedded_hal::i2c::ErrorKind;
    }
    impl I2c for Bus {
        async fn transaction(
            &mut self,
            address: u8,
            operations: &mut [Operation<'_>],
        ) -> Result<(), Self::Error> {
            assert_eq!(address, 0x55);
            if self.fail {
                return Err(embedded_hal::i2c::ErrorKind::Bus);
            }
            let mut reg = 0;
            for op in operations {
                match op {
                    Operation::Write(bytes) => {
                        reg = bytes[0] as usize;
                        for &value in &bytes[1..] {
                            self.regs[reg] = value;
                            self.writes.push((reg as u8, value));
                            if self.retune_on_route
                                && reg == 0x0a
                                && self.regs[8..=10] == [0, 0x4e, 0x20]
                            {
                                self.regs[0x2c] ^= 1;
                            }
                            if reg == 0x29 && !self.busy {
                                self.regs[reg] = 0;
                            }
                            if reg == 0x28 {
                                self.regs[0x60] = u8::from(value & 8 != 0);
                            }
                            reg += 1;
                        }
                    }
                    Operation::Read(bytes) => {
                        for value in bytes.iter_mut() {
                            *value = self.regs[reg];
                            reg += 1;
                        }
                    }
                }
            }
            Ok(())
        }
    }
    struct Delay;
    impl DelayNs for Delay {
        async fn delay_ns(&mut self, _: u32) {}
    }
    fn config() -> Config {
        Config {
            frequency: 14_200_000,
            rate: 48_000,
            bandwidth: 10_000,
            flags: 2,
        }
    }
    #[test]
    fn initialization_orders_carrier_and_commits_every_coefficient() {
        futures::executor::block_on(async {
            let mut chip = Chip::new(Bus::new(), Delay);
            chip.configure(config()).await.unwrap();
            let writes = &chip.bus.writes;
            assert!(
                writes
                    .windows(3)
                    .any(|v| v == [(8, 0x82), (9, 0x2a), (10, 0xb0)])
            );
            assert_eq!(writes.iter().filter(|(r, _)| *r == 0xd9).count(), 56);
            assert_eq!(chip.bus.regs[0x28], 0x9b);
            assert_eq!(chip.bus.regs[0xa2], 0x02);
            chip.mute(false).await.unwrap();
            assert_eq!(chip.bus.regs[0x60], 0);
        });
    }
    #[test]
    fn lf_routing_preserves_calibrated_pll_and_actual_frequency_order() {
        futures::executor::block_on(async {
            let mut chip = Chip::new(Bus::new(), Delay);
            let config = Config {
                frequency: 474_200,
                ..config()
            };
            chip.configure(config).await.unwrap();
            let writes = &chip.bus.writes;
            let calibration = writes.iter().position(|v| *v == (0x29, 5)).unwrap();
            assert!(
                writes[..calibration]
                    .windows(3)
                    .any(|v| v == [(8, 0), (9, 0x12), (10, 0x86)])
            );
            assert!(
                writes[calibration + 1..]
                    .windows(3)
                    .any(|v| v == [(8, 0), (9, 0x4e), (10, 0x20)])
            );
            // Nothing after routing may recalculate the PLL or enter a new mode.
            assert!(
                !writes[calibration + 1..]
                    .iter()
                    .any(|(r, _)| *r == 0x29 || *r == 3)
            );
            assert_eq!(chip.bus.regs[0xa2], 2);
            let mut bus = Bus::new();
            bus.retune_on_route = true;
            assert_eq!(
                Chip::new(bus, Delay).configure(config).await,
                Err(Error::Readback)
            );
        });
    }
    #[test]
    fn calibration_and_bus_failures_propagate() {
        futures::executor::block_on(async {
            let mut bus = Bus::new();
            bus.busy = true;
            assert_eq!(
                Chip::new(bus, Delay).configure(config()).await,
                Err(Error::Timeout)
            );
            let mut bus = Bus::new();
            bus.fail = true;
            assert_eq!(
                Chip::new(bus, Delay).configure(config()).await,
                Err(Error::I2c)
            );
        });
    }
    #[test]
    fn invalid_configuration_never_touches_bus() {
        futures::executor::block_on(async {
            let mut chip = Chip::new(Bus::new(), Delay);
            assert!(
                chip.configure(Config {
                    rate: 960_000,
                    ..config()
                })
                .await
                .is_err()
            );
            assert!(chip.bus.writes.is_empty());
        });
    }

    #[test]
    fn crystal_trim_is_explicit_and_reset_is_released() {
        futures::executor::block_on(async {
            let mut chip = Chip::new(Bus::new(), Delay);
            for code in 0..16 {
                let config = Config {
                    flags: 2 | 8 | (code << 4),
                    ..config()
                };
                chip.configure(config).await.unwrap();
                assert_eq!(chip.bus.regs[0x97], ((code << 4) | 1) as u8);
            }
            chip.configure(config()).await.unwrap();
            assert_eq!(chip.bus.regs[0x97], 0x61);
            chip.reset().await.unwrap();
            assert!(chip.bus.writes.ends_with(&[(4, 1), (4, 0)]));
        });
    }

    #[test]
    fn status_probe_reads_signed_rssi_without_programming_registers() {
        futures::executor::block_on(async {
            let mut bus = Bus::new();
            bus.regs[0xd0] = 0x0f;
            bus.regs[0xd1] = 0xf0;
            let mut chip = Chip::new(bus, Delay);
            assert_eq!(chip.status().await, Ok((8, -16)));
            assert!(chip.bus.writes.is_empty());
        });
    }
}
