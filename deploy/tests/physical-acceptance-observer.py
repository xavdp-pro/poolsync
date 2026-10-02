#!/usr/bin/env python3
"""Observe real input, ownership and screen changes without recording keys.

Run as root on an Asus/Acer desktop. This does not send input, copy content,
change configuration or restart services. XInput2 event details/text are never
retained; only counts from evdev-backed devices, ignored synthetic counts,
control leases and monitor geometry are recorded. Observation cannot replace
human confirmation of successful typing, paste, or emergency recovery.
"""
import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import pwd
import re
import subprocess
import threading
import time
import tomllib


def digest(path):
    with path.open('rb') as stream: return hashlib.file_digest(stream, 'sha256').hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--user', default='zaza')
    parser.add_argument('--seconds', type=float, default=300)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--expected-sha256', required=True)
    parser.add_argument('--agent-pid', type=int, help='Explicit diagnostic process; identity verification still applies')
    parser.add_argument('--config', type=Path, help='Isolated DEV configuration override')
    args = parser.parse_args()
    assert os.geteuid() == 0 and 0 < args.seconds <= 3600
    account = pwd.getpwnam(args.user); home = Path(account.pw_dir)
    executable = home/'.local/bin/poolsync-agent'
    config = args.config or home/'.config/poolsync/agent.toml'
    cfg = tomllib.loads(config.read_text()); config_before = digest(config)
    pids = []
    for proc in Path('/proc').glob('[0-9]*'):
        try:
            if (proc/'cmdline').read_bytes().split(b'\0')[0] == str(executable).encode(): pids.append(int(proc.name))
        except OSError: pass
    if args.agent_pid: pid = args.agent_pid
    else:
        assert len(pids) == 1, 'expected one installed graphical agent'
        pid = pids[0]
    proc = Path('/proc')/str(pid)
    assert digest(proc/'exe') == args.expected_sha256
    verified_identity = (proc/'cmdline').read_bytes().split(b'\0')[0] == str(executable).encode()
    inherited = dict(item.decode().split('=', 1) for item in (proc/'environ').read_bytes().split(b'\0') if b'=' in item)
    environment = {key: inherited[key] for key in ('DISPLAY', 'XAUTHORITY', 'DBUS_SESSION_BUS_ADDRESS', 'XDG_RUNTIME_DIR') if key in inherited}
    environment.update(PATH='/usr/bin:/bin', HOME=str(home), USER=args.user, LOGNAME=args.user, LC_ALL='C')
    assert environment.get('DISPLAY'), 'graphical session unavailable'
    user = {'user': account.pw_uid, 'group': account.pw_gid, 'extra_groups': os.getgrouplist(args.user, account.pw_gid)}
    counts = Counter(); physical_devices = set(); synthetic_devices = set(); lock = threading.Lock()
    def catalog():
        result = subprocess.run(['/usr/bin/xinput', '--list', '--short'], env=environment, capture_output=True, text=True, timeout=5, **user)
        assert result.returncode == 0, 'XInput2 device inventory unavailable'
        physical, synthetic = set(), set()
        for line in result.stdout.splitlines():
            match = re.search(r'id=(\d+)', line)
            if not match or '[slave' not in line: continue
            device = int(match[1])
            if 'XTEST' in line: synthetic.add(device); continue
            props = subprocess.run(['/usr/bin/xinput', '--list-props', str(device)], env=environment, capture_output=True, text=True, timeout=3, **user)
            if re.search(r'Device Node[^\n]*"/dev/input/event\d+"', props.stdout): physical.add(device)
        with lock:
            physical_devices.clear(); physical_devices.update(physical)
            synthetic_devices.clear(); synthetic_devices.update(synthetic)
    catalog()
    stream = subprocess.Popen(['/usr/bin/stdbuf', '-oL', '/usr/bin/xinput', 'test-xi2', '--root'],
                              env=environment, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                              text=True, bufsize=1, **user)
    def read_events():
        kind = None
        for line in stream.stdout:
            if line.lstrip().startswith('EVENT'):
                match = re.search(r'\((Raw(?:KeyPress|KeyRelease|ButtonPress|ButtonRelease|Motion))\)', line)
                kind = match[1] if match else None
            elif kind:
                match = re.match(r'\s*device:\s*\d+\s*\((\d+)\)', line)
                if not match: continue
                source = int(match[1])
                with lock:
                    if source in physical_devices: counts[kind] += 1
                    elif source in synthetic_devices: counts['synthetic_ignored'] += 1
                    else: counts['non_evdev_ignored'] += 1
                kind = None
    reader = threading.Thread(target=read_events, daemon=True); reader.start()
    started = time.monotonic(); snapshots = []; previous = None; refresh = started+3; dropped = 0
    result = {'node': cfg['node'], 'candidate_sha256': args.expected_sha256, 'pid': pid,
              'production_agent_identity_verified': verified_identity, 'display': environment['DISPLAY'],
              'hardware_acceptance_passed': None, 'human_confirmation_required': True,
              'recorded_content': 'event counts, control owner/focus, peer availability and screen geometry only'}
    try:
        while time.monotonic()-started < args.seconds:
            assert stream.poll() is None, 'XInput2 observation stream stopped'
            now = time.monotonic()
            if now >= refresh: catalog(); refresh = now+3
            try:
                status = json.loads(config.with_suffix('.status.json').read_text())
                snapshot = {'hubless': status.get('hubless'), 'lease': status.get('lease'),
                            'peers': status.get('peers', {}), 'topology': status.get('topology')}
            except (OSError, json.JSONDecodeError): snapshot = {'status_unavailable': True}
            if snapshot != previous:
                with lock: snapshot['physical_event_counts'] = dict(counts)
                snapshot['elapsed_seconds'] = round(now-started, 3)
                if len(snapshots) < 1000: snapshots.append(snapshot)
                else: dropped += 1
                previous = {k: v for k, v in snapshot.items() if k not in ('physical_event_counts', 'elapsed_seconds')}
            time.sleep(.25)
        result['observer_completed'] = True
    except Exception as error:
        result.update(observer_completed=False, error=str(error))
    finally:
        stream.terminate()
        try: stream.wait(timeout=3)
        except subprocess.TimeoutExpired: stream.kill(); stream.wait(timeout=3)
        reader.join(timeout=2); stream.stdout.close()
        with lock:
            result.update(event_counts=dict(counts), evdev_device_count=len(physical_devices), synthetic_device_count=len(synthetic_devices))
        try: executing_preserved = digest(proc/'exe') == args.expected_sha256
        except OSError: executing_preserved = False
        result.update(snapshots=snapshots, dropped_status_changes=dropped, duration_seconds=round(time.monotonic()-started, 3),
                      configuration_preserved=digest(config) == config_before,
                      executing_agent_preserved=executing_preserved)
        args.output.write_text(json.dumps(result, indent=2)+'\n'); args.output.chmod(0o600)
    print(json.dumps({key: result[key] for key in ('node', 'observer_completed', 'event_counts', 'evdev_device_count', 'hardware_acceptance_passed', 'configuration_preserved', 'executing_agent_preserved')}))
    return 0 if result.get('observer_completed') and result['configuration_preserved'] and result['executing_agent_preserved'] else 1


if __name__ == '__main__': raise SystemExit(main())
