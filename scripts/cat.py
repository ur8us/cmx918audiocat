#!/usr/bin/env python3
"""Control a CMX918 USB Audio CAT receiver (requires pyserial)."""
import argparse
from decimal import Decimal, DecimalException


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--list', action='store_true', help='list matching receivers, without opening ports')
    parser.add_argument('--port', help='explicit serial device path or COM port')
    parser.add_argument('--serial', help='USB receiver serial number')
    frequency = parser.add_mutually_exclusive_group()
    frequency.add_argument('--frequency', '--fq', type=int, help='dial frequency in Hz, 70000..130000000 (experimental)')
    frequency.add_argument('--mhz', help='dial frequency in MHz, e.g. 14.074')
    parser.add_argument('--mode', type=str.upper, choices=['USB', 'LSB'])
    parser.add_argument('--retry', action='store_true', help='retry a faulted receiver configuration')
    args = parser.parse_args()
    if args.mhz is not None:
        try:
            hz = Decimal(args.mhz) * 1_000_000
            if not hz.is_finite() or hz != hz.to_integral_value():
                raise ValueError('frequency must resolve to whole Hz')
            if not 70000 <= hz <= 130000000:
                raise ValueError('frequency must be between 70 kHz and 130 MHz')
            args.frequency = int(hz)
        except (DecimalException, ValueError) as exc:
            parser.error(str(exc))
    if args.frequency is not None and not 70000 <= args.frequency <= 130000000:
        parser.error('frequency must be between 70000 and 130000000 Hz')
    if args.port and args.serial:
        parser.error('choose --port or --serial, not both')
    try:
        import serial
        from serial.tools import list_ports
    except ImportError:
        parser.error('install pyserial in your Python environment')
    devices = [p for p in list_ports.comports() if p.vid == 0xc0de and p.pid == 0x0919]
    if args.list:
        for p in devices:
            print(f'{p.device}\t{p.serial_number}\t{p.description}')
        return
    if not args.port:
        matches = [p for p in devices if not args.serial or p.serial_number == args.serial]
        if len(matches) != 1:
            parser.error(f'found {len(matches)} matching receivers; use --list and select --serial or --port')
        args.port = matches[0].device
    try:
        with serial.Serial(args.port, 115200, timeout=8, write_timeout=2) as port:
            port.reset_input_buffer()

            def query(command, setter=''):
                payload = (setter + command + ';').encode('ascii')
                if port.write(payload) != len(payload):
                    raise RuntimeError('incomplete CAT command write')
                reply = port.read_until(b';', size=64)
                if reply == b'?;':
                    raise RuntimeError('receiver rejected command/configuration; query ZZST for fault details')
                if not reply.endswith(b';') or not reply.startswith(command.encode('ascii')):
                    raise RuntimeError(f'incomplete or unexpected reply: {reply!r}')
                return reply.decode('ascii')

            if args.retry:
                print(query('ZZST', 'ZZRX;'))
            setter = '' if args.frequency is None else f'FA{args.frequency:011d};'
            reply = query('FA', setter)
            if args.frequency is not None and reply != f'FA{args.frequency:011d};':
                raise RuntimeError(f'frequency was not applied: {reply}')
            print(reply)
            setter = '' if args.mode is None else f'MD{1 if args.mode == "LSB" else 2};'
            reply = query('MD', setter)
            if setter and reply != setter:
                raise RuntimeError(f'mode was not applied: {reply}')
            print(reply)
            print(query('ZZST'))
    except (serial.SerialException, RuntimeError, UnicodeError) as exc:
        raise SystemExit(str(exc)) from exc


if __name__ == '__main__':
    main()
