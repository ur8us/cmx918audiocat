# Offline validation — 2026-09-09

| Check | Result |
| --- | --- |
| Rust formatting | Pass |
| Host library tests | 20 passed |
| Actual USB descriptor builder / control handler test | 1 passed |
| Host Clippy, all targets, warnings denied | Pass |
| RP2350A Clippy, warnings denied | Pass |
| RP2350A release ELF / UF2 generation and picotool inspection | Pass |
| RP2350B release ELF / UF2 generation and picotool inspection | Pass |
| DSP coefficient regeneration check | Pass |
| Python helper syntax and CAT `--help` | Pass |
| Flash, USB enumeration, CDC host traffic, physical audio and RF | Not performed |

Both variants target Cortex-M33 `thumbv8m.main-none-eabihf` using Rust 1.90.0
and the committed Cargo.lock. `picotool info` identifies ARM Secure RP2350
images. Artifacts are ignored by Git; their `build.json` records the exact
source commit, dirty flag and SHA-256 values. Use the artifacts associated
with the final clean source commit, not intermediate development builds.

DSP tests validate sideband selection and software sample processing, and
the packetizer tests cover finite simulated clock drift. They do not establish
processor headroom, electrical timing, RF performance or OS interoperability.
See [hardware acceptance](TESTING.md) and the unresolved upstream
[SPI diagnostic](upstream/REFLASH-2026-09-09.md).
