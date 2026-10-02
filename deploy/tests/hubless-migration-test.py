#!/usr/bin/env python3
"""Verify preservation and fail-closed migration with representative fixtures."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('migration', Path(__file__).parents[1]/'migrate-hubless-config.py')
migration = importlib.util.module_from_spec(spec)
spec.loader.exec_module(migration)


class MigrationTests(unittest.TestCase):
    def fixture(self):
        configs = {}
        for name in ('a', 'b', 'c'):
            raw = f'''# Existing custom configuration
node = "{name}"
node_token = "existing-{name}"
e2e_key = "existing-pool-key"
peer_tls_cert = "/existing/{name}.crt"
peer_tls_key = "/existing/{name}.key"
peer_direct_clipboard = true
hub_clipboard = false
mode = "full"
custom_setting = "preserve me"
[screen]
width = 1366
height = 768
'''
            if name == 'a':
                raw += '''[peer_tokens]
b = "existing-b"
[[neighbors]]
node = "b"
direction = "left"
peer_url = "wss://192.168.1.2:9472/ws"
peer_url_vpn = "wss://10.0.0.2:9472/ws"
'''
            configs[name] = raw, migration.tomllib.loads(raw)
        routes = {n:f'wss://10.0.0.{i+1}:9472/ws' for i,n in enumerate(configs)}
        topology = {'nodes':{n:{'x':i*1366,'y':0,'width':1366,'height':768,'kvm_enabled':True} for i,n in enumerate(configs)}}
        return configs, routes, topology

    def test_adds_missing_members_and_preserves_identity_routes_unknown_settings(self):
        configs, routes, topology = self.fixture()
        output = migration.render(configs, routes, topology)
        cfg = migration.tomllib.loads(output['a'])
        self.assertTrue(cfg['hubless'])
        self.assertEqual(cfg['node_token'], 'existing-a')
        self.assertEqual(cfg['e2e_key'], 'existing-pool-key')
        self.assertEqual(cfg['custom_setting'], 'preserve me')
        self.assertEqual(cfg['neighbors'][0], configs['a'][1]['neighbors'][0])
        self.assertEqual(set(cfg['peer_tokens']), {'b','c'})
        self.assertIn('# Existing custom configuration', output['a'])

    def test_conflicting_existing_authorization_is_rejected(self):
        configs, routes, topology = self.fixture()
        configs['a'][1]['peer_tokens']['b'] = 'wrong-identity'
        with self.assertRaises(AssertionError):migration.render(configs, routes, topology)

    def test_different_pool_keys_are_rejected(self):
        configs, routes, topology = self.fixture()
        configs['c'][1]['e2e_key'] = 'different-key'
        with self.assertRaises(AssertionError):migration.render(configs, routes, topology)

    def test_plaintext_or_incomplete_routes_are_rejected(self):
        configs, routes, topology = self.fixture()
        routes['a'] = 'ws://10.0.0.1:9472/ws'
        with self.assertRaises(AssertionError):migration.render(configs, routes, topology)
        del routes['a']
        with self.assertRaises(AssertionError):migration.render(configs, routes, topology)


if __name__ == '__main__':unittest.main()
