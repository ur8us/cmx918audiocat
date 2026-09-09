# Project instructions

- Rust + Embassy on RP2350; preserve CMX918 I2C control GP0/GP1 and PIO SPI
  slave I/Q capture GP8/GP9/GP10 with DMA. CMX918 supplies clock and CS.
- This fork outputs demodulated mono PCM at 12 kHz over USB Audio Class,
  with CDC ACM Kenwood CAT frequency and USB/LSB control. AM/FM are excluded.
  The user's audio scope supersedes upstream raw-I/Q-only transport rules.
- Keep chip control, capture, DSP, CAT and USB separable and bounded.
  Retune/mode changes discard stale audio; USB stalls cannot block capture/control.
- Source project and investigation are read-only. Do not copy vendor PDFs,
  bulk captures, build directories or upstream Git history.
- Read docs/PROVENANCE.md and historical docs/upstream references for hardware
  assumptions. Software tests and builds are not hardware qualification.
- Preserve MIT licensing and source provenance. Document non-obvious constants.
- Before firmware commits run cargo fmt, host tests and RP2350 target build;
  review diffs and commit each coherent milestone. Preserve unrelated changes.
- No hardware flashing or generator experiments without an identified scoped
  hardware task. Do not push unless requested.
