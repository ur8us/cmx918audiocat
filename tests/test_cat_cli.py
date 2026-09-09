"""Offline CLI tests: no USB/serial devices opened."""
import contextlib
import importlib.util
import io
from pathlib import Path
import sys
import types
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('cat_cli', Path(__file__).resolve().parents[1] / 'scripts/cat.py')
cli = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cli)

class Port:
    def __init__(self, replies):
        self.replies = list(replies)
        self.writes = []
    def __enter__(self): return self
    def __exit__(self, *args): pass
    def reset_input_buffer(self): pass
    def write(self, data): self.writes.append(data); return len(data)
    def read_until(self, *args, **kwargs): return self.replies.pop(0)

class CatCliTests(unittest.TestCase):
    def test_mhz_lowercase_mode_and_exact_cat_frames(self):
        port = Port([b'FA00014074000;', b'MD1;', b'ZZST1,00,0000000000,0000000000,0000000000,0000000000;'])
        receiver = types.SimpleNamespace(vid=0xc0de, pid=0x0919, device='receiver-port', serial_number='abc')
        unrelated = types.SimpleNamespace(vid=0x2e8a, pid=0xc, device='probe-port', serial_number='def')
        serial = types.ModuleType('serial'); serial.SerialException = OSError
        def open_port(name, *args, **kwargs):
            self.assertEqual(name, 'receiver-port')
            return port
        serial.Serial = open_port
        tools = types.ModuleType('serial.tools')
        tools.list_ports = types.SimpleNamespace(comports=lambda: [unrelated, receiver])
        with patch.dict(sys.modules, {'serial': serial, 'serial.tools': tools}), patch.object(sys, 'argv', ['cat.py', '--mhz', '14.074', '--mode', 'lsb']), contextlib.redirect_stdout(io.StringIO()):
            cli.main()
        self.assertEqual(port.writes, [b'FA00014074000;FA;', b'MD1;MD;', b'ZZST;'])

    def test_invalid_arguments_fail_before_serial_import(self):
        for args in [['--mhz','NaN'], ['--mhz','Infinity'], ['--mhz','14.0740001'], ['--mhz','0.1'], ['--mhz','109'], ['--mhz','x'], ['--fq','14074000','--mhz','14.074'], ['--mode','FM'], ['--frequency','149999']]:
            with self.subTest(args=args), patch.object(sys, 'argv', ['cat.py', *args]), contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as exc:
                cli.main()
            self.assertEqual(exc.exception.code, 2)

if __name__ == '__main__': unittest.main()
