#!/usr/bin/env python3
"""Build Instigator alone or its optional Linux Podman image."""
import argparse
import json
from build import ROOT, build_lock, execute
import instigator


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['image', 'native'])
    args = parser.parse_args()
    cache = ROOT / 'target/bootstrap'
    destination = ROOT / 'target/install-server'
    destination.mkdir(parents=True, exist_ok=True)
    with build_lock(cache):
        source, info = instigator.fetch(cache)
        if args.action == 'native':
            instigator.build(source, destination / 'instigator', cache,
                             destination / 'licenses')
        else:
            image = f"localhost/prismatic-instigator:{info['instigator_revision'][:12]}"
            execute(['podman', 'build', '--file', ROOT / 'packaging/install-server/Containerfile',
                     '--tag', image, source], cwd=ROOT)
            print(f'Install image: {image} (set this in Network > Installation)')
        (destination / 'build-info.json').write_text(json.dumps(info, indent=2) + '\n')


if __name__ == '__main__':
    main()
