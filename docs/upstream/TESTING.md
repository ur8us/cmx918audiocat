# Frequency, sample-rate, and FFT testing

Live progress is tracked in [BRINGUP-2026-09-09.md](BRINGUP-2026-09-09.md).
MCU boot/USB/RTT pass; a follow-up confirms host USB access and 20 successful
read-only I2C probes. Baseline programming/calibration and short non-synthetic
SPI captures also pass at 100 MHz, 10 kHz bandwidth and 24 ksample/s after a DMA
count fix. This does not qualify any RF frequency or sample rate.

This repository has executable offline test preparation and initial firmware
with a Python USB capture adapter. See [build and defmt instructions](../firmware/README.md)
and [capture commands](PROTOCOL.md). Neither the receiver nor a CH552/CH55x serial
device was available during preparation. Synthetic results are never RF
reception results. An automated physical matrix run remains a bring-up task;
the matrix, generator helper, capture CLI and FFT checker are separate tools.

## Install and run offline

Use Python 3.10+ on Linux, Windows, or macOS. From the repository root, create a
virtual environment and use its Python interpreter (activate it using your OS's
normal procedure):

```sh
python -m venv .venv
# Activate .venv, then:
python -m pip install -e .
python -m unittest discover -s host/tests -v
python -m cmx918_test matrix matrix-new.csv
python -m cmx918_test simulate simulation-new.json
```

These commands open no serial ports. Output files use exclusive creation;
choose a new filename for a new run. The checked-in
[frequency-matrix.csv](../tests/frequency-matrix.csv) is generated with the
default 1,000 Hz beat offset. Use `--offset-hz 500`, for example, to change it.
“1 up or down” is provisionally interpreted as **1 kHz**, not 1 Hz or 1 MHz.
Both signs are tested by keeping the receiver center fixed and setting the
generator to center − 1 kHz and center + 1 kHz. This is equivalent in relative
tuning and exercises exact receiver band edges without shifting them.

## Coverage matrix

47 receiver center frequencies × 7 bandwidth/rate profiles × 2 beat signs gives
**658 cases**. There are 168 cases blocked by LF/MF wiring (some also by generator
range) and 490 candidate HF/VHF cases. Nothing is silently omitted. The centers
include 100/150 kHz, LF/MF broadcast anchors, 2 MHz and 40 MHz input boundaries,
HF amateur/broadcast anchors, 2.3/43 MHz synthesizer regions, VHF broadcast,
108 MHz, and experimental 109/120/125/128/130 MHz coverage.

| Bandwidth (kHz) | Included sample rates (ksample/s) |
| --- | --- |
| 5 | 12, 24 |
| 10 | 24, 48 |
| 20 | 48, 96 |
| 100 | 240, subject to USB throughput qualification |

480 and 960 ksample/s are outside the revised scope. Standard 200 kHz-bandwidth
profiles require those excluded rates, so the matrix does not include them.
These are candidate hardware profile combinations, not claims that every
frequency/profile combination has been demonstrated. Read back the selected
profile and actual rate; record unsupported combinations with an explicit reason.

The CSV includes receiver/source frequencies, source channel, RF input, bandwidth,
rate, expected signed beat, source-away frequency, experimental-range flag,
USB qualification requirement, and blockers. Channel `0` means no wired LF/MF
source; it is never a valid oscillator command. Frequencies and rates use Hz.
At 240 ksample/s the payload is 960,000 bytes/s before framing. Require a
30-minute no-loss native USB test with concurrent control/status before running
or advertising that profile. Otherwise record it as unavailable. The lower rates
also need streaming qualification.

## Generator control

Captured command help is in
`~/WORK/drm1000-sniff/captures/generator-l2-matrix/events.jsonl` (`generator_rx`);
the previous controller is `generator_response_probe.py`.

- Identify serial metadata containing **CH552**; also recognize **CH55x** because
  the supplied reference device enumerated as `Deqing CH55xduino CH55x`.
  Zero or multiple matches is an error; select an explicit port in the eventual
  acquisition runner instead of guessing.
- Serial: **115200 baud, 8 data bits, no parity, 1 stop bit**, CRLF commands.
- `?` reports profile, OCXO reference frequency, multiplier, and all three outputs.
- `fq 1 <Hz>` sets VHF source; `fq 2 <Hz>` sets HF source.
- `load` applies the settings; the previous investigation also records persistent
  storage behavior. Wait five seconds and query `?` to verify the result.
- Only channels 1 and 2 are changed. Preserve profile, OCXO, multiplier, and
  channel 3. Snapshot original values and restore channels 1/2 in a `finally` or
  context exit, including when tuning, capture, or analysis fails. Log restoration
  failures; never hide them behind an earlier capture error.

`cmx918_test.generator` implements discovery, bounded serial exchanges, complete
status parsing, verified tuning, and restoration. Its `Generator` receives an
exchange callable, so unit tests substitute an oscillator without accessing
hardware. `SerialCommands` accepts a pyserial port opened with `timeout=0.1`
and `write_timeout=1`; a future runner must also log its TX/RX events. No live
generator command is issued by `matrix`, `simulate`, or `analyze`.

The 330 kHz–330 MHz software guard comes from the previous runner; it is not
independent proof of the oscillator's full operating range. Do not “test” LF by
commanding unsupported source frequencies or interpreting a harmonic response
as a fundamental.

## Acquisition procedure when the receiver is available

1. Identify receiver/firmware and CH552 device. Save the generator snapshot,
   exact matrix, board wiring, attenuation, USB capabilities, and test thresholds.
   Validate I2C control/readback and external SPI framing before RF measurements.
2. Qualify selected rates using a known data pattern through capture/USB; reserve
   bandwidth for status/control. Report dropped/unknown samples and resets.
3. Execute the matrix in ascending frequency order, then descending order to
   expose history-dependent setup. Repeat after reset. Keep the configured RF
   input explicit and skip blocked rows with reasons, never as successful tests.
4. For each case, tune the proper generator channel, apply `load`, wait five
   seconds, and verify all output settings. Apply receiver settings, verify actual
   frequency/rate/bandwidth and PLL/calibration status, then discard settling and
   any partial or old-generation sample blocks. Allow an initial two-second
   receiver settling interval, refined later from measurements.
5. Capture at least one second of raw I/Q with sequence/loss metadata. With the
   receiver unchanged, tune the same source to `away_generator_hz`, settle and
   capture again, then return to the original source frequency and repeat. Keep
   gain fixed for comparison where practical and record gain/AGC state throughout.
6. Run the signed complex FFT checks below and save results with the three raw
   captures. Actual readback values determine the expected beat
   `generator_hz - receiver_hz` and FFT sample-rate axis. A mismatch against the
   requested matrix is a configuration failure, not a silent change of test case.
7. Restore original generator settings and close ports on success, error, or
   interruption. Record incomplete sessions and restoration errors explicitly.

The eventual runner must gate FFT acceptance on matching applied settings,
PLL/calibration readiness, complete sample accounting, fresh configuration
generation, and no loss/discontinuities. A pretty spectrum alone cannot establish
these conditions. Do not call `analyze` a complete hardware pass without them.

## FFT analysis of saved data

Input is raw **signed 16-bit little-endian `I0,Q0,I1,Q1,...`**, four bytes per
complex sample. Convert the chip's high-byte-first words before writing this
format. This tool does not parse USB transport headers. Preserve raw captures
and a sidecar recording actual settings, byte order, I/Q orientation, sequence
range, gaps, firmware, generator readback, and capture phase.

```sh
python -m cmx918_test analyze on.iq --sample-rate 48000 --expected-hz 1000
python -m cmx918_test analyze on.iq --sample-rate 48000 --expected-hz -1000 \
    --away away.iq --returned returned.iq
```

The checker uses `I + jQ`, removes each window's mean, applies a Hann window,
computes a two-sided complex FFT, and averages window powers. Frequency bins are
at most 10 Hz apart. It reports unused trailing samples, DC values, clipping,
strongest non-DC peak, expected-tone level in digital dBFS, tone prominence above
median noise-bin power, and mirrored-tone rejection. These are digital spectral
metrics; prominence is not calibrated RF SNR and dBFS is not dBm. The sign follows
[NumPy's FFT convention](https://numpy.org/doc/stable/reference/routines.fft.html).

Initial acceptance thresholds, to retain with each hardware report:

- Strongest non-DC peak within ±100 Hz of the expected **signed** beat.
- Wanted tone at least 20 dB above median noise-bin power.
- Wanted tone at least 15 dB above its mirrored frequency; no clipped samples.
- Source-away measurement drops the expected-bin peak by at least 10 dB; source
  return recovers to within 3 dB and passes the signed-tone checks again.

The physical receiver's I/Q sign must be established once with both offsets;
if the chip/configuration reverses it, explicitly correct/declare the orientation
in the acquisition adapter. Do not accept either sign indiscriminately per case.
The default tolerance may need adjustment for measured reference accuracy, but
record any change before testing. Narrower offsets also require narrower
tolerances: pass `--tolerance-hz 25` for a 100 Hz offset. Simulation automatically
uses the smaller of 100 Hz or one quarter of the configured beat offset.

Owner update, 2026-09-09: resume the coarse sweep from 50 MHz with
`--tolerance-hz 250`. The live runner retains 100 Hz as its default and records
the explicit tolerance in each result, FFT report and session. This parameter
sets both the signed-peak limit and the wanted/mirror spectral search windows;
the other acceptance thresholds stay unchanged. Reports state the actual
tolerance and identify any offline reassessment of previously recorded I/Q.

Offline tests cover matrix coverage/routing, USB exclusions, generator range,
status parsing, both-channel tuning and restoration after failures, signed/endian
conversion, all five candidate rates, DC, off-bin tones, I/Q reversal, wrong tone,
noise-only data, image leakage, clipping, malformed data, and source-away/back
rejection of stationary spurs. `simulate` exercises all matrix rows with synthetic
I/Q and labels its report `synthetic_only`, with zero hardware runs. It does not
model RF tuning, PLL behavior, filtering, SPI timing, USB throughput, or oscillator
hardware. Linux installation and offline tests are checked first; Windows/macOS
installation and physical tests remain to be run.

## Preparation validation — 2026-09-08

Linux, Python 3.12.6, NumPy 2.1.3, pyserial 3.5:

- 29 unit tests passed, including serial fragmentation/timeouts, session lifetime,
  and checked-in CSV regeneration consistency.
- All 658 synthetic matrix cases passed the on/away/return FFT checks; **zero
  hardware cases were executed**. Blockers remain unchanged by simulation.
- CLI accepted an expected +1 kHz tone and returned a failing exit status when
  the same capture was checked against −1 kHz.
- Editable installation and portable Python wheel construction passed; wheel
  contents include the host package and exclude the test directory.
- Local documentation links and Git whitespace checks passed.

The environment reused installed NumPy/pyserial for offline checks. A clean
dependency installation on other systems and physical receiver/generator tests
remain outstanding.

## Firmware implementation validation — 2026-09-08

The firmware addition passes 12 host-side Rust tests and 39 total Python tests.
Release builds and warning-free Clippy checks pass for both RP2350A and RP2350B
on `thumbv8m.main-none-eabihf`; formatting and host-side Rust Clippy pass too.
An additional build enables debug/I2C trace filters. `picotool` recognizes both
ELF boot headers and converts them to UF2; ELF inspection finds RTT and `defmt`
metadata. The USB CLI's editable install and portable wheel build pass on Linux.
New cases exercise rate/frequency rejection, register ordering and I2C/calibration
failures, SPI word conversion, DMA cursor wrap/overrun, the shared command fixture,
fragmented/coalesced sample messages, signed I/Q, source/generation/sequence
mismatches, synthetic-counter wrap, incomplete capture metadata and STOP on failure.
These are software checks, not a simulation of the electrical PIO/DMA engine.

Firmware logs use `defmt` through nonblocking RTT. During on-board acceptance,
run with the probe attached and detached, disconnect it during streaming, and
verify the receiver remains responsive. Also exercise USB suspend/disconnect,
host stalls, interrupted commands, failed calibration and stopped SPI clock.
All affected captures must stop or show a detected failure; never report an
unknown-loss interval as continuous. Keep the matching ELF and decoded logs
with the build hash and measurement report. Logs alone cannot establish RF
reception or timing margin.

## Live acquisition runner

`python -m cmx918_test.live` explicitly opens the receiver and the oscillator
selected by CH552/CH55x identity. It snapshots/restores generator settings and
logs source commands/readbacks, receiver commands/replies, each raw I/Q phase,
and a result for every selected row. It configures the receiver once per
on/away/return triplet; subsequent phases only stop/start capture and check fresh
chip status. Each source change settles for five seconds in the generator
helper, followed by two seconds of receiver settling and at least one second
of capture. STOP drains trailing USB traffic before closing the host handle.

Example commands for the recorded September 9 session (output directories must
not already exist):

```sh
PYTHONPATH=host python3 -m cmx918_test.live captures/NEW-orientation \
  --serial A71CD5C0A010DEF2 --mode orientation \
  --usb-gates captures/rf-matrix-20260909/usb-gates.json
PYTHONPATH=host python3 -m cmx918_test.live captures/NEW-ascending \
  --serial A71CD5C0A010DEF2 --mode ascending --iq-sign 1 \
  --usb-gates captures/rf-matrix-20260909/usb-gates.json
```

The USB gate file records each attempted rate, `passed`, `seconds_requested`,
and capture evidence. A missing/failed pattern gate blocks a rate; 240 ksps
also requires at least 1800 seconds. Short lower-rate checks permit experimental
RF measurements but do not qualify sustained operation or advertise rates.
`--frequencies` and `--profile BANDWIDTH RATE` select explicit diagnostic subsets.
Explicit centers need not occur in the default matrix: they must be distinct,
within 100 kHz–130 MHz, and aligned to 100 Hz. They retain the same input routing,
profile/rate gates, experimental flags and both beat signs. For the owner's
coarser sweep, use `--frequencies 10000000 30000000 40000000 50000000 70000000 130000000`.
This gives 84 rows per direction: 72 acquisition triplets and 12 blocked
100 kHz / 240 ksps rows with the current USB gate.
`--mode descending` reverses center order. `--reset-before` performs the CMX918 software reset and a five-second settling
wait before the selected sweep; its command/reply are recorded. `--xtal-caps`
selects a documented crystal load-capacitance code, recorded in session and
applied configuration flags. Establish the trim before the matrix and retain
uncorrected diagnostics separately.
`--stop-file PATH` finishes the current case, then restores the generator.
SIGINT/SIGTERM also attempt cleanup and record incomplete sessions.

`RF_CHECKS_PASS` means the acquisition and on/away/return FFT checks passed;
`hardware_qualified` remains false. `RF_CHECKS_FAIL`, `ERROR`, `BLOCKED`, and
`NOT_RUN` remain distinct. I/Q sign is fixed before the sweep, never selected
independently to make a case pass. Raw files are unchanged if FFT conjugation
is explicitly selected with `--iq-sign -1`.


## Receiver waterfall command

`python3 scripts/capture_waterfall.py 14.134` records ten seconds of physical
I/Q at receiver center 14.134 MHz, 5 kHz bandwidth and 24 ksps, then writes a PNG.
The command does not operate the generator. It uses the existing framing/loss
checks and gates capture on fresh PLL/calibration status and applied settings.
It closes USB and attempts STOP on success, failure and Ctrl-C; failures retain
session/capture metadata and do not produce a success PNG.

The FFT uses fixed I+jQ, 4,096-sample Hann windows and a 1,024-sample hop.
At most 1,000 time rows are retained for display by averaging spectral power;
raw ci16 samples are unchanged. Mean removal is for display only. The image
shows the configured filter span and reports clipping and FFT resolution.
The frequency/time axes use the programmed sample rate. A visible peak is not
an RF qualification result. Duration is bounded to 1–600 seconds and this CLI
allows only the 12/24/48/96 ksps profiles; the failed 240 ksps gate remains blocked.

Validation on 2026-09-09: 62 offline host tests passed, including exact frequency
parsing, no USB access on invalid input, failed-capture cleanup, PLL-lock gating,
signed FFT peaks, bounded display rows, and PNG rendering from saved fixtures.
A live invocation at 14.134 MHz captured 240,128 physical I/Q pairs, generated
the PNG, and left the receiver stopped with zero reported dropped blocks/faults.
Evidence: `captures/waterfall-cli-validation-20260909/`. Generator settings were
not changed for this CLI test. A clean install on Windows/macOS remains untested.
