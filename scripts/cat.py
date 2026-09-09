#!/usr/bin/env python3
"""Control a CMX918 USB Audio CAT receiver (requires pyserial)."""
import argparse


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--list', action='store_true', help='list matching receivers, without opening ports')
    parser.add_argument('--port', help='explicit serial device path or COM port')
    parser.add_argument('--serial', help='USB receiver serial number')
    parser.add_argument('--frequency', type=int, help='dial frequency in Hz, 150000..108000000')
    parser.add_argument('--mode', choices=['USB', 'LSB'])
    parser.add_argument('--retry', action='store_true', help='retry a faulted receiver configuration')
    args = parser.parse_args()
    if args.frequency is not None and not 150000 <= args.frequency <= 108000000:
        parser.error('frequency must be between 150000 and 108000000 Hz')
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
                port.write((setter + command + ';').encode('ascii'))
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
