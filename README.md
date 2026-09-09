# CMX918 USB audio / CAT receiver

RP2350 Rust + Embassy firmware derived from `../cmx918sdr`.
Target: mono signed 16-bit USB Audio Class recording at 12 kHz and a CDC ACM
serial port for Kenwood-style frequency and USB/LSB mode control.
AM and FM demodulation are outside this version's scope.

See [provenance](docs/PROVENANCE.md). Implementation is in progress.
