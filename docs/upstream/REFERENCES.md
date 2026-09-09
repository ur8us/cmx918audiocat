# Reference material and evidence

Reviewed for initial planning on 2026-09-08. Local paths are source locations on
the owner's machine, not required build dependencies. Vendor documents remain
under their own terms; this repository's MIT license does not relicense them.

## CMX918 manufacturer documents

Both files are in `~/WORK/drm1000-sniff/references/`:

| File | Revision | SHA-256 |
| --- | --- | --- |
| `CMX918_DS_V2.0.pdf` | D/918/2.0, January 2026 | `a7d74d0d25026df0d6f2e1c884b44bff15f02fb443d389db762e7d21133ace06` |
| `CMX918_UM_V2.0.pdf` | UM918/2.0, January 2026 | `4117467d95306d69302122f09e85fa23735f03dd3d6c86ae1fd38fcae92cf0ff` |

The investigation identifies the datasheet's publisher URL as
[CML CMX918 datasheet](https://cmlmicro.com/Content/Downloads/CMX918_DS_V2.0.pdf).
The local PDFs were used for this plan; a successful fresh download from that
URL was not verified. The user manual is the register-description second part.

Useful sections, using printed page numbers:

- Datasheet p. 1: general coverage 150 kHz–108 MHz; pin descriptions identify
  separate control and output data interfaces.
- Datasheet pp. 16–18: synthesizer, tuning, and the high-side LO requirement at
  the LF edge. The carrier field has 100 Hz units; field capacity is not an RF
  coverage guarantee.
- Datasheet pp. 20–24: digital processing, FIR loading, bandwidth, and Table 6
  sample/clock/frame-rate profiles. Table 6 p. 23 was also inspected visually.
- Datasheet pp. 25–26: output pins and SPI/I2S formats. Figure 13 was inspected
  visually: SPI I/Q data uses four bytes and a one-byte-time pause with CS
  marking active data. Two 16-bit components form one complex sample.
- Datasheet pp. 56–60: I2C selection, address `0x55`, 8-bit register addressing,
  read/write transactions, and 400 kHz timing limit.
- User manual p. 28, `0x28`: SPI selection, channel order, polarity, mute,
  oversampling, and clock ratio. Pages 49–51, `0x5A`–`0x5C`: framing and manual
  clock options, including repetition versus waiting at ratios above one.
- The user manual's opening note says pale-grey text describes future device
  features. Text extraction loses this distinction: inspect relevant rendered
  pages before claiming a feature or implementing its registers.

The user manual's generic 7-bit register/page description differs from the
datasheet's explicit I2C 8-bit register address. The local captures corroborate
direct I2C addresses above `0x7F`; document this interface-specific distinction
in the driver instead of mechanically applying paging to I2C.

## Local DRM1000 investigation

Start at `~/WORK/drm1000-sniff/README.md`, which links the newest corrections.
There was no Git repository available there at review time, so no source commit
could be recorded. The PDFs above are identified by hash; consult the source
project's `MANIFEST.json` and individual capture provenance for experiments.

| Relative source path | Use and limitation |
| --- | --- |
| `REPORT.md` | I2C transactions, frequency packing, FIR/gain profiles; its early PLL-only conclusions need later corrections |
| `registers.json` | Partial annotated map, not a complete replacement for the user manual |
| `analysis/reference-sequences.json` | Six observed programming sequences with trace offsets; not validated standalone initialization |
| `analysis/matrix/fir-tables.json` | Recovered FIR coefficients and source cases; reconfiguration history affects profiles |
| `PLL-LIMIT-REPORT.md` | Lock limits; lock is not proof of RF reception |
| `GENERATOR-RESPONSE-REPORT.md` | Carrier response at 128 MHz using L=4; relative RSSI, uncalibrated CMOS source |
| `L3-WIDE-SEARCH-REPORT.md` | L=3 behaves like effective L=7 in tested configurations; nominal 125/145 MHz recipes respond elsewhere |
| `L1-L2-EFFECTIVE-DIVIDER-REPORT.md` | Later L=1/L=2 findings; avoid assuming documented divider fields always match effective division |
| `L2-RECEPTION-REPORT.md` | Additional out-of-range responses with reversed IF selection; does not establish continuous calibrated coverage |

These are observations on the investigated DRM1000 hardware. Record silicon
revision and board differences before generalizing them to a new receiver.
Capture scripts may actively operate hardware; reading their evidence does not
require executing them. Do not import them wholesale or alter the source bundle.

## RP2350 and Embassy primary sources

- [RP2350 datasheet](https://datasheets.raspberrypi.com/rp2350/rp2350-datasheet.pdf):
  peripheral limits, DMA/PIO, clocking, and native USB full-speed operation at
  12 Mbit/s. This bounds transport feasibility independently of firmware language.
- [Embassy RP2350 HAL](https://docs.embassy.dev/embassy-rp/git/rp235xa/index.html):
  target support and available peripherals.
- [Embassy I2C](https://docs.embassy.dev/embassy-rp/git/rp235xa/i2c/index.html)
  and [USB](https://docs.embassy.dev/embassy-rp/git/rp235xa/usb/index.html):
  control and initial host interface support.
- [PIO receive API](https://docs.embassy.dev/embassy-rp/git/rp235xa/pio/struct.StateMachineRx.html):
  `dma_pull` and FIFO status support; not proof of a working high-rate SPI slave.
- [Embassy SPI source](https://github.com/embassy-rs/embassy/blob/main/embassy-rp/src/spi.rs):
  inspected during planning; no public slave constructor found. A receive-only
  constructor must not be assumed to accept an external master clock.

These online Embassy references are moving targets. Select and pin a tested
version/revision with the first firmware scaffold and update feasibility findings
against that revision.

## Owner's SPI receiver and oscillator test setup

- `~/WORK/RP2350/cmx918_ssb/cmx918_ssb.ino`, commit
  `1250cc4e2eca01383260b1c882a41b3f95a28442`: SPI1 RX/CS/SCLK GPIO8/9/10;
  separate I2S audio output on GPIO20/21/22. The owner identifies it as a working
  SPI slave example. Preserve the pins while implementing Rust + Embassy.
  See [HARDWARE.md](HARDWARE.md) for mode/timing and callback limitations.
- [Pico 2 header pinout](https://datasheets.raspberrypi.com/pico/Pico-2-Pinout.pdf):
  GPIO-to-physical-header mapping used in the wiring document.
- `~/WORK/drm1000-sniff/captures/generator-l2-matrix/events.jsonl`,
  `generator_rx`: captured OCXO HW 1.3 / SW Jul 10 2025 command help and status.
  `generator_response_probe.py` supplies baud rate, apply/settle/readback order,
  and the conservative 330 kHz–330 MHz frequency guard.
- [NumPy FFT reference](https://numpy.org/doc/stable/reference/routines.fft.html):
  signed frequency convention for the offline complex-I/Q checker.

The owner's updated scope limits testing/streaming to native USB rates. The
generator channel-1 VHF and channel-2 HF connections each have a 40 dB attenuator.
This supersedes the source investigation's channel-1-only test setup; preserve
channel 3. Initial live MCU/I2C checks are recorded in
[BRINGUP-2026-09-09.md](BRINGUP-2026-09-09.md); RF testing is still pending.
