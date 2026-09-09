# Validation

## Offline checks

```sh
cargo fmt --all -- --check
cargo test --locked -p cmx918-firmware
cargo clippy --locked -p cmx918-firmware --all-targets -- -D warnings
cargo clippy --locked -p cmx918-firmware --release \
  --target thumbv8m.main-none-eabihf --features device,rp235xa \
  --bin cmx918-audiocat -- -D warnings
python3 scripts/generate_dsp.py --check
python3 scripts/build_firmware.py --variant rp235xa
python3 scripts/build_firmware.py --variant rp235xb
```

The suite includes 20 library tests and one USB integration test:

- Chip configuration encoding, register ordering, coefficient commits,
  crystal trim, readback, signed RSSI, calibration timeouts and I2C failures.
- Signed I/Q byte order, DMA ring wrap and unknown-overrun detection.
- CAT fragmentation/coalescing, case handling, malformed input, bounds,
  resynchronization after overflow/inactivity and fixed-width IF/status replies.
- Both sidebands, unwanted-sideband rejection, DC and alias-band rejection,
  fine tuning, exact 2:1 output count, saturation and history reset.
- 30 seconds of simulated audio packetization per clock rate, including
  ±500 ppm clock drift, checking every sample for loss/duplication/reordering;
  FIFO overrun, underrun and configuration flush behavior.
- The actual USB builder with a mock driver: two IAD functions, four interfaces,
  zero-bandwidth/audio alternates, terminal routing, PCM type/rate, endpoint
  size/synchronization, sampling-rate requests, CDC request isolation, and
  suspend/resume/alternate/reset callbacks.

These tests run entirely offline. They do not open USB, serial or SWD devices.
The USB mock does not emulate bus timing or prove host-driver compatibility.

## Hardware acceptance still required

No hardware was flashed or exercised for this fork. The latest upstream board
has an unresolved lack of SPI samples; start with
[its diagnostic report](upstream/REFLASH-2026-09-09.md).

1. Identify the receiver, board variant, flash, SWD probe and wiring. Confirm
   CMX918 CS/data/clock reach GP9/GP8/GP10 and the separate I2C GP0/GP1 bus.
   Program only the appropriate variant's image and retain the matching ELF.
2. Inspect USB descriptors (`lsusb -v -d c0de:0919` on Linux). Confirm native
   audio capture and CDC appear together and the audio hardware format is
   mono S16_LE at 12000 Hz. Reject a 48000 Hz hardware-rate request. Desktop
   audio servers may resample, so use ALSA `hw` for native-rate checks.
3. Use CAT to query FA/MD/IF/ZZST, set both modes and several frequencies,
   including nonmultiples of 100 Hz. Read back settings. Verify `MD4;`,
   `MD5;`, `TX;` and invalid frequencies return `?;` without changing state.
   Wait more than 500 ms and verify capture stays armed before RF tests.
4. With an identified signal generator, confirm USB receives a carrier above
   the dial and rejects the same offset below it; confirm the reverse in LSB.
   Test ±450, ±1000 and ±2500 Hz. Include HF below/above the 2 MHz LO transition
   and VHF, source-away/back checks and the useful RF range. Fine tuning must
   move the audio tone by the requested Hz, with its sign appropriate to mode.
   Follow inherited attenuation and generator-restoration constraints in
   [upstream testing](upstream/TESTING.md); scope experiments before running them.
5. Record at least 30 minutes under host load while repeatedly querying CAT.
   Measure actual sample clock and audio continuity; verify stable counters,
   no unexpected silence, DMA faults or software scheduling overruns. Check
   passband, image rejection, clipping and end-to-end latency on real audio.
6. Retune and switch modes while recording; confirm bounded silence and no
   persistent stale audio. Close/reopen capture without closing CDC, toggle
   DTR, suspend/resume and unplug/replug. The audio function must recover without
   a CAT START command; a receiver fault still requires FA/MD or `ZZRX` retry.
7. Induce missing SPI, I2C failure and host stalls with an agreed hardware test
   setup. Confirm silence, bounded failure, responsive CAT and meaningful
   counters. Recovery must never silently report failed tuning as applied.
8. Repeat enumeration, capture and CDC checks on Windows and macOS. Full
   compatibility with a particular CAT application is a separate acceptance
   test because this implements only a small Kenwood subset.

Do not label software-generated IQ tests as reception measurements. The nominal
frequency range, clock rate and filter response are implementation settings,
not hardware qualification claims.
