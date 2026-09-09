# RP2350 laboratory firmware

Rust 1.90.0 + Embassy, Cortex-M33, with CMX918 I2C configuration, PIO SPI slave
capture, continuous DMA, native USB bulk I/Q, and `defmt` RTT diagnostics.
This is **initial bring-up firmware, not a hardware-qualified receiver**.
The [September 9 hardware session](../docs/BRINGUP-2026-09-09.md) confirms
RP2350A boot/USB/RTT and records both the initial I2C NACK on GP0/GP1 and
20 successful read-only probes after host USB access became available. A later
DMA count fix enables short non-synthetic SPI-to-USB captures at 100 MHz,
10 kHz bandwidth and 24 ksample/s, including a restart. Electrical timing and
sustained streaming remain unqualified.

## Board assumptions and build

Development layout: 4 MiB external flash, 512 KiB main RAM, 12 MHz MCU crystal,
150 MHz system clock. Select `rp235xa` for RP2350A/Pico 2 or `rp235xb` for
RP2350B. The supplied PlatformIO configuration defines `PICO_RP2350B`, so do not
assume that the physical target is a Pico 2. The board/flash and control wiring
still need confirmation before programming. Neither image changes OTP/security
configuration; the ROM image definition selects ordinary ARM secure execution.

I2C0 uses **GP0 SDA, GP1 SCL**, 400 kHz, address `0x55`. These control pins are a
provisional development choice. SPI input preserves **GP8 data, GP9 CS, GP10
clock**, using PIO0/SM0 and DMA0. GP11 is unused. No CMX918 reset GPIO is assumed;
power/reset and bus isolation must be provided externally. The CMX918 reference
is the source project's 38.4 MHz baseline, separate from the MCU crystal.
See [wiring](../docs/HARDWARE.md).

From the repository root, with Rust/rustup and `picotool` on PATH:

```sh
cargo test --locked -p cmx918-firmware
python3 scripts/build_firmware.py --variant rp235xa
python3 scripts/build_firmware.py --variant rp235xb
```

Artifacts are `artifacts/<variant>/cmx918-sdr.elf`, `.uf2`, and `build.json`
containing hashes and source revision. The helper builds and inspects files;
it does not flash. Always retain the matching ELF for `defmt` decoding.
The linker places Embassy's RP2350 image definition immediately after the
vector table. This follows the [Embassy RP235x linker layout](https://github.com/embassy-rs/embassy/blob/main/examples/rp235x/memory.x).

## Traces through picoprobe/debugprobe

The probe transports SWD memory reads; firmware writes encoded `defmt` records
to an RTT buffer, and `probe-rs` decodes them using the ELF. Raspberry Pi's
[Debug Probe firmware](https://github.com/raspberrypi/debugprobe) runs on Pico/Pico 2
and supplies CMSIS-DAP. An older proprietary picoprobe firmware may need updating
to the CMSIS-DAP debugprobe build for use with `probe-rs`. No probe update has
been performed here. Connect target SWDIO, SWCLK and GND; RTT needs no UART pins.

Local `probe-rs` 0.28.0 includes target **`RP235x`**. Once the board is identified
and connected, list probes, then select its VID:PID:serial explicitly:

```sh
probe-rs list
# Replace PROBE_ID and choose the correct variant's image.
probe-rs run --chip RP235x --probe PROBE_ID artifacts/rp235xa/cmx918-sdr.elf
# Attach to the already-programmed matching firmware to collect RTT:
probe-rs attach --chip RP235x --probe PROBE_ID artifacts/rp235xa/cmx918-sdr.elf
```

`run` programs and starts the target; `attach` reads the running target's RTT.
These hardware commands were exercised on September 9 with the identified probe.
`picotool` remains an alternative
for programming over USB BOOTSEL; it does not decode RTT. See the
[probe-rs documentation](https://probe.rs/docs/) and [defmt setup](https://defmt.ferrous-systems.com/setup).

Default `DEFMT_LOG=info` logs boot, configuration, stream lifecycle and faults.
`debug` adds USB lifecycle and periodic status/RSSI; `trace` adds individual
I2C transactions. Filters are chosen **at compile time**. For example:

```sh
DEFMT_LOG=cmx918_sdr=debug,cmx918_firmware=trace python3 scripts/build_firmware.py --variant rp235xa
```

RTT has a 4096-byte buffer and `defmt-rtt/disable-blocking-mode` enabled.
It may discard diagnostics when full; it cannot wait indefinitely for a reader,
even if the host requests blocking RTT. No per-sample or data-packet logging is
present. Panic reports use the same nonblocking path and halt; host capture then
times out. No blocking logger flush or semihosting is used. This follows the
[defmt-rtt blocking-mode guidance](https://docs.rs/defmt-rtt/1.3.0/defmt_rtt/).
Debug logs are supplementary: the USB status and sample sequence checks carry
stream validity. During throughput tests, do not halt the core or set breakpoints.

## Implemented behavior and limits

- Configuration selects frequency, sample rate and chip bandwidth, with exact
  100 Hz frequency granularity. Seven bandwidth/rate profiles cover 12/24/48/96
  and provisionally 240 ksample/s. 480/960 ksample/s are rejected.
- Every profile requires explicit `--allow-unqualified`: the qualified-rate
  bitmap is zero. Frequencies outside 150 kHz–108 MHz additionally require
  `--experimental-rf`, bounded to 100 kHz–130 MHz. This opts into investigation;
  it does not implement the source project's special measured L=4 extension recipe.
- Optional `--xtal-caps 0..15` selects CMX918 crystal load capacitance (UM918/2.0
  p.61, $97), with readback and explicit applied flags; the default is code 6.
  `cmx918-device reset` issues the documented $04 software reset under a bounded
  timeout and clears configuration, permitting repeat-after-reset measurements.
- A read-only I2C status/RSSI probe runs at startup and is available as
  `cmx918-device probe` while stopped. RTT reports the result even before USB
  permissions are configured. It does not initialize the chip or start SPI.
- CMX918 initialization uses documented FIR coefficients, automatic PLL/VCO and
  IF calibration, 96 kHz IF, high-side LO below 2 MHz and low-side above, fixed
  RF/digital gain defaults with digital AGC bypassed, active-low CS, direct clock,
  and one output frame per sample. Calibration is bounded and configuration
  readback is checked. Manual input/divider/gain controls are not exposed yet.
- PIO samples rising edges and captures I then Q, 16 bits each, MSB first.
  DMA runs continuously into a 32 KiB aligned circular buffer. Software converts
  byte order without changing sample values, then queues up to eight USB blocks.
  A block is 512 complex samples with a 32-byte header.
- FIFO stall, observed early CS release, DMA errors, possible ring overrun or
  an ambiguous cursor interval stop capture with unknown loss reported. Polls
  are bounded to less than 8.192 ms using a conservative 1 Mframe/s input bound.
  No-progress timeout is 500 ms; USB block transfer timeout is 20 ms.
  Extra/missing electrical clock edges cannot all be diagnosed in software;
  known-pattern SPI and scope checks are mandatory before qualification.
- STOP/reconfigure discard queued samples and partial acquisition. START creates
  a new generation and sample timeline. An in-flight USB block may finish after
  STOP; the host drains it before START. Disconnect/suspend or host backpressure
  stops the stream; recovery requires an explicit new START/configuration.
- Synthetic mode emits deterministic I=index modulo 65536, Q=bitwise complement
  at the requested rate. It tests the USB path, not SPI timing or RF reception.
  Periodic chip status is cached; RSSI is signed raw 1/16 dB, not calibrated dBm.

The selected maximum SCLK is nominally 9.6 MHz (40 clocks × 240 ksample/s).
PIO runs at 150 MHz with input synchronizers enabled. Compilation and instruction
counts do not establish electrical sampling margin, actual sample rate, passband,
I/Q orientation or continuous USB throughput. Those checks require hardware.

See [protocol and PC commands](../docs/PROTOCOL.md) and the
[frequency/FFT qualification procedure](../docs/TESTING.md).
