# CDC CAT protocol

USB CDC ACM uses semicolon-terminated ASCII, without a greeting or echoed
commands. Uppercase and lowercase commands work. CR/LF between commands are
ignored. USB packet boundaries do not delimit commands. Any host-selected baud
rate is accepted; 115200 8N1 is a convenient convention. DTR is not required.

| Command | Behavior / example reply |
| --- | --- |
| `FA;` | Read dial frequency: `FA00014200000;` (11 digits, Hz) |
| `FA00007074123;` | Set 7,074,123 Hz; success has no reply |
| `MD;` | Read mode: `MD1;` LSB or `MD2;` USB |
| `MD1;` / `MD2;` | Set LSB / USB; success has no reply |
| `IF;` | 38-byte TS-480-style status including frequency, RX and mode |
| `ID;` | `ID020;`, TS-480 compatibility identity for this limited subset |
| `AI;` / `AI0;` | `AI0;` / disable unsolicited status; no other AI modes |
| `FR;` / `FR0;` | `FR0;` / select the only receive VFO, A |
| `RX;` | Acknowledge receive-only operation, no state change |
| `ZZST;` | Project-specific status and discontinuity counters, below |
| `ZZRX;` | Retry configuration/capture using the last accepted dial/mode |

Other commands, malformed parameters and unsupported modes return `?;`.
In particular `MD4;` (FM), `MD5;` (AM), CW modes, `TX;`, VFO B and split are
unsupported. This is not complete TS-480 emulation; applications that require
additional commands may need configuration or an adapter. RIT/XIT, scan, split,
memory and tones are reported inactive in `IF`. There is no transmitter.

The [Kenwood TS-480 PC control reference](https://www.kenwood.com/i/products/info/amateur/ts_480/pdf/ts_480_pc.pdf)
defines the command framing and FA/MD/IF fields used by this subset.

## Frequency and failures

The accepted range is 150,000–108,000,000 Hz in 1 Hz steps, with no claim of
validated reception throughout that range. Nearest-100-Hz chip tuning plus
an on-device NCO implements the requested dial. Crystal accuracy is not
corrected automatically. Boot defaults to 14.200000 MHz USB and crystal trim
code 8. Settings are volatile and not saved to flash.

Setters acknowledge success by silence, following Kenwood convention. Send
`FA;MD;ZZST;` after a setter to verify the applied state. Failed hardware writes,
readback, calibration or unmute return `?;`, retain the previous CAT dial/mode,
and leave audio silent. Queries report the last accepted state, not a newly
verified RF center. If startup fails, that state is the boot default. Once SPI
capture starts, missing samples can fault later; check `ZZST` after at least
one second. `RX;` does not recover faults; `ZZRX;` does.

`ZZSTr,ee,ffffffffff,uuuuuuuuuu,oooooooooo,ssssssssss;` fields are:

- `r`: 1 if capture is armed/running, 0 if stopped/faulted. This is not a signal
  or PLL-lock indication and is independent of whether a host records audio.
- `ee`: last error (00 none, 05 I2C, 06 timeout, 07 register readback, 09 capture).
- `f`: cumulative receiver faults.
- `u`: audio FIFO underruns after initial priming.
- `o`: audio FIFO overruns, which discard queued audio.
- `s`: audio USB write errors/timeouts, which also discard queued audio.

All counters are decimal, saturating, and reset on reboot. Initial priming,
explicit retunes and USB lifecycle changes are deliberate discontinuities and
do not increment loss counters. During absent SPI or receiver faults the USB
audio interface keeps delivering silence.

Frames are limited to 32 characters before `;`. Overflow, invalid bytes and
an incomplete frame idle for one second poison the frame until the next `;`,
which returns `?;`. A subsequent complete frame is accepted. Replies are at
most 53 bytes. USB resets discard parser state and suppress stale replies.
Allow an 8-second host timeout for startup plus a configuration request.
