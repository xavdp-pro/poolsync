#!/usr/bin/env python3
"""Back up and atomically install an explicitly qualified local hubless bundle.

Run as root on the selected computer. No keys, certificates, autostart entries
or graphical settings are replaced. Rollback restores the complete saved
configuration and exactly the files this tool changed.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import pwd
import shutil
import subprocess
import time
import tomllib


FILES = {
    'poolsync-agent-launch.sh': '.local/bin/poolsync-agent-launch.sh',
    'poolsync-watchdog.sh': '.local/bin/poolsync-watchdog.sh',
    'poolsync-ctl.sh': '.local/bin/poolsync-ctl',
    'systemd/poolsync-watchdog.service': '.config/systemd/user/poolsync-watchdog.service',
}


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def atomic(source, target, mode, uid, gid):
    target.parent.mkdir(parents=True, exist_ok=True)
    temp = target.with_name(target.name+'.hubless-new')
    shutil.copyfile(source, temp)
    os.chmod(temp, mode)
    os.chown(temp, uid, gid)
    with temp.open('rb') as stream:os.fsync(stream.fileno())
    os.replace(temp, target)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--user', default='zaza')
    parser.add_argument('--bundle', type=Path)
    parser.add_argument('--expected-original-config-sha256')
    parser.add_argument('--expected-candidate-sha256')
    parser.add_argument('--backup-id')
    parser.add_argument('--check-only', action='store_true')
    parser.add_argument('--rollback', type=Path)
    args = parser.parse_args()
    assert os.geteuid() == 0, 'requires root to preserve ownership and saved services'
    account = pwd.getpwnam(args.user)
    home = Path(account.pw_dir)
    runtime = f'/run/user/{account.pw_uid}'
    def service(*arguments, check=False):
        return subprocess.run(['runuser','-u',args.user,'--','env','XDG_RUNTIME_DIR='+runtime,'DBUS_SESSION_BUS_ADDRESS=unix:path='+runtime+'/bus','systemctl','--user',*arguments], capture_output=True, text=True, timeout=25, check=check)
    def restore(backup):
        manifest = json.loads((backup/'manifest.json').read_text())
        assert manifest['user'] == args.user and manifest['home'] == str(home)
        service('stop','poolsync-watchdog.timer','poolsync-watchdog.service','poolsync-agent.service')
        config = home/'.config/poolsync'
        shutil.rmtree(config)
        shutil.copytree(backup/'config', config, copy_function=shutil.copy2)
        for path in config.rglob('*'):os.chown(path, account.pw_uid, account.pw_gid)
        os.chown(config, account.pw_uid, account.pw_gid)
        for relative, existed in manifest['changed_files'].items():
            target = home/relative
            if existed:
                source = backup/'files'/relative
                atomic(source, target, source.stat().st_mode & 0o777, account.pw_uid, account.pw_gid)
            else:target.unlink(missing_ok=True)
        service('daemon-reload', check=True)
        for unit, status in manifest['units'].items():
            if status['active']:service('start', unit, check=True)
        assert digest(home/'.config/poolsync/agent.toml') == manifest['original_config_sha256']
        print(json.dumps({'rollback_restored': True, 'node': manifest['node'], 'backup': str(backup)}))
    if args.rollback:
        restore(args.rollback)
        return
    assert all([args.bundle, args.expected_original_config_sha256, args.expected_candidate_sha256, args.backup_id]), 'bundle, fingerprints and backup identifier are required'
    assert all(c.isalnum() or c in '-_.' for c in args.backup_id), 'invalid backup identifier'
    current = home/'.config/poolsync/agent.toml'
    assert digest(current) == args.expected_original_config_sha256, 'live configuration changed after inventory'
    cfg = tomllib.loads(current.read_text())
    candidate = args.bundle/'agent.toml'
    new = tomllib.loads(candidate.read_text())
    assert new['hubless'] and new['peer_direct_clipboard'] and not new['hub_clipboard']
    for field in ('node','node_token','token','e2e_key','mode','peer_tls_cert','peer_tls_key','screen','kvm_enabled','kvm_capture'):
        assert cfg.get(field) == new.get(field), 'identity, permissions or saved geometry changed'
    for key, value in cfg.items():
        if key not in ('hubless','neighbors','peer_tokens'):assert new.get(key) == value, 'unrelated setting changed'
    assert all(n in new['neighbors'] for n in cfg['neighbors']), 'existing route changed'
    assert all(new['peer_tokens'].get(k) == v for k,v in cfg['peer_tokens'].items()), 'existing peer authorization changed'
    executable = args.bundle/'poolsync-agent'
    assert digest(executable) == args.expected_candidate_sha256, 'candidate binary fingerprint differs'
    version = subprocess.check_output([str(executable),'--version'], text=True, timeout=5).strip()
    ldd = subprocess.run(['ldd',str(executable)], capture_output=True, text=True, timeout=5)
    assert ldd.returncode == 0 and 'not found' not in ldd.stdout, 'candidate runtime dependency unavailable'
    layout = json.loads((args.bundle/'agent.topology.json').read_text())
    assert cfg['node'] in layout['topology']['nodes'] and layout['origin'] in layout['topology']['nodes']
    assert not current.with_suffix('.topology.json').exists(), 'an existing hubless layout requires a separate migration'
    backup = home/'.local/state/poolsync/deployment-backups'/args.backup_id
    assert not backup.exists(), 'backup identifier already used'
    print(json.dumps({'node': cfg['node'], 'candidate_version': version, 'candidate_sha256':args.expected_candidate_sha256, 'preservation_checks_passed':True, 'installed':False}))
    if args.check_only:return
    units = {unit:{'active':service('is-active',unit).returncode == 0,'enabled':service('is-enabled',unit).stdout.strip()} for unit in ('poolsync-agent.service','poolsync-watchdog.timer')}
    assert units['poolsync-agent.service']['active'], 'qualify a live graphical agent before upgrade'
    backup.mkdir(mode=0o700, parents=True)
    shutil.copytree(home/'.config/poolsync', backup/'config', copy_function=shutil.copy2)
    changed = {'.local/bin/poolsync-agent', *FILES.values()}
    manifest = {'user':args.user,'home':str(home),'node':cfg['node'],'original_config_sha256':digest(current),'original_executable_sha256':digest(home/'.local/bin/poolsync-agent'),'units':units,'changed_files':{relative:(home/relative).exists() for relative in changed},'created_unix':time.time()}
    for relative, existed in manifest['changed_files'].items():
        if existed:
            target = backup/'files'/relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(home/relative,target)
    (backup/'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
    (backup/'manifest.json').chmod(0o600)
    # Keep the archive readable only by its owner; copied private files retain
    # their original permissions inside the mode-0700 backup boundary.
    for directory, _, files in os.walk(backup):
        os.chown(directory, account.pw_uid, account.pw_gid)
        for name in files:os.chown(Path(directory)/name, account.pw_uid, account.pw_gid)
    try:
        service('stop','poolsync-watchdog.timer','poolsync-watchdog.service','poolsync-agent.service', check=True)
        atomic(executable,home/'.local/bin/poolsync-agent',0o755,account.pw_uid,account.pw_gid)
        for source, relative in FILES.items():
            atomic(args.bundle/source,home/relative,0o644 if source.endswith('.service') else 0o755,account.pw_uid,account.pw_gid)
        atomic(candidate,current,0o600,account.pw_uid,account.pw_gid)
        atomic(args.bundle/'agent.topology.json',current.with_suffix('.topology.json'),0o600,account.pw_uid,account.pw_gid)
        service('daemon-reload', check=True)
        service('start','poolsync-agent.service', check=True)
        if units['poolsync-watchdog.timer']['active']:service('start','poolsync-watchdog.timer', check=True)
        assert service('is-active','poolsync-agent.service').returncode == 0
        print(json.dumps({'installed':True,'node':cfg['node'],'backup':str(backup),'running_verification_pending':True}))
    except BaseException:
        restore(backup)
        raise


if __name__ == '__main__':main()
