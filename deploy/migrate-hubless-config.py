#!/usr/bin/env python3
"""Render a private, identity-preserving migration from existing pool configs.

This tool never contacts a host, installs an agent or modifies its inputs.
Routes are explicit JSON node-to-WSS-URL mappings. No credentials are printed.
"""
import argparse
import copy
import json
import os
from pathlib import Path
import re
import tomllib
from urllib.parse import urlparse


def root_value(raw, key, value):
    match = re.search(r'^\s*\[', raw, re.M)
    end = match.start() if match else len(raw)
    head, tail = raw[:end], raw[end:]
    rendered = json.dumps(value) if not isinstance(value, bool) else str(value).lower()
    pattern = rf'^{re.escape(key)}\s*=.*$'
    if re.search(pattern, head, re.M):
        head = re.sub(pattern, key + ' = ' + rendered, head, flags=re.M)
    else:
        head = key + ' = ' + rendered + '\n' + head
    return head + tail


def render(configs, routes, topology):
    assert set(configs) == set(routes), 'routes must cover exactly the configured members'
    assert set(topology['nodes']) == set(configs), 'saved topology must cover exactly the pool members'
    keys = {c['e2e_key'] for _, c in configs.values()}
    assert len(keys) == 1 and next(iter(keys)), 'existing pool encryption keys differ or are missing'
    tokens = {name: c['node_token'] for name, (_, c) in configs.items()}
    assert all(tokens.values()) and len(set(tokens.values())) == len(tokens), 'invalid existing node identities'
    output = {}
    for name, (raw, cfg) in configs.items():
        assert cfg['node'] == name, 'filename and existing node identity disagree'
        for field in ('peer_tls_cert', 'peer_tls_key'):
            assert cfg.get(field), 'migration requires the existing TLS identity'
        assert cfg.get('peer_direct_clipboard'), 'direct peer transport is disabled'
        assert not cfg.get('hub_clipboard', True), 'retire the hub clipboard dependency first'
        for peer, token in cfg.get('peer_tokens', {}).items():
            if peer in tokens:
                assert token == tokens[peer], 'existing authorization differs from peer identity'
        for peer, url in routes.items():
            u = urlparse(url)
            assert u.scheme == 'wss' and u.hostname and u.port and u.path == '/ws', 'explicit encrypted routes required'
        edited = root_value(raw, 'hubless', True)
        peers = cfg.get('peer_tokens', {})
        additions = ''.join(f'{json.dumps(peer)} = {json.dumps(token)}\n' for peer, token in sorted(tokens.items()) if peer != name and peer not in peers)
        if additions:
            section = re.search(r'^\[peer_tokens\]\s*$', edited, re.M)
            if section:
                next_section = re.search(r'^\s*\[', edited[section.end():], re.M)
                end = section.end() + next_section.start() if next_section else len(edited)
                edited = edited[:end].rstrip() + '\n' + additions + '\n' + edited[end:]
            else:
                edited += '\n[peer_tokens]\n' + additions
        existing = {n['node'] for n in cfg.get('neighbors', [])}
        for peer, url in sorted(routes.items()):
            if peer != name and peer not in existing:
                edited += '\n[[neighbors]]\nnode = ' + json.dumps(peer) + '\ndirection = "right"\npeer_url = ' + json.dumps(url) + '\n'
        candidate = tomllib.loads(edited)
        # Preserve every original field, including unknown future settings,
        # comments, fallback routes and existing credentials.
        baseline = copy.deepcopy(candidate)
        baseline.pop('hubless', None)
        original = copy.deepcopy(cfg)
        original.pop('hubless', None)
        if 'peer_tokens' in original:baseline['peer_tokens'] = original['peer_tokens']
        else:baseline.pop('peer_tokens', None)
        if 'neighbors' in original:baseline['neighbors'] = [n for n in baseline['neighbors'] if n['node'] in existing]
        else:baseline.pop('neighbors', None)
        assert baseline == original, 'migration changed an unrelated setting'
        assert set(n['node'] for n in candidate['neighbors']) >= set(routes) - {name}
        assert all(candidate['peer_tokens'][p] == t for p, t in tokens.items() if p != name)
        output[name] = edited
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input-directory', type=Path, required=True)
    parser.add_argument('--routes', type=Path, required=True)
    parser.add_argument('--saved-topology', type=Path, required=True)
    parser.add_argument('--output-directory', type=Path, required=True)
    parser.add_argument('--layout-author', required=True)
    args = parser.parse_args()
    routes = json.loads(args.routes.read_text())
    configs = {name: (raw, tomllib.loads(raw)) for name in routes for raw in [(args.input_directory/(name+'.toml')).read_text()]}
    topology = json.loads(args.saved_topology.read_text())
    rendered = render(configs, routes, topology)
    assert args.layout_author in rendered, 'layout author must be a pool member'
    args.output_directory.mkdir(mode=0o700, parents=True, exist_ok=False)
    os.chmod(args.output_directory, 0o700)
    document = {'revision': 1, 'origin': args.layout_author, 'topology': topology}
    for name, raw in rendered.items():
        for suffix, content in [('toml', raw), ('topology.json', json.dumps(document, indent=2)+'\n')]:
            target = args.output_directory/(name+'.'+suffix)
            target.write_text(content)
            target.chmod(0o600)
    print(json.dumps({'rendered_members': sorted(rendered), 'identities_and_existing_settings_preserved': True, 'installed': False}))


if __name__ == '__main__':
    main()
