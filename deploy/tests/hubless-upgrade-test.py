#!/usr/bin/env python3
"""Exercise real backup/install/rollback files with an isolated fake user service."""
import importlib.util
import io
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch
from contextlib import redirect_stdout

spec=importlib.util.spec_from_file_location('upgrade',Path(__file__).parents[1]/'apply-hubless-upgrade.py')
upgrade=importlib.util.module_from_spec(spec);spec.loader.exec_module(upgrade)


@unittest.skipUnless(os.geteuid()==0,'run as root to qualify ownership preservation')
class UpgradeTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory(prefix='poolsync-upgrade-fixture-')
        self.root=Path(self.temp.name);self.home=self.root/'home';self.bundle=self.root/'bundle'
        self.home.mkdir();self.bundle.mkdir()
        self.config=self.home/'.config/poolsync/agent.toml';self.config.parent.mkdir(parents=True)
        self.old='''node="a"
node_token="fixture-node-a"
token="fixture-legacy"
e2e_key="fixture-pool"
hubless=false
peer_direct_clipboard=true
hub_clipboard=false
mode="full"
kvm_enabled=true
peer_tls_cert="/fixture/a.crt"
peer_tls_key="/fixture/a.key"
[screen]
width=1600
height=900
[peer_tokens]
b="fixture-node-b"
[[neighbors]]
node="b"
direction="right"
peer_url="wss://10.0.0.2:9472/ws"
'''
        self.config.write_text(self.old);self.config.chmod(0o600)
        (self.config.parent/'agent.away').write_text('away')
        (self.config.parent/'tls').mkdir()
        (self.config.parent/'tls/a.key').write_text('fixture-private-key-preserved')
        (self.config.parent/'tls/a.key').chmod(0o600)
        (self.bundle/'agent.toml').write_text(self.old.replace('hubless=false','hubless=true'))
        (self.bundle/'agent.topology.json').write_text('{"revision":1,"origin":"a","topology":{"nodes":{"a":{"x":42,"y":18}}}}')
        (self.bundle/'poolsync-agent').write_text('#!/bin/sh\necho "poolsync-agent fixture-version"\n');(self.bundle/'poolsync-agent').chmod(0o755)
        self.originals={}
        for relative in {'.local/bin/poolsync-agent',*upgrade.FILES.values()}:
            path=self.home/relative;path.parent.mkdir(parents=True,exist_ok=True);path.write_text('original '+relative);path.chmod(0o755 if '/bin/' in relative else 0o644)
            self.originals[relative]=path.read_bytes()
        for source in upgrade.FILES:
            path=self.bundle/source;path.parent.mkdir(parents=True,exist_ok=True);path.write_text('replacement '+source)
        self.account=SimpleNamespace(pw_dir=str(self.home),pw_uid=1000,pw_gid=1000)
        self.calls=[]

    def tearDown(self):self.temp.cleanup()

    def command(self,args,**kwargs):
        self.calls.append(args)
        if args[0]=='ldd':return subprocess.CompletedProcess(args,0,'fixture dependencies present','')
        if args[0]==str(self.bundle/'poolsync-agent'):
            return subprocess.CompletedProcess(args,0,'poolsync-agent fixture-version\n','')
        assert args[0]=='runuser',args
        return subprocess.CompletedProcess(args,0,'enabled\n','')

    def invoke(self,*extra):
        args=['upgrade','--bundle',str(self.bundle),'--expected-original-config-sha256',upgrade.digest(self.config),'--expected-candidate-sha256',upgrade.digest(self.bundle/'poolsync-agent'),'--backup-id','fixture-backup',*extra]
        with patch.object(sys,'argv',args),patch.object(upgrade.pwd,'getpwnam',return_value=self.account),patch.object(upgrade.subprocess,'run',side_effect=self.command),redirect_stdout(io.StringIO()):upgrade.main()

    def assert_originals(self):
        self.assertEqual(self.config.read_text(),self.old)
        for relative,data in self.originals.items():self.assertEqual((self.home/relative).read_bytes(),data)
        self.assertEqual((self.config.parent/'agent.away').read_text(),'away')
        self.assertEqual((self.config.parent/'tls/a.key').read_text(),'fixture-private-key-preserved')
        self.assertEqual((self.config.parent/'tls/a.key').stat().st_mode&0o777,0o600)
        self.assertFalse(self.config.with_suffix('.topology.json').exists())

    def test_preflight_changes_no_user_files_or_services(self):
        self.invoke('--check-only');self.assert_originals()
        self.assertFalse(any(args[0]=='runuser' for args in self.calls))
        self.assertFalse((self.home/'.local/state/poolsync/deployment-backups').exists())

    def test_install_preserves_private_state_and_explicit_rollback_restores_every_file(self):
        self.invoke()
        self.assertIn('hubless=true',self.config.read_text())
        self.assertEqual((self.config.parent/'tls/a.key').read_text(),'fixture-private-key-preserved')
        self.assertEqual((self.config.parent/'agent.away').read_text(),'away')
        backup=self.home/'.local/state/poolsync/deployment-backups/fixture-backup'
        self.assertEqual(backup.stat().st_mode&0o777,0o700)
        self.invoke('--rollback',str(backup));self.assert_originals()

    def test_partial_install_failure_automatically_rolls_back(self):
        (self.bundle/'poolsync-watchdog.sh').unlink()
        with self.assertRaises(FileNotFoundError):self.invoke()
        self.assert_originals()


if __name__=='__main__':unittest.main()
