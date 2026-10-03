#!/usr/bin/env python3
"""Replace a qualified fleet while all participants remain temporarily absent.

Dry run is the default. Apply requires exact executable and installer hashes.
Use the existing per-host installer/backup. Resume only a verified uniform
cohort; on failure roll back changed agents while the whole pool stays absent.
SSH aliases must connect as root. The local host uses passwordless sudo.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import time


REMOTE = r'''
import hashlib,json,os,pathlib,pwd,subprocess,sys,tomllib
request=json.load(sys.stdin);assert os.geteuid()==0
account=pwd.getpwnam(request['user']);account_home=pathlib.Path(account.pw_dir)
installed=account_home/'.local/bin/poolsync-agent';config=account_home/'.config/poolsync/agent.toml'
marker=config.with_suffix('.away');backup=account_home/'.local/state/poolsync/deployment-backups'/request['backup_id']
def digest(path):
 with pathlib.Path(path).open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
def context():
 matches=[]
 for entry in pathlib.Path('/proc').glob('[0-9]*'):
  try:
   if (entry/'cmdline').read_bytes().split(b'\0')[0]!=str(installed).encode():continue
   variables=dict(part.decode().split('=',1) for part in (entry/'environ').read_bytes().split(b'\0') if b'=' in part)
   assert entry.stat().st_uid==account.pw_uid
   matches.append((entry,variables))
  except (OSError,UnicodeError):continue
 assert len(matches)==1,'Expected one graphical agent'
 entry,variables=matches[0];assert variables.get('DISPLAY'),'Graphical session unavailable'
 assert digest(entry/'exe')==digest(installed),'Installed and executing binaries differ'
 return entry,variables
def user_command(arguments):
 _,variables=context()
 bindings=[name+'='+variables[name] for name in ('DISPLAY','XAUTHORITY','XDG_RUNTIME_DIR','DBUS_SESSION_BUS_ADDRESS') if name in variables]
 return subprocess.check_output(['/usr/sbin/runuser','-u',request['user'],'--','/usr/bin/env',*bindings,*arguments],text=True,timeout=15)
def quiescence():
 counts={}
 for device in ('Virtual core XTEST keyboard','Virtual core XTEST pointer'):
  counts[device]=sum(line.strip().endswith('=down') for line in user_command(['xinput','query-state',device]).splitlines())
 assert not any(counts.values()),'Injected input remains held'
 return counts
def state():
 cfg=tomllib.loads(config.read_text());assert cfg['hubless']
 entry,variables=context();status=json.loads(config.with_suffix('.status.json').read_text())
 return {'node':cfg['node'],'mode':cfg['mode'],'pid':int(entry.name),'uid':account.pw_uid,
   'display':variables['DISPLAY'],'running_sha256':digest(entry/'exe'),
   'configuration_sha256':digest(config),'layout_sha256':digest(config.with_suffix('.topology.json')),
   'away':marker.exists(),'self_active':status.get('peers',{}).get(cfg['node'],{}).get('active'),
   'direct_peer_names':sorted(status.get('peers',{})),'backup_exists':backup.exists()}
operation=request['operation']
if operation=='preflight':
 assert digest(request['candidate'])==request['candidate_sha256']
 assert digest(request['installer'])==request['installer_sha256']
 output=state()
elif operation=='state':output=state()
elif operation=='away':
 value=request['value'];assert type(value) is bool
 if not value:assert digest(installed)==request['resume_sha256'],'Refuse to resume an unexpected executable'
 user_command([str(installed),'--config',str(config),'--away',str(value).lower()]);output=state()
elif operation in ('install','rollback'):
 before=state();assert before['away'] and before['self_active'] is False
 if before['mode']=='full':quiescence()
 assert digest(request['installer'])==request['installer_sha256']
 arguments=['python3',request['installer'],'--user',request['user']]
 if operation=='install':
  assert before['running_sha256']==request['previous_sha256'] and not backup.exists()
  assert digest(request['candidate'])==request['candidate_sha256']
  arguments+=['--candidate',request['candidate'],'--expected-current-sha256',request['previous_sha256'],
    '--expected-candidate-sha256',request['candidate_sha256'],'--backup-id',request['backup_id']]
 else:arguments+=['--rollback',str(backup)]
 output=json.loads(subprocess.check_output(arguments,text=True,timeout=100));output['state']=state()
 assert output['state']['away'],'Maintenance absence was lost'
elif operation=='quiescence':
 output=state();assert output['away'] and output['self_active'] is False
 if output['mode']=='full':output['injected_inputs_down']=quiescence()
elif operation=='normalize_backup':
 if backup.exists():
  manifest_path=backup/'manifest.json';manifest=json.loads(manifest_path.read_text())
  assert manifest['previous_sha256']==request['previous_sha256']
  (backup/'config/agent.away').unlink(missing_ok=True)
  manifest.update(original_participation_away=False,cohort_paused_before_replacement=True)
  manifest_path.write_text(json.dumps(manifest,indent=2)+'\n')
 output={'backup':str(backup),'normalized':backup.exists()}
else:raise AssertionError('Unknown maintenance operation')
print(json.dumps(output))
'''

TARGETS = [('gbs-p3', 'gbs-p3', 'clipboard_only'), ('gbs-p2', 'gbs-p2', 'clipboard_only'),
           ('local', 'zaza-desktop', 'clipboard_only'), ('gbs-asus-vpn', 'asus', 'full'),
           ('gbs-acer-vpn', 'acer', 'full')]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--apply', action='store_true')
    parser.add_argument('--user', default='zaza')
    parser.add_argument('--candidate', required=True)
    parser.add_argument('--installer', required=True)
    parser.add_argument('--expected-current-sha256', required=True)
    parser.add_argument('--expected-candidate-sha256', required=True)
    parser.add_argument('--expected-installer-sha256', required=True)
    parser.add_argument('--backup-id', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    assert re.fullmatch(r'[A-Za-z0-9_.-]+', args.backup_id)
    for fingerprint in (args.expected_current_sha256, args.expected_candidate_sha256, args.expected_installer_sha256):
        assert re.fullmatch(r'[0-9a-f]{64}', fingerprint)
    assert Path(args.candidate).is_absolute() and Path(args.installer).is_absolute()
    common = {'user': args.user, 'candidate': args.candidate, 'installer': args.installer,
              'previous_sha256': args.expected_current_sha256,
              'candidate_sha256': args.expected_candidate_sha256,
              'installer_sha256': args.expected_installer_sha256, 'backup_id': args.backup_id}
    report = {'apply_requested': args.apply, 'preflight': {}, 'installations': {}, 'rollbacks': {},
              'maintenance_resumed': False, 'source_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
    paused = False

    def checkpoint():
        temporary=args.output.with_name(args.output.name+'.tmp')
        with temporary.open('w') as stream:
            temporary.chmod(0o600)
            stream.write(json.dumps(report, indent=2) + '\n')
            stream.flush();os.fsync(stream.fileno())
        temporary.replace(args.output)

    def call(host, operation, **extra):
        command = (['sudo', '-n', 'python3', '-c', REMOTE] if host == 'local' else
                   ['ssh', '-o', 'BatchMode=yes', '-o', 'ConnectTimeout=10', host,
                    'python3 -c ' + shlex.quote(REMOTE)])
        process = subprocess.run(command, input=json.dumps({**common, 'operation': operation, **extra}),
                                 text=True, capture_output=True, timeout=120)
        if process.returncode:
            # Never include the embedded source or inherited environment in a
            # public report. Retain the diagnostic in a private adjacent file.
            diagnostic = args.output.with_name(args.output.stem + '-' + host + '-' + operation + '-error.log')
            diagnostic.write_text(process.stderr)
            diagnostic.chmod(0o600)
            raise RuntimeError(host + ': ' + operation + ' failed; private diagnostic retained')
        return json.loads(process.stdout)

    def verify(state, before, fingerprint, absent):
        assert state['running_sha256'] == fingerprint
        for field in ('node', 'mode', 'uid', 'display', 'configuration_sha256', 'layout_sha256'):
            assert state[field] == before[field], 'Changed original ' + field
        assert state['away'] is absent

    try:
        for host, node, mode in TARGETS:
            state = call(host, 'preflight')
            assert state['node'] == node and state['mode'] == mode
            assert state['running_sha256'] == args.expected_current_sha256
            assert not state['away'] and state['self_active'] is True and not state['backup_exists']
            report['preflight'][host] = state
        checkpoint()
        if not args.apply:
            report['dry_run_passed'] = True
            return 0
        # Full-mode nodes leave first, releasing source/target injected input.
        paused = True
        for host, _, _ in [target for target in TARGETS if target[2] == 'full'] + [target for target in TARGETS if target[2] != 'full']:
            call(host, 'away', value=True)
            print(json.dumps({'phase': 'paused', 'host': host}), flush=True)
        time.sleep(4)
        for host, _, _ in TARGETS:
            call(host, 'quiescence')
        report['cohort_quiescence_verified'] = True
        checkpoint()
        for host, _, _ in TARGETS:
            result = call(host, 'install')
            verify(result['state'], report['preflight'][host], args.expected_candidate_sha256, True)
            report['installations'][host] = result
            checkpoint()
            print(json.dumps({'phase': 'installed_while_absent', 'host': host}), flush=True)
        for host, _, _ in TARGETS:
            verify(call(host, 'state'), report['preflight'][host], args.expected_candidate_sha256, True)
        report['uniform_candidate_cohort_verified'] = True
        checkpoint()
    except Exception as error:
        report['error'] = str(error)
        if paused:
            # A lost connection may hide a completed installation. Inspect
            # every host, including the failed host, before choosing rollback.
            try:
                for host, _, _ in reversed(TARGETS):
                    call(host, 'away', value=True)
                    state = call(host, 'state')
                    if state['running_sha256'] == args.expected_candidate_sha256:
                        report['rollbacks'][host] = call(host, 'rollback')
                    elif state['running_sha256'] != args.expected_current_sha256:
                        raise AssertionError('Unknown running executable on ' + host)
                for host, _, _ in TARGETS:
                    verify(call(host, 'state'), report['preflight'][host], args.expected_current_sha256, True)
                report['uniform_previous_cohort_restored'] = True
            except Exception as rollback_error:
                report['rollback_error'] = str(rollback_error)
        return 1
    finally:
        # An incomplete/unknown cohort stays absent. Its local input remains
        # usable; no active mixed-version KVM pool is silently resumed.
        can_resume = report.get('uniform_candidate_cohort_verified') or report.get('uniform_previous_cohort_restored')
        if paused and can_resume:
            try:
                resume_sha=(args.expected_candidate_sha256 if report.get('uniform_candidate_cohort_verified') else args.expected_current_sha256)
                for host, _, _ in TARGETS:
                    call(host, 'normalize_backup')
                for host, _, _ in TARGETS:
                    call(host, 'away', value=False, resume_sha256=resume_sha)
                final_states={}
                for host, _, _ in TARGETS:
                    deadline=time.monotonic()+10
                    while time.monotonic()<deadline:
                        state=call(host,'state')
                        verify(state,report['preflight'][host],resume_sha,False)
                        if state['self_active'] is True:break
                        time.sleep(.2)
                    else:raise AssertionError('Participation did not resume on '+host)
                    final_states[host]=state
                report['final_states']=final_states
                report['maintenance_resumed'] = True
            except Exception as resume_error:
                report['resume_error'] = str(resume_error)
        checkpoint()
        print(json.dumps({key: report.get(key) for key in
              ('dry_run_passed', 'uniform_candidate_cohort_verified', 'uniform_previous_cohort_restored',
               'maintenance_resumed', 'error', 'rollback_error', 'resume_error')}), flush=True)
    return 0 if report.get('uniform_candidate_cohort_verified') and report['maintenance_resumed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
