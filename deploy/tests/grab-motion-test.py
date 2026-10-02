#!/usr/bin/env python3
"""Run the native grab-motion fixture on a private Xorg session in neko-desk-a."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import pwd
import signal
import subprocess
import time
import uuid


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--harness', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    account = pwd.getpwnam('zaza'); config = Path(account.pw_dir)/'.config/poolsync/agent.toml'
    before = hashlib.sha256(config.read_bytes()).hexdigest()
    original = subprocess.check_output(['pgrep', '-u', 'zaza', '-x', 'poolsync-agent'], text=True).split()
    assert len(original) == 1 and not Path('/tmp/.X11-unix/X114').exists()
    spec = importlib.util.spec_from_file_location('fixture', Path(__file__).with_name('no-hub-desktop-test.py'))
    fixture = importlib.util.module_from_spec(spec); spec.loader.exec_module(fixture)
    root = Path('/tmp/poolsync-grab-motion-'+uuid.uuid4().hex); root.mkdir(mode=0o700)
    os.chown(root, account.pw_uid, account.pw_gid)
    (root/'xorg.conf').write_text(fixture.XORG_CONFIG)
    (root/'authority').touch()
    environment = {'PATH': '/usr/bin:/bin', 'HOME': str(root), 'USER': 'zaza', 'DISPLAY': ':114', 'XAUTHORITY': str(root/'authority')}
    result = {'harness_sha256': hashlib.sha256(args.harness.read_bytes()).hexdigest(), 'isolated_display': 114, 'physical_input': False}
    with (root/'xorg-console.log').open('ab') as log:
        server = subprocess.Popen(['Xorg', ':114', '-config', str(root/'xorg.conf'), '-ac', '-noreset', '-nolisten', 'tcp', '-logfile', str(root/'xorg.log')], stdout=log, stderr=log, start_new_session=True)
    try:
        deadline = time.monotonic()+5
        while not Path('/tmp/.X11-unix/X114').exists():
            assert time.monotonic() < deadline; time.sleep(.05)
        native = subprocess.run([str(args.harness)], env=environment, user=account.pw_uid, group=account.pw_gid,
                                capture_output=True, text=True, timeout=15)
        (root/'harness-stderr.log').write_text(native.stderr)
        result['checks'] = [json.loads(line) for line in native.stdout.splitlines()]
        result['exit_code'] = native.returncode
        result['passed'] = native.returncode == 0 and all(check['passed'] for check in result['checks'])
    except Exception as error: result.update(passed=False, error=str(error))
    finally:
        if server.poll() is None: os.killpg(server.pid, signal.SIGTERM); server.wait(timeout=5)
        result['original_configuration_and_agent_preserved'] = hashlib.sha256(config.read_bytes()).hexdigest() == before and subprocess.check_output(['pgrep', '-u', 'zaza', '-x', 'poolsync-agent'], text=True).split() == original
        result['private_diagnostics'] = str(root)
        args.output.write_text(json.dumps(result, indent=2)+'\n'); args.output.chmod(0o600)
    print(json.dumps(result))
    return 0 if result.get('passed') and result['original_configuration_and_agent_preserved'] else 1


if __name__ == '__main__': raise SystemExit(main())
