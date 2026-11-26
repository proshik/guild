#!/usr/bin/env python3
"""Freeze exact main, isolated/common diagnostic main and the candidate; never git worktrees."""
import hashlib,json,pathlib,subprocess,tarfile,io,shutil,difflib,sys
ROOT=pathlib.Path(__file__).resolve().parents[2];OUT=ROOT/(sys.argv[1] if len(sys.argv)>1 else 'target/adaptive-refresh/comparison');BASE='1b1d553656c6b7b0563d4f3eb9c4300a30be348f';DIAG='5d8b88f01823ede75fb6b274a5344bd3396ef9c6'
OUT.mkdir(exist_ok=True)
def export(ref,name):
 dest=OUT/name;dest.mkdir(exist_ok=False)
 data=subprocess.check_output(['git','archive',ref],cwd=ROOT)
 with tarfile.open(fileobj=io.BytesIO(data)) as archive:archive.extractall(dest)
 return dest
A0=export(BASE,'A-untouched');A=export(DIAG,'A-instrumented');B=export('HEAD','B')
changed=subprocess.check_output(['git','diff','--name-only'],cwd=ROOT,text=True).splitlines()
new=['crates/desktop/src/native_preview/frame_meter.rs','crates/desktop/src/system/terminal_deadline.rs']+[str(p.relative_to(ROOT)) for p in (ROOT/'scripts/terminal-refresh').glob('*') if p.is_file()]
for name in changed+new:
 p=B/name;p.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(ROOT/name,p)
for name in ['crates/desktop/src/system/window_probe.rs','crates/desktop/examples/resource-preview.rs','crates/desktop/src/native_preview.rs','crates/desktop/src/native_preview/adaptive_refresh_probe.rs','crates/desktop/src/native_preview/resource_probe.rs','crates/desktop/src/native_preview/frame_meter.rs']+new[2:]:
 p=A/name;p.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(B/name,p)
# This host callback belongs to the experimental scheduler, not common diagnostics.
p=A/'crates/desktop/src/native_preview/resource_probe.rs';s=p.read_text();start=s.index('        #[cfg(feature = "adaptive-terminal-refresh")]');end=s.index('        div()',start);s=s[:start]+s[end:];p.write_text(s)
# Historical A has neither the runtime feature nor its Settings/Look field.
p=A/'crates/desktop/src/native_preview/resource_probe.rs';s=p.read_text()
start=s.index('    // BEGIN adaptive fixture override');end=s.index('    // END adaptive fixture override.',start)+len('    // END adaptive fixture override.')
p.write_text(s[:start]+s[end:])
# Copy only the diagnostic grid observation method, never scheduler fields/hooks.
p=A/'crates/desktop/src/terminal/view/mod.rs'
source=(B/'crates/desktop/src/terminal/view/mod.rs').read_text()
start=source.index('    /// Synthetic fixture observation only;')
end=source.index('    /// A tab around an already running process.',start)
p.write_text(p.read_text().replace('impl TerminalTab {', 'impl TerminalTab {\n'+source[start:end],1))
# Apply exactly the common generation hooks without any candidate scheduler code.
p=A/'crates/desktop/src/terminal/emulator.rs';s=p.read_text().replace('    term: SharedTerm,','    term: SharedTerm,\n    #[cfg(feature = "native-preview")]\n    pub(crate) frame_generation: Arc<crate::native_preview::frame_meter::Generation>,',1).replace('        let reader_term = Arc::clone(&term);','        #[cfg(feature = "native-preview")]\n        let frame_generation = Arc::new(crate::native_preview::frame_meter::Generation::default());\n        #[cfg(feature = "native-preview")]\n        let reader_generation = frame_generation.clone();\n        let reader_term = Arc::clone(&term);',1).replace('                                    parser.advance(&mut *term, &buffer[..count]);','                                    parser.advance(&mut *term, &buffer[..count]);\n                                    #[cfg(feature = "native-preview")]\n                                    reader_generation.advance();',1).replace('        Ok(Self {\n            term,','        Ok(Self {\n            #[cfg(feature = "native-preview")]\n            frame_generation,\n            term,',1);p.write_text(s)
p=A/'crates/desktop/src/terminal/view/drawing.rs';s=p.read_text().replace('        #[cfg(feature = "native-preview")]\n        let held','        #[cfg(feature = "native-preview")]\n        crate::native_preview::frame_meter::capture(&term, &self.emulator.frame_generation);\n\n        #[cfg(feature = "native-preview")]\n        let held',1);p.write_text(s)
p=A/'crates/desktop/src/system.rs';extra=(B/'crates/desktop/src/system.rs').read_text();start=extra.index('#[cfg(feature = "native-preview")]\npub fn probe_mach_ticks');p.write_text(p.read_text()+'\n'+extra[start:])
assert not (A/'crates/desktop/src/terminal/refresh.rs').exists()
assert 'adaptive-terminal-refresh' not in (A/'crates/desktop/Cargo.toml').read_text()
manifest={};patch=[]
for name,directory in [('A-untouched',A0),('A-instrumented',A),('B',B)]:
 files={str(p.relative_to(directory)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(directory.rglob('*')) if p.is_file()}
 manifest[name]={'source_digest':hashlib.sha256(''.join(k+'\0'+v+'\n' for k,v in sorted(files.items())).encode()).hexdigest(),'files':files}
for name in sorted(set(manifest['A-untouched']['files'])|set(manifest['A-instrumented']['files'])):
 if manifest['A-untouched']['files'].get(name)==manifest['A-instrumented']['files'].get(name):continue
 # Only common diagnostic/isolation source and tools, no plan/runtime feature.
 if not (name.startswith('crates/') or name.startswith('scripts/') or name == 'Cargo.lock'):
  original=A0/name;dest=A/name
  if original.exists():shutil.copyfile(original,dest)
  elif dest.exists():dest.unlink()
  continue
 before=(A0/name).read_text() if (A0/name).exists() else '';after=(A/name).read_text() if (A/name).exists() else ''
 patch.extend(difflib.unified_diff(before.splitlines(True),after.splitlines(True),fromfile='a/'+name,tofile='b/'+name))
text=''.join(patch);(OUT/'common-diagnostics.patch').write_text(text)
# Recompute after excluding historical diagnostic documents from the A delta.
for name,directory in [('A-untouched',A0),('A-instrumented',A),('B',B)]:
 files={str(p.relative_to(directory)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(directory.rglob('*')) if p.is_file()}
 manifest[name]={'source_digest':hashlib.sha256(''.join(k+'\0'+v+'\n' for k,v in sorted(files.items())).encode()).hexdigest(),'files':files}
manifest['base']=BASE;manifest['diagnostics_patch_sha256']=hashlib.sha256(text.encode()).hexdigest()
(OUT/'sources.json').write_text(json.dumps(manifest,indent=2)+'\n');print(manifest['diagnostics_patch_sha256'])
