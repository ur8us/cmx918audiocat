# RP2350 firmware

Rust 1.90.0 / Embassy, Cortex-M33. This fork replaces upstream vendor bulk
I/Q USB with a UAC1 recording interface and CDC ACM CAT port. No external
USB bridge or PC demodulator is required. Hardware operation is unverified.

## Build

From the repository root, with Rust/rustup and `picotool` on PATH:

```sh
cargo test --locked -p cmx918-firmware
python3 scripts/build_firmware.py --variant rp235xa
python3 scripts/build_firmware.py --variant rp235xb
```

Each helper invocation produces `artifacts/<variant>/cmx918-audiocat.elf`,
`cmx918-audiocat.uf2`, and `build.json` with hashes, source revision, dirty
state and `hardware_tested: false`. It inspects the image without flashing.
Use `rp235xa` for RP2350A/Pico 2; `rp235xb` is for RP2350B. The inherited
layout assumes 4 MiB flash, 512 KiB main RAM, a 12 MHz MCU crystal, and a
150 MHz system clock. Confirm the board variant and flash before programming.

For a target build without UF2 packaging:

```sh
cargo build --locked -p cmx918-firmware --bin cmx918-audiocat --release \
  --target thumbv8m.main-none-eabihf --features device,rp235xa
```

## Wiring and receiver configuration

I2C0: **GP0 SDA, GP1 SCL**, 400 kHz, address `0x55`. PIO0/SM0 receives SPI
on **GP8 data, GP9 active-low CS, GP10 SCLK**; DMA0 owns the capture ring.
CMX918 supplies CS and clock. No GPIO reset line is assumed. Preserve common
ground, bus isolation, and the inherited 38.4 MHz CMX918 reference assumption.
Physical header numbers are in [upstream wiring](../docs/upstream/HARDWARE.md).
The old I2S output pins GP20/21/22 are unused.

Boot configures 14.200000 MHz USB, 10 kHz CMX918 bandwidth, 24 kcomplex
samples/s, crystal cap code 8, and the upstream fixed gain / automatic
PLL-calibration configuration. Only the demodulated audio is decimated to
12 kHz. Capture continues with the audio interface closed, discarding output.
The radio remains controlled over CDC independently of whether a PC records.

Frequency and mode setters stop DMA, invalidate the audio queue, configure
and verify the chip, reset DSP, restart DMA and unmute. The CAT state changes
only after successful register programming/readback and unmute. SPI progress
is checked separately; an absent stream faults after 500 ms. No-progress,
PIO framing/FIFO, DMA, I2C and calibration failures leave silence and working
CAT control. Send `ZZRX;` or a frequency/mode setter to retry. `ZZST;` reports
armed/running status and faults; it is not proof of RF reception.

## DSP and USB

See [architecture](../docs/ARCHITECTURE.md) and [CAT protocol](../docs/CAT.md).
The driver, DSP, parser, packetizer and descriptor checks run in host tests.
The 32 KiB DMA ring and its hardware fault checks are inherited; block size
is reduced to 48 complex samples (2 ms) for audio latency. DMA cursors retain
the conservative upstream 8.192 ms maximum poll interval.

USB identity `c0de:0919` is a private, unallocated development VID/PID,
distinct from upstream `c0de:0918`. The ROM chip ID supplies the USB serial.
UAC1 has one mono 16-bit little-endian recording stream at nominal 12 kHz.
CDC uses Embassy's ACM descriptors and accepts line coding without coupling
baud rate or DTR to audio. No vendor USB driver is required by the descriptor
design; OS interoperability still needs testing.

## Debugging

`defmt` traces use nonblocking RTT, with a 4096-byte buffer. Keep the matching
ELF for decoding. There are no per-sample logs. `DEFMT_LOG=debug` changes the
compile-time filter. For an identified SWD probe, `probe-rs attach --chip
RP235x --probe PROBE_ID artifacts/rp235xa/cmx918-audiocat.elf` attaches to
already running firmware. `probe-rs run` would program hardware; no flashing
was performed as part of this implementation.
