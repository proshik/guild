"""Explicit local build preparation. Uses installed Apple tools and vendored JS only.
No downloads, runtime dependency install, archive delivery or user state writes.
"""
from pathlib import Path
import argparse, hashlib, platform, shutil, subprocess

ROOT = Path(__file__).resolve().parents[3]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--output', type=Path, default=ROOT/'target/diagram-runtime')
args = parser.parse_args()
assert platform.system() == 'Darwin' and platform.machine() == 'arm64', 'Guild supports macOS arm64'
assets=ROOT/'crates/desktop/assets/diagrams'
for line in (assets/'files.sha256').read_text().splitlines():
    digest,name=line.split('  ')
    assert hashlib.sha256((assets/name).read_bytes()).hexdigest()==digest, f'Vendored integrity mismatch: {name}'
args.output.mkdir(parents=True,exist_ok=True)
for source in assets.iterdir():
    if source.is_dir(): shutil.copytree(source,args.output/source.name,dirs_exist_ok=True)
    else: shutil.copy2(source,args.output/source.name)
helper=args.output/'guild-diagram-renderer'
subprocess.run(['xcrun','--no-cache','swiftc','-O','-target','arm64-apple-macos11.0',str(Path(__file__).with_name('host.swift')),
                '-framework','AppKit','-framework','WebKit','-o',str(helper)],check=True)
subprocess.run(['codesign','--force','--sign','-',str(helper)],check=True)
subprocess.run(['codesign','--verify','--strict',str(helper)],check=True)
(args.output/'helper.sha256').write_text(hashlib.sha256(helper.read_bytes()).hexdigest()+'\n')
shutil.copy2(Path(__file__).with_name('LICENSE'),args.output/'HELPER-LICENSE')
print(f'Prepared {args.output}; helper {helper.stat().st_size} bytes; no downloads')
