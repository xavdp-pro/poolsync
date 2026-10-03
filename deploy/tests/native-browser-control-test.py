#!/usr/bin/env python3
"""Compare native GTK/browser clipboard retrieval without starting PoolSync.

Use a separate private display, HOME, DBus session and loopback receiver in a
dedicated test container. Existing agents/displays remain running. Report only
synthetic fixture hashes, event metadata and original-state preservation.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import time
import uuid


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--container', default='neko-desk-b')
    parser.add_argument('--engine', choices=('firefox', 'chromium'), default='firefox')
    parser.add_argument('--rounds', type=int, default=3)
    parser.add_argument('--pastes-per-round', type=int, default=12)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    assert os.geteuid() == 0
    assert args.container in ('neko-desk-a', 'neko-desk-b', 'neko-desk-c')
    assert 1 <= args.rounds <= 10 and 1 <= args.pastes_per_round <= 50
    spec = importlib.util.spec_from_file_location('desktop_lab', Path(__file__).with_name('no-hub-desktop-test.py'))
    lab = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(lab)
    # These constants differ from the simultaneous three-peer lab. The native
    # helper is reused without starting or communicating with a candidate.
    helper = lab.HELPER
    helper = helper.replace("else:raise RuntimeError('unknown operation')", """elif op=='owned_worker_pids':
    output=[]
    for file in root.glob('*.pid.*'):
        try:
            pid=int(file.read_text());raw=Path('/proc/'+str(pid)+'/environ').read_bytes()
            variables=dict(part.decode().split('=',1) for part in raw.split(b'\\0') if b'=' in part)
            if variables.get('XDG_RUNTIME_DIR')==str(root/'runtime'):output.append(pid)
        except (OSError,ValueError,UnicodeError):pass
else:raise RuntimeError('unknown operation')""")
    root = '/tmp/poolsync-no-hub-native-control-' + uuid.uuid4().hex[:12]
    args.output.parent.mkdir(parents=True, exist_ok=True)
    artifacts = args.output.parent / ('private-native-control-' + root.rsplit('-', 1)[1])
    artifacts.mkdir(mode=0o700)
    result = {'poolsync_process_started': False, 'display': ':112', 'engine': args.engine,
              'rounds_requested': args.rounds, 'pastes_per_round': args.pastes_per_round,
              'native_pastes': [], 'native_text_pastes': 0,
              'original_desktop_mutation_requested': False}
    result['source_sha256'] = {name: hashlib.sha256(Path(__file__).with_name(name).read_bytes()).hexdigest()
        for name in ('native-browser-control-test.py', 'no-hub-desktop-test.py',
                     'browser-paste-server.py', 'clipboard-paste.html')}
    workers = []
    copier = None
    original = None

    def op(operation, **extra):
        process = subprocess.run(['podman', 'exec', '-i', args.container, 'python3', '-c', helper],
                                 input=json.dumps({'root': root, 'op': operation, **extra}),
                                 capture_output=True, text=True, timeout=20)
        if process.returncode:
            diagnostic = artifacts / (operation + '-error.log')
            diagnostic.write_text(process.stderr)
            diagnostic.chmod(0o600)
            raise AssertionError(operation + ' failed; private diagnostics retained')
        return json.loads(process.stdout)

    try:
        version = subprocess.run(['podman', 'exec', args.container, args.engine, '--version'],
                                 capture_output=True, text=True, timeout=10)
        assert version.returncode == 0, 'Selected browser is unavailable'
        result['browser_version'] = version.stdout.strip()[:256]
        available = subprocess.run(['podman', 'exec', args.container, 'python3', '-c',
            "import socket;s=socket.socket();s.bind(('127.0.0.1',19581));s.close()"], capture_output=True, timeout=5)
        assert available.returncode == 0, 'Private receiver port is already occupied'
        original = op('snapshot')
        fixtures = {name: Path(__file__).with_name(name).read_text()
                    for name in ('browser-paste-server.py', 'clipboard-paste.html')}
        images = op('prepare', original=original, config='', slow_fixture='', lossless_fixture='',
                    display=112, xorg_config=lab.XORG_CONFIG, large_images=True,
                    native_owner='gtk', browser_fixtures=fixtures, browser_engine=args.engine,
                    browser_port=19581)
        workers.append(op('xserver'))
        for attempt in range(60):
            try:
                op('screen')
                break
            except AssertionError:
                if attempt == 59:
                    raise
                time.sleep(.1)
        workers.append(op('dbus'))
        workers.append(op('receiver'))
        workers.extend(op('browser').values())
        workers.append(op('key_window'))
        for index in range(args.rounds):
            image = 'large-first' if index % 2 == 0 else 'large-second'
            expected = images[image]['sha256']
            copier = op('copy', image=image)['copier_pid']
            for paste_round in range(args.pastes_per_round):
                op('key_window_focus')
                since = time.time()
                op('browser_paste')
                deadline = time.monotonic() + 10
                while time.monotonic() < deadline:
                    if any(record['at'] >= since and record.get('sha256') == expected
                           for record in op('browser_records')):
                        result['native_pastes'].append({'round': index, 'paste': paste_round,
                                                       'pixel_sha256': expected})
                        break
                    time.sleep(.1)
                else:
                    result['failed_paste'] = {'round': index, 'paste': paste_round,
                                             'diagnostics': op('browser_diagnostics')}
                    raise AssertionError('Native GTK-to-browser image retrieval failed without PoolSync')
            text = 'Native control text ' + uuid.uuid4().hex
            copier = op('copy', text=text)['copier_pid']
            since = time.time()
            op('browser_paste')
            expected = hashlib.sha256(text.encode()).hexdigest()
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline:
                if any(record['at'] >= since and record.get('sha256') == expected
                       for record in op('browser_records')):
                    result['native_text_pastes'] += 1
                    break
                time.sleep(.1)
            else:
                raise AssertionError('Native GTK-to-browser text retrieval failed without PoolSync')
            print(json.dumps({'completed_round': index + 1,
                              'native_image_pastes': len(result['native_pastes'])}), flush=True)
        result['functional_checks_passed'] = True
    except Exception as error:
        result.update(functional_checks_passed=False, error=str(error))
    finally:
        cleanup_errors = []
        for pid in ([copier] if copier else []) + list(reversed(workers)):
            try:
                op('stop', pid=pid)
            except Exception as error:
                cleanup_errors.append(str(error))
        # A browser-start assertion can happen after the helper spawned its
        # HTTP server. Include only workers whose private runtime identity
        # still matches, so startup failures cannot leak a receiver service.
        try:
            for pid in op('owned_worker_pids'):
                op('stop', pid=pid)
            assert not op('owned_worker_pids'), 'private workers remain running'
        except Exception as error:
            cleanup_errors.append(str(error))
        if original:
            try:
                after = op('snapshot')
                result['original_state_preserved'] = all(original[key] == after[key]
                    for key in ('pid', 'running_sha256', 'config_sha256', 'away'))
            except Exception as error:
                cleanup_errors.append(str(error))
        result['cleanup_errors'] = cleanup_errors
        result['cleanup_passed'] = not cleanup_errors and result.get('original_state_preserved', False)
        args.output.write_text(json.dumps(result, indent=2) + '\n')
        args.output.chmod(0o600)
    print(json.dumps({key: result.get(key) for key in
                     ('engine', 'functional_checks_passed', 'error', 'cleanup_passed')}), flush=True)
    return 0 if result['functional_checks_passed'] and result['cleanup_passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
