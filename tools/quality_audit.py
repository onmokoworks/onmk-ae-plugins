"""Read-only source inventory for the repository-wide AE quality audit.

Only plugins/* package roots and their explicit Cargo workspace members are
visited. No builds, installs, Git commands, GUI launch or external downloads.
Results are local audit evidence, never a test pass.
"""
import argparse
import json
from pathlib import Path
import tomllib


def inventory(root):
    rows = []
    for plugin in sorted((root/'plugins').iterdir()):
        if not plugin.is_dir():
            continue
        manifests = [p for p in (plugin/'Cargo.toml', plugin/'rust'/'Cargo.toml') if p.is_file()]
        # Also record legacy/excluded packages: workspace membership alone
        # would silently omit CelGlow's original effect target.
        extra = sorted(plugin.glob('rust/*/Cargo.toml')) + sorted(plugin.glob('crates/*/Cargo.toml'))
        queue = list(manifests) + extra
        seen = set()
        packages = []
        while queue:
            manifest = queue.pop(0)
            if manifest in seen:
                continue
            seen.add(manifest)
            data = tomllib.loads(manifest.read_text(encoding='utf-8-sig'))
            for member in data.get('workspace', {}).get('members', []):
                for folder in sorted(manifest.parent.glob(member)):
                    candidate = folder/'Cargo.toml'
                    if candidate.is_file() and candidate.resolve().is_relative_to(plugin.resolve()):
                        queue.append(candidate)
            if 'package' not in data:
                continue
            package = data['package']
            lib = data.get('lib', {})
            source = manifest.parent/lib.get('path', 'src/lib.rs')
            build = manifest.parent/package.get('build', 'build.rs') if package.get('build') is not False else None
            packages.append(dict(name=package['name'], manifest=str(manifest.relative_to(root)),
                crate_types=lib.get('crate-type', []), source=str(source.relative_to(root)) if source.is_file() else None,
                build=str(build.relative_to(root)) if build and build.is_file() else None,
                features=sorted(data.get('features', {})), status='not_tested'))
        rows.append(dict(plugin=plugin.name, packages=packages, status='inventoried_not_tested'))
    return dict(schema='onmk.quality_inventory.v1', repository=str(root), plugins=rows,
                plugin_directories=len(rows), note='Inventory is not a build or compatibility pass.')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    result = inventory(args.root.resolve())
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2, ensure_ascii=False), encoding='utf-8')
    print(json.dumps(result, indent=2, ensure_ascii=False))
