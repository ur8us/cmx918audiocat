# Windows recording endpoint

The receiver uses the built-in Windows USB Audio Class 1 driver and appears
as a recording endpoint associated with **CMX918 Audio CAT Receiver**. Windows
may label the endpoint **Line In**. It carries demodulated radio audio from
HF_IN: mono, signed 16-bit PCM at 12,000 samples/s. The line-input category
does not describe an additional physical audio connector.

## Device Manager detects the receiver but applications cannot select it

The original firmware advertised USB input terminal type `0x0710` (Radio
Receiver). This is a valid USB terminal type, but Windows 7 and later create
`KSNODETYPE_RADIO_RECEIVER` endpoints as **disabled and hidden** by default.
Device Manager can therefore show the audio driver even though recording
applications have no enabled endpoint to open. Microsoft explicitly lists
this behavior in its
[endpoint activation policy](https://learn.microsoft.com/en-us/windows-hardware/drivers/audio/pkey-audiodevice-enableendpointbydefault).

The corrected firmware advertises `0x0603` (Line Connector), which maps to
`KSNODETYPE_LINE_CONNECTOR`, the exception to that LineLevel hiding policy.
The codes are defined in the USB-IF
[Terminal Types specification, tables 2-6 and 2-7](https://www.usb.org/sites/default/files/termt10.pdf).
USB identifiers, CDC CAT, terminal links, endpoint format, sample rate and
audio processing are unchanged. No custom Windows driver is required.

## Enable the existing firmware's endpoint

1. Open **Sound → Recording** (run `mmsys.cpl`).
2. Right-click an empty area of the device list and select **Show Disabled
   Devices** and **Show Disconnected Devices**. Scroll through the whole list.
3. Find the receiver's recording endpoint, right-click it and select **Enable**.
4. Reopen the receiving application's audio-device list and select that endpoint.

If no receiver endpoint appears even with both display options enabled, obtain
the audio device's **Device Manager → Properties → General** status and driver
name before assuming endpoint hiding is the only problem.

## Install the corrected firmware

Build with `python3 scripts/build_firmware.py --variant rp235xa` for the
identified RP2350A board. The file is
`artifacts/rp235xa/cmx918-audiocat.uf2`. Use the board's BOOTSEL mode and copy
the UF2 to its bootloader drive, then reconnect it normally to Windows.

Check **Sound → Recording** for the receiver/Line In endpoint and select it in
WSJT-X or the recording application. Windows may retain earlier endpoint
preferences, so enable an existing disabled entry using the steps above.
If it still retains the old topology, uninstall **only this receiver's audio
device** in Device Manager and reconnect it to let Windows enumerate it again;
do not remove the generic Windows audio driver package or unrelated devices.
Do not install a WinUSB/Zadig driver over the audio interface.

## Validation and limits

The descriptor regression test uses the actual Embassy composite builder. It
checks the Line Connector terminal category, mono channel cluster, USB output
terminal link, 12 kHz PCM format, asynchronous IN endpoint, CDC interfaces,
sampling-rate requests and audio lifecycle. The new terminal-category check
was run against the old firmware and failed on `0x0710` as expected.

Host tests and the RP2350A target build validate the firmware change; they do
not emulate Windows AudioEndpointBuilder. The reported screenshot and the
documented Windows policy support this diagnosis, but Windows enumeration and
recording must still be confirmed on the owner's PC. No receiver or debug
probe was attached to the development machine during this fix.
