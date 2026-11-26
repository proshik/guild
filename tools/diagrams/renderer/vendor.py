"""Explicit developer-only vendoring of pinned official browser files; no npm hooks.
Existing archives under target/webkit-spike are preferred. --fetch explicitly permits
fetching missing immutable inputs. Never invoked by Cargo, check, bundle or the app.
"""
from pathlib import Path
import argparse, base64, hashlib, io, json, tarfile, urllib.request

ROOT = Path(__file__).resolve().parents[3]
DEST = ROOT / 'crates/desktop/assets/diagrams'
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--fetch', action='store_true')
args = parser.parse_args()
manifest = json.loads((DEST / 'PROVENANCE.json').read_text())
records = []
for package, filename in zip(manifest['engines'], ['mermaid-upstream.tgz', 'core.tgz']):
    archive = ROOT / 'target/webkit-spike' / filename
    if not archive.exists():
        if not args.fetch: raise SystemExit(f'Missing {archive}; explicit --fetch required')
        with urllib.request.urlopen(package['url'], timeout=30) as response:
            data = response.read(package['downloadBytes'] + 1)
    else: data = archive.read_bytes()
    assert len(data) == package['downloadBytes'], 'upstream archive length mismatch'
    assert 'sha512-' + base64.b64encode(hashlib.sha512(data).digest()).decode() == package['integrity'], 'upstream SHA512 mismatch'
    if not archive.exists(): archive.parent.mkdir(parents=True, exist_ok=True); archive.write_bytes(data)
    folder = DEST / (package['id'] + '-' + package['version']); folder.mkdir(exist_ok=True)
    with tarfile.open(fileobj=io.BytesIO(data)) as tar:
        for item in package['files']:
            entry = tar.getmember(item['path'])
            assert entry.isfile() and entry.size == item['bytes'], 'invalid admitted entry'
            payload = tar.extractfile(entry).read(item['bytes'] + 1)
            assert len(payload) == item['bytes'] and hashlib.sha256(payload).hexdigest() == item['sha256']
            path = folder / item['name']; path.write_bytes(payload)
            records.append((path.relative_to(DEST).as_posix(), item['sha256']))
for item in manifest['notices']:
    path = DEST / item['destination']
    if path.exists(): data = path.read_bytes()
    else:
        if not args.fetch: raise SystemExit(f'Missing notice {path}; explicit --fetch required')
        with urllib.request.urlopen(item['url'], timeout=30) as response: data = response.read(item['bytes'] + 1)
    assert len(data) == item['bytes'] and hashlib.sha256(data).hexdigest() == item['sha256'], 'notice integrity mismatch'
    path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(data)
    records.append((item['destination'],item['sha256']))
(DEST / 'files.sha256').write_text(''.join(f'{digest}  {name}\n' for name,digest in sorted(records)))
print(f'Vendored {len(records)} exact files; {sum((DEST/name).stat().st_size for name,_ in records)} bytes')
