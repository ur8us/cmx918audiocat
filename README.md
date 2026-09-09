# CMX918 USB audio / CAT receiver

RP2350 firmware derived from `../cmx918sdr`, using Rust and Embassy. It exposes:

- **USB audio recording:** mono, signed 16-bit PCM at **12,000 samples/s**.
- **USB CDC serial:** a small Kenwood CAT subset for frequency and **USB/LSB**.

I/Q is captured from the CMX918 at 24 ksps, demodulated and filtered on the
RP2350, then decimated to 12 kHz audio. AM and FM are not implemented.
Default tuning is **14.200000 MHz USB**. CAT accepts 1 Hz dial steps using
100 Hz chip tuning plus digital fine tuning.

**Status:** flashed and verified on RP2350A receiver `C1E27EA41B7ECCC3`.
Linux audio/CDC, CAT control, and a 60-second 12 kHz capture pass. One initial
I2C NACK recovered on retry. Generator tests at **14.074 MHz** pass for USB, LSB
and fine tuning. A post-retune transient was fixed and retested; sustained
operation is not yet qualified. See the
[hardware report](docs/HARDWARE-2026-09-09.md) and [provenance](docs/PROVENANCE.md).

## Build

```sh
cargo test --locked -p cmx918-firmware
python3 scripts/build_firmware.py --variant rp235xa
```

Outputs: `artifacts/rp235xa/cmx918-audiocat.uf2` and the matching `.elf`.
Use `--variant rp235xb` for RP2350B. Confirm the board/flash before programming.
The helper only builds and packages; it does not flash.
See [firmware and wiring](firmware/README.md).

## Use from a PC

After programming the appropriate image and connecting USB, select **CMX918
Audio CAT Receiver** as a recording/input device. Request mono 12 kHz PCM;
this is a receive-only sound card. On Linux, `arecord -l` lists capture devices.
After identifying its card number, for example card 2:

```sh
arecord -D hw:2,0 -f S16_LE -c 1 -r 12000 -d 10 receive.wav
```

CDC should appear as `/dev/ttyACM*` on Linux, a COM port on Windows, or
`/dev/cu.usbmodem*` on macOS. Native audio/CDC drivers are intended; platform
interoperability has not yet been measured. Linux serial access normally
requires the distribution's serial-device group (often `dialout`).

Install pyserial into your Python environment to use the supplied CAT helper:

```sh
python3 -m pip install pyserial
python3 scripts/cat.py --list
python3 scripts/cat.py --mhz 14.074 --mode USB
python3 scripts/cat.py --frequency 7074123 --mode LSB
```

It discovers only USB `c0de:0919` receivers; use `--serial` or `--port` if
necessary. `--frequency` (or `--fq`) uses integer Hz; `--mhz` accepts exact
decimal MHz. Mode names are case-insensitive. Without setters it reads
frequency, mode and receiver status.
Alternatively send raw commands from a serial terminal:

```text
FA00007074123;MD1;FA;MD;ZZST;
```

Successful setters are silent; queries return semicolon-terminated replies.
`MD1` selects LSB; `MD2` selects USB. Unsupported commands/modes return `?;`.
See [the complete CAT subset and fault recovery](docs/CAT.md).

[Architecture and DSP](docs/ARCHITECTURE.md) ·
[Validation and hardware acceptance](docs/TESTING.md) ·
[MIT license](LICENSE)
