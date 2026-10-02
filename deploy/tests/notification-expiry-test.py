#!/usr/bin/env python3
"""Qualify real XFCE notification expiry on a private container X11 session.

Run inside a dedicated neko desktop as root. The harness is built from
poolsync-agent/examples/notification-qualification.rs. Existing agents, their
configuration, and their graphical/DBus sessions are preserved.
"""
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
    parser.add_argument('--display', type=int, default=114)
    args = parser.parse_args()
    module_spec = importlib.util.spec_from_file_location('native_fixture', Path(__file__).with_name('no-hub-desktop-test.py'))
    fixture = importlib.util.module_from_spec(module_spec)
    module_spec.loader.exec_module(fixture)
    account = pwd.getpwnam('zaza')
    config = Path(account.pw_dir)/'.config/poolsync/agent.toml'
    baseline = hashlib.sha256(config.read_bytes()).hexdigest()
    original_pids = subprocess.check_output(['pgrep', '-u', 'zaza', '-x', 'poolsync-agent'], text=True).split()
    assert len(original_pids) == 1
    assert not Path('/tmp/.X11-unix/X'+str(args.display)).exists()
    root = Path('/tmp/poolsync-notification-'+uuid.uuid4().hex)
    root.mkdir(mode=0o700)
    home = root/'home'; home.mkdir(mode=0o700)
    runtime = root/'runtime'; runtime.mkdir(mode=0o700)
    (root/'xorg.conf').write_text(fixture.XORG_CONFIG)
    authority = root/'authority'; authority.touch(mode=0o600)
    for path in (root, home, runtime, authority): os.chown(path, account.pw_uid, account.pw_gid)
    environment = {'PATH': '/usr/local/bin:/usr/bin:/bin', 'HOME': str(home), 'USER': 'zaza', 'LOGNAME': 'zaza',
                   'DISPLAY': ':'+str(args.display), 'XAUTHORITY': str(authority), 'XDG_RUNTIME_DIR': str(runtime),
                   'DBUS_SESSION_BUS_ADDRESS': 'unix:path='+str(runtime/'bus'), 'GTK_USE_PORTAL': '0',
                   'XDG_CONFIG_HOME': str(home/'.config'), 'XDG_CACHE_HOME': str(home/'.cache')}
    children = []
    checks = []
    def launch(command, log, user=True):
        with (root/log).open('ab') as stream:
            child = subprocess.Popen(command, env=environment, stdout=stream, stderr=stream,
                                     start_new_session=True,
                                     **({'user': account.pw_uid, 'group': account.pw_gid, 'extra_groups': []} if user else {}))
        children.append(child)
        return child
    def run(command):
        return subprocess.run(command, env=environment, user=account.pw_uid, group=account.pw_gid,
                              extra_groups=[], capture_output=True, text=True, timeout=5)
    def stop(child):
        if child.poll() is None:
            os.killpg(child.pid, signal.SIGTERM)
            try: child.wait(timeout=3)
            except subprocess.TimeoutExpired:
                os.killpg(child.pid, signal.SIGKILL); child.wait(timeout=3)
    def wait_for(condition, timeout=5):
        deadline = time.monotonic()+timeout
        while time.monotonic() < deadline:
            if condition(): return
            time.sleep(.05)
        raise AssertionError('Timed out waiting for isolated notification fixture')
    def count():
        response = run(['xdotool', 'search', '--onlyvisible', '--class', 'Xfce4-notifyd'])
        return len(response.stdout.split())
    def check(name, condition):
        checks.append({'name': name, 'passed': bool(condition)})
        assert condition, name
    daemon_path = next(p for p in ('/usr/lib/x86_64-linux-gnu/xfce4/notifyd/xfce4-notifyd', '/usr/lib/xfce4/notifyd/xfce4-notifyd') if Path(p).is_file())
    def daemon():
        child = launch([daemon_path], 'daemon.log')
        wait_for(lambda: run(['busctl', '--user', 'call', 'org.freedesktop.Notifications', '/org/freedesktop/Notifications', 'org.freedesktop.Notifications', 'GetServerInformation']).returncode == 0)
        return child
    def unrelated():
        response = run(['notify-send', '-p', '-a', 'notification-qualification-unrelated', '-u', 'critical', 'Unrelated DEV notification', 'Must survive PoolSync status changes'])
        assert response.returncode == 0
        return int(response.stdout.strip())
    result = {'harness_sha256': hashlib.sha256(args.harness.read_bytes()).hexdigest(), 'isolated_display': args.display, 'checks': checks}
    try:
        launch(['Xorg', environment['DISPLAY'], '-config', str(root/'xorg.conf'), '-noreset', '-nolisten', 'tcp', '-ac', '-logfile', str(root/'xorg.log')], 'xserver.log', user=False)
        wait_for(lambda: Path('/tmp/.X11-unix/X'+str(args.display)).exists())
        launch(['dbus-daemon', '--session', '--nofork', '--address='+environment['DBUS_SESSION_BUS_ADDRESS']], 'dbus.log')
        wait_for(lambda: (runtime/'bus').exists())
        notifyd = daemon(); unrelated()
        burst = launch([str(args.harness), 'denial-burst'], 'burst.log')
        wait_for(lambda: 'BURST_FINISHED' in (root/'burst.log').read_text(), timeout=20)
        check('Twenty denied control requests produce one PoolSync toast alongside an unrelated toast', count() == 2)
        time.sleep(6)
        check('Denied control toast expires within six seconds; unrelated critical toast survives', count() == 1)
        stop(burst)
        suspend = run([str(args.harness), 'suspend']); assert suspend.returncode == 0
        check('Suspension status appears without replacing the unrelated toast', count() == 2)
        time.sleep(11)
        check('Suspension status expires within eleven seconds', count() == 1)
        stop(notifyd); notifyd = daemon()
        restart = launch([str(args.harness), 'restart'], 'restart.log')
        wait_for(lambda: 'RESTART_READY' in (root/'restart.log').read_text())
        stop(notifyd); notifyd = daemon()
        reused_id = unrelated()
        check('Fresh daemon assigns the old PoolSync ID to an unrelated notification', reused_id == 1)
        wait_for(lambda: 'RESTART_FINISHED' in (root/'restart.log').read_text(), timeout=8)
        check('PoolSync does not replace another application after notification daemon restart', count() == 2)
        time.sleep(6)
        check('Post-restart PoolSync toast expires while unrelated critical toast remains', count() == 1)
        result['passed'] = True
    except Exception as error:
        result.update(passed=False, error=str(error))
    finally:
        for child in reversed(children): stop(child)
        result['original_agent_and_config_preserved'] = (hashlib.sha256(config.read_bytes()).hexdigest() == baseline and subprocess.check_output(['pgrep', '-u', 'zaza', '-x', 'poolsync-agent'], text=True).split() == original_pids)
        result['private_diagnostics'] = str(root)
        args.output.write_text(json.dumps(result, indent=2)+'\n'); args.output.chmod(0o600)
    print(json.dumps(result))
    return 0 if result.get('passed') and result['original_agent_and_config_preserved'] else 1


if __name__ == '__main__': raise SystemExit(main())
