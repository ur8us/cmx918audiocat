# RP2350 firmware

The root `AGENTS.md` applies. This directory is reserved for Rust + Embassy
firmware. SPI1 RX/CS/SCLK must use GPIO8/9/10 from the supplied receiver example.
Other board wiring remains to be confirmed; see `../docs/upstream/HARDWARE.md`.

- Start with the RP2350 Cortex-M33 target; select the exact chip/board, flash
  layout, clocks, boot configuration, and pins before adding the build scaffold.
- Use async I2C for control and hardware-driven capture with DMA. Evaluate the
  fixed SPI peripheral against the real CS/clock timing; PIO is an allowed SPI
  slave implementation. Never bit-bang samples in an Embassy task.
- Verify clock timing at the highest supported setting, including input
  synchronization, sampling margin, FIFO depth, and DMA handover. Merely finding
  an Embassy API or compiling a PIO program is not a throughput demonstration.
- Give the data path bounded owned buffers. Keep register control serialized
  and independent of host backpressure. Avoid per-sample allocations or logs.
- Use `defmt` over RTT for traces and debug messages through the SWD probe.
  Keep RTT nonblocking even when the probe disconnects; never log per sample or
  USB data packet. Keep the matching ELF for decoding. See `README.md` here.
- Bound calibration/PLL waits, preserve reserved bits as documented, and
  propagate I2C errors. Reject frequency overflow before packing the 21-bit
  field; expose the applied 100 Hz quantization explicitly.
- During reconfiguration, mute/stop at a known boundary, discard partial frames,
  apply and verify settings, then restart with a new configuration generation.
- Document every unsafe block's ownership, alignment, lifetime, and peripheral
  invariants. Use host-testable pure code for tuning arithmetic and codecs.
- Before committing firmware changes, run formatting, relevant host-side tests,
  and a build for the pinned RP2350 target. Add appropriate lint checks once the
  build exists. Clearly distinguish compile checks from on-board verification.
