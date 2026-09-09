#!/bin/sh
# Play CMX918 audio through the default PipeWire output. Stop with Ctrl+C.
exec pw-loopback \
  -C 'alsa_input.usb-CMX918_project_CMX918_Audio_CAT_Receiver_C1E27EA41B7ECCC3-00.mono-fallback' \
  -c 1 -m MONO -l 50
