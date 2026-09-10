# HF input check below 2 MHz

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
