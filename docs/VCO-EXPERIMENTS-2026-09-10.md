# VCO experiment branch hardware check

This check used branch `vco-experiments`, commit `b038fe3`, on receiver
`C1E27EA41B7ECCC3` (RP2350A) with probe `E46214B0A7101C28`. The image was
built as `artifacts/rp235xa/cmx918-audiocat.elf` and flashed with
`probe-rs run --verify`.

The branch accepts 70 kHz–130 MHz and writes CMX918 `$A2=0x02` after PLL/VCO
calibration. That selects `HF_IN` (bit 1) and clears `VHF_IN` and `LF_MF_IN`
(bits 0 and 2). The configure routine verifies the `$A2` readback and
`STATUS` bit 3 (`PLL_LOCK`) before CAT acknowledges a setter.

Endpoint checks through USB CDC CAT completed successfully:

| Requested frequency | CAT result | `ZZST` result |
| ---: | --- | --- |
| 70,000 Hz | `FA00000070000;`, `MD2;` | running, error `00` |
| 130,000,000 Hz | `FA00130000000;`, `MD2;` | running, error `00` |

The intervening 14.074 MHz retune also succeeded. The audio FIFO counters
were nonzero during host retunes, while receiver fault and error counters
remained zero. This is a PLL-lock/configuration check only; it does not prove
usable RF sensitivity with the HF input at either out-of-spec endpoint.

The subsequent [HF input check below 2 MHz](LF-HF-INPUT-CHECK-2026-09-10.md)
found no wanted tone at 474.2 kHz or 1.9 MHz, while reception worked above
2 MHz. Enabling the HF LNA alone does not establish the complete low-frequency
signal route; the endpoint lock results above must not be read as RF validation.
