#!/usr/bin/env python3
"""Exercise direct-mode script behavior without touching a real user service."""
import os
from pathlib import Path
import subprocess
import tempfile


def main():
    with tempfile.TemporaryDirectory(prefix='poolsync-hubless-scripts-') as directory:
        root = Path(directory)
        home, bin_dir = root / 'home', root / 'bin'
        (home / '.config/poolsync').mkdir(parents=True)
        bin_dir.mkdir()
        (home / '.config/poolsync/agent.toml').write_text(
            'node="isolated"\nhubless = true # direct peers\n'
            'hub_url="ws://127.0.0.1:1/ws"\nnode_token="synthetic-token"\n')
        log = root / 'commands.log'
        for name in ('curl', 'timeout', 'systemctl'):
            path = bin_dir / name
            path.write_text('#!/bin/sh\nprintf "%s\\n" "' + name +
                            ' $*" >> "$POOLSYNC_TEST_COMMAND_LOG"\nexit 0\n')
            path.chmod(0o755)
        for name in ('logger', 'ip', 'pgrep'):
            path = bin_dir / name
            path.write_text('#!/bin/sh\nexit 1\n')
            path.chmod(0o755)
        env = {**os.environ, 'HOME': str(home), 'PATH': str(bin_dir) + ':' + os.environ['PATH'],
               'POOLSYNC_TEST_COMMAND_LOG': str(log)}
        env.pop('DISPLAY', None)
        repo = Path(__file__).resolve().parents[2]
        def run(script, *args):
            subprocess.run(['bash', str(repo / 'deploy' / script), *args],
                           env=env, check=True, capture_output=True)
        for _ in range(4):
            run('poolsync-watchdog.sh')
        assert not log.exists(), 'hubless watchdog probes the hub or restarts a healthy agent'
        cache = home / '.cache/poolsync/clipboard'
        cache.mkdir(parents=True)
        (cache / 'synthetic').touch()
        run('poolsync-ctl.sh', 'clear-history')
        assert not cache.exists() and not log.exists(), 'history clearing depends on the hub'
        localbin = home / '.local/bin'
        localbin.mkdir(parents=True)
        pick = localbin / 'poolsync-pick-session.sh'
        pick.write_text('#!/bin/sh\nprintf ":110 42\\n"\n')
        pick.chmod(0o755)
        env['POOLSYNC_RDP_MISS_THRESHOLD'] = '1'
        run('poolsync-watchdog.sh')
        assert log.read_text().strip() == 'systemctl --user restart poolsync-agent.service'
        print('PASS: no hub probes/restarts, local history clear, graphical-session attachment retained')


if __name__ == '__main__':
    main()
