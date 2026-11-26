"""Read-only verification of a built app's fixed renderer resources; no downloads."""
from pathlib import Path
import hashlib, sys
app=Path(sys.argv[1]);assets=app/'Contents/Resources/diagrams'
source=Path(__file__).resolve().parents[3]/'crates/desktop/assets/diagrams'
for line in (source/'files.sha256').read_text().splitlines():
    digest,name=line.split('  ')
    assert hashlib.sha256((assets/name).read_bytes()).hexdigest()==digest, f'Bundled resource mismatch: {name}'
helper=app/'Contents/MacOS/guild-diagram-renderer'
assert hashlib.sha256(helper.read_bytes()).hexdigest()==(assets/'helper.sha256').read_text().strip()
assert (assets/'HELPER-LICENSE').is_file()
print(f'Bundled renderer verified: {app}; {sum(p.stat().st_size for p in assets.rglob("*") if p.is_file())} resource bytes + {helper.stat().st_size} helper bytes')
