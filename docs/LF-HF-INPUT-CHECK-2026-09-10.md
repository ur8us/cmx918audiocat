# HF input check below 2 MHz

**Follow-up result: fixed and RF-tested after the receiver power-cycle.**
The final firmware selects HF mixer routing as well as the HF LNA. A
475.2 kHz generator signal now produces 998 Hz audio at 474.2 kHz USB.
The initial failed tests and interruption below are retained as evidence.

The owner reported no 1 kHz audio with a 475.2 kHz generator signal connected
to HF_IN and the receiver at 474.2 kHz USB. Testing used receiver
`C1E27EA41B7ECCC3`, RP2350A, and the `vco-experiments` firmware at `b038fe3`.

## Findings on the original branch firmware

- CAT confirmed `FA00000474200;`, `MD2;` and running status, error zero.
- PipeWire connected the receiver to the default playback sink. A recording
  from the same source contained noise and no prominent 1 kHz carrier.
- Generator readback was channel 1 = 475,200 Hz, channel 2 = 25,000,000 Hz,
  channel 3 = 24,000,000 Hz. Earlier wiring notes place channel 2 on HF_IN;
  the current physical output connection was not yet confirmed by the owner.
- Setting both channels 1 and 2 to 475,200 Hz still produced no tone in USB
  or LSB. Moving channel 2 to 525,200 Hz did not reveal a wanted tone either.
- With both channels at dial + 1 kHz, USB reception produced a prominent
  949 Hz tone at a 14.074 MHz dial and 992 Hz at 2.1 MHz. At 1.9 MHz the
  recording contained noise again. These higher-frequency recordings were
  clipped, so they establish a response, not amplitude or sensitivity.

Recordings used PipeWire, mono S16 at requested 12 kHz; these are not native
ALSA clock or gain measurements. Source gain was above unity. The speaker
loopback was temporarily muted during the strong-tone tests.

The earlier `$A2=0x02` change enables the HF LNA and disables the other
LNAs, but did not establish the complete signal route below 2 MHz. The
CMX918 datasheet D/918/2.0 p.12 also describes band-dependent routing of
both mixer outputs and quadrature LO. The measured loss below 2 MHz is
consistent with that routing remaining on the LF/MF path; this is an
inference, not a direct observation of the internal switches.

## Follow-up experiment and interruption

A candidate routing experiment first calibrates the PLL for the actual
frequency, then changes Fc to an HF-band value while retaining the IF sign,
without executing another PLL calculation. Datasheet p.28 says writing Fc
does not itself execute PLL changes. This experiment requires on-board
RF verification before it can be treated as a fix.

The original firmware encountered a configuration failure while switching
mode at 1.9 MHz, before the candidate was flashed. Subsequent configuration
attempts also failed. Flashing/restarting the RP2350 did not recover control;
RTT with trace logging showed the first write, `$03=0x01`, never completing
before the outer three-second configuration timeout. CAT remained responsive
and reported stopped/error `06`. A receiver power-cycle was requested.

The candidate patch was saved with the captures, then removed from the source.
The original branch firmware was rebuilt and reflashed with readback
verification so the unvalidated routing experiment is not left installed.

Both generator channels were restored to their original readback values;
channel 3, reference, multiplier and profile were preserved. Session scripts,
WAVs, command/reply records, FFT summaries and RTT logs are retained in the
Git-ignored `captures/lf-check-20260910/` directory. The failed range script's
cleanup encountered a receiver error but the generator context still restored
its outputs, independently verified afterward.

This check supersedes any inference that successful CAT setters, `$A2`
readback or PLL lock alone qualify HF_IN reception below 2 MHz.

## Successful follow-up after power-cycle

An initial retry returned an I2C NACK, then control recovered and configuration
completed. The routing experiment described above immediately produced a clear
998 Hz USB tone with generator channel 1 at 475.2 kHz and channel 2 still at
25 MHz. Setting both channels to 475.2 kHz was unnecessary. Subsequent tests
changed only channel 1; channel 2 stayed at 25 MHz and channel 3 at 24 MHz.
The owner identifies the generator connection as HF_IN; the earlier wiring
table's channel mapping is therefore not assumed for this session.

The final driver snapshots `$2A–$34` after actual-frequency calibration and
verifies they are unchanged after the Fc routing override. It also verifies
Fc, `$A2=0x02`, and PLL lock. At 474.2 kHz, RTT reports active L=760,
STATUS=`0x39`, and routing Fc bytes `00 4E 20` (2 MHz, high-side IF).
CAT still reports the actual 474.2 kHz dial and DSP still uses that frequency.

The final RP2350A ELF was flashed with `probe-rs run --verify`; SHA-256:
`adf672fd010f75f8f3ff3915c8009410ca5fe9deb40c6ad0c62b71e3804e8200`.
All 21 library tests and the USB descriptor integration test passed, as did
formatting, host/target Clippy with warnings denied, and the RP2350A build.

Final recordings used native ALSA `hw:CARD=Receiver,DEV=0`, mono S16_LE at
12,000 Hz, three seconds per capture. Only this receiver's PipeWire profile
was disabled for capture and restored afterward. All eleven WAVs had zero
clipped samples. Analysis discarded the first 0.5 seconds and averaged
Hann-windowed FFT powers at 1 Hz bin spacing.

| Dial / mode | Generator channel 1 | Measured tone / result |
| --- | --- | --- |
| 474,200 Hz USB | 475,200 Hz | 998 Hz, −12.23 dBFS |
| 474,249 Hz USB | 475,200 Hz | 949 Hz: exactly the expected −49 Hz shift |
| 474,200 Hz USB, dial restored | 475,200 Hz | 998 Hz; level within 0.01 dB of initial |
| 474,200 Hz LSB | 475,200 Hz | Wanted-tone band suppressed by 61.5 dB |
| 474,200 Hz LSB | 473,200 Hz | 1,002 Hz |
| 474,200 Hz USB | 473,200 Hz | Wrong-sideband suppression 76.1 dB |
| 474,200 Hz USB | 525,200 Hz | Wanted-tone band drops 73.5 dB |
| 474,200 Hz USB, source restored | 475,200 Hz | 998 Hz; level within 2 dB of initial |
| 1,900,000 Hz USB | 1,901,000 Hz | 993 Hz; previously no wanted tone |
| 2,100,000 Hz USB | 2,101,000 Hz | 993 Hz |
| 14,074,000 Hz USB | 14,075,000 Hz | 949 Hz |

Retunes to 70,000, 77,000, 135,700 and 130,000,000 Hz also passed CAT,
PLL/register checks and running status. Those were configuration/lock checks,
not RF reception tests; the generator helper does not cover the low endpoints.
This remains a branch-specific, experimentally verified routing workaround,
not a manufacturer qualification of HF_IN sensitivity throughout the range.

During the native captures, receiver fault, FIFO underrun and overrun counters
remained zero. The USB
timeout counter was initially one and rose to two across the repeated native
capture stop/reopen sequence, including the `usb-return` recording. This
session does not establish loss-free sustained streaming or resolve the
previously documented USB lifecycle issue. After desktop audio was restored
and the debug probe disconnected, counters reached one FIFO overrun and four
USB timeouts; receiver faults and underruns remained zero and audio recovered.

Final state: receiver **474,200 Hz USB**, generator outputs restored to
**475,200 / 25,000,000 / 24,000,000 Hz**, receiver Mono Input profile restored,
listening loopback unmuted, and the debug probe session closed. Final CAT
status was `ZZST1,00,0000000000,0000000000,0000000001,0000000004;`.
Exact final test script and evidence: `captures/lf-check-20260910/final.py`,
`final/` (WAVs, results and events) and `final-rtt.log`.
