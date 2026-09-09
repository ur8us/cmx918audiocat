# Prototype wiring and programming

The RP2350A receiver is connected and boots the SDR firmware as of September 9.
See the [bring-up record](BRINGUP-2026-09-09.md) for device identities and the
initial I2C NACK and subsequent successful read-only probes. Preserve the SPI
wiring in
`~/WORK/RP2350/cmx918_ssb/cmx918_ssb.ino`, Git revision
`1250cc4e2eca01383260b1c882a41b3f95a28442`. Its `setup()` configures `SPISlave1`:

| Signal | RP2350 GPIO | Pico 2 physical header pin | CMX918 package pin |
| --- | --- | --- | --- |
| SPI1 RX, I/Q data input | GP8 | 11 | 23, data output |
| SPI1 CS input | GP9 | 12 | 22, CS output |
| SPI1 SCLK input | GP10 | 14 | 24, clock output |
| SPI1 TX, configured in reference | GP11 | 15 | No connection needed for receive-only data |
| Ground | GND | 13 is adjacent | Common board ground |

Header numbers apply to the Pico 2 form factor, not bare RP2350 package pins.
See the [official Pico 2 pinout](https://datasheets.raspberrypi.com/pico/Pico-2-Pinout.pdf).
The actual board and I2C GPIO assignment still need confirmation. The development
firmware selects GP0 SDA and GP1 SCL (Pico 2 header pins 1 and 2); change these
if the final board differs. It uses PIO0 on the fixed SPI pins above.
CMX918 control
SDA/SCL are package pins 17/18; do not confuse control SDA with output data SDA.
Keep GPIO11 unconnected/high impedance unless the board requires it; never wire
an MCU output to the CMX918's data output.

The reference README identifies DRM1000 test points 9/10/11 collectively for
SPI, but does not map individual test points to signals. Confirm that mapping
from the board or scope instead of inferring it from numbering.

The Arduino sketch receives four-byte, high-byte-first I/Q words over SPI and
outputs demodulated audio using I2S: GP20 BCLK (header 26), GP21 WS (header 27),
GP22 data (header 29), at a configured 24 kHz audio rate. This is **I2S output to
MAX98357A**, not I2C control or I2S input from CMX918. The USB SDR prototype
retains raw I/Q and uses I2C control plus SPI capture; I2S audio output is optional.

Use the sketch as wiring and principle-of-operation evidence. It configures
`SPI_MODE1`, whereas the CMX918 datasheet describes rising-edge input sampling;
verify the chip's clock inversion, CS polarity, and actual electrical timing
before selecting the Rust driver's mode. Its audio output rate and requested
`SPISettings` clock do not prove the external SPI sample/clock rate. Do not port
callback code wholesale: its transmit callback formats a message into a four-byte
buffer, and its buffering has no trustworthy loss accounting.

## RF test connections

| Generator output | Connection | Attenuation |
| --- | --- | --- |
| Channel 1 | CMX918 VHF input | 40 dB |
| Channel 2 | CMX918 HF input | Separate 40 dB |
| Channel 3 | Outside test control | Preserve unchanged |

The present wiring does not connect the separate LF/MF input. Tests below 2 MHz
must remain blocked pending a suitable connection; HF-input leakage is not a
validated LF/MF reception measurement. The existing oscillator runner's range is
330 kHz–330 MHz, so it also does not establish a source for the 100–329 kHz tests.

The source is a programmable oscillator, not a calibrated sinusoidal RF source.
Log the two outputs because both are connected; an inactive channel or its
harmonics can contaminate a measurement. Do not assume an output-disable command
exists. If isolation is needed, record a controlled disconnection or verified
parking frequency. Source-away/back tests are required but do not by themselves
exclude harmonic conversion or establish absolute sensitivity.

## Programming and first connection

`picotool` v2.1.0 is installed on this Linux host. It is a PC programming utility,
not the receiver's application USB interface. ELF/UF2 images can now be built for
RP2350A and RP2350B with a provisional 4 MiB flash / 12 MHz MCU crystal layout;
see [firmware instructions](../firmware/README.md). RP2350A upload and readback
verification succeeded over SWD on September 9. Before flashing, identify the exact RP2350
board, flash size, USB boot device, and intended image. Record firmware hash and
board identity with each hardware test session. If an SWD probe is also present,
identify it separately from the receiver USB and CH552 serial device.

When hardware arrives: verify power/logic levels and bus ownership, program the
Rust + Embassy test firmware, verify I2C readback and SPI timing on the fixed pins,
then qualify USB streaming before starting the RF matrix. Firmware and the USB
receiver adapter are implemented for this bring-up; electrical and sustained
throughput validation remain outstanding. `defmt` RTT uses a separate SWD probe
(picoprobe with compatible CMSIS-DAP debugprobe firmware) and the matching ELF.
The probe connection must not be confused with the receiver's streaming USB.
