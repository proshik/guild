#!/usr/bin/env python3
"""Prepare a free, real-revmux native acceptance fixture; never launch providers."""
from pathlib import Path
import argparse, json, tempfile, shlex
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--revmux-checkout', type=Path, required=True)
parser.add_argument('--guild-bin', type=Path, default=Path(__file__).resolve().parents[2] / 'target/debug')
options = parser.parse_args()
repo = options.revmux_checkout.resolve()
guild_bin = options.guild_bin.resolve()
for required in (repo / 'app/executor/testdata/claude-clean.jsonl', guild_bin / 'guild'):
    if not required.is_file():
        parser.error(f'missing required fixture input: {required}')
root=Path(tempfile.mkdtemp(prefix='guild-terminal-e2e-',dir='/tmp')).resolve()
project=root/'project'; project.mkdir()
(root/'state').mkdir(); (root/'shots').mkdir(); (root/'bin').mkdir()
config=project/'config'; (config/'prompts/profiles').mkdir(parents=True)
(config/'prompts/profiles/guild-fixture.md').write_text('''---
model: claude/opus:high
agents:
  - {name: one, lenses: [bugs]}
  - {name: two, lenses: [bugs]}
---
Review {{SCOPE}} in {{WORKDIR}}. Controlled fixture; no model is called.
''')
for stage in ('synthesis','verify'):
    original=(repo/f'app/prompt/defaults/prompts/{stage}.md').read_text()
    (config/f'prompts/{stage}.md').write_text(original.replace('---\n','---\nmodel: claude/opus:high\n',1))
records=[json.loads(line) for line in (repo/'app/executor/testdata/claude-clean.jsonl').read_text().splitlines()]
last=next(r for r in reversed(records) if r.get('type')=='result')
(root/'recorded-result.json').write_text(json.dumps(last))
provider='''#!/usr/bin/env python3
import json,sys,time
from pathlib import Path
root=Path(__file__).resolve().parents[1]
if '--version' in sys.argv: print('controlled-claude-fixture'); sys.exit()
prompt=sys.stdin.read()
schema=json.loads(sys.argv[sys.argv.index('--json-schema')+1])
properties=schema['properties']
finding={'file':'sample.py','line':1,'end_line':1,'severity':'major','confidence':95,'title':'Controlled fixture finding','body':'Deterministic finding for terminal host acceptance.','lenses':['bugs']}
if 'verdicts' in properties:
    stage='verify'; output={'verdicts':[{'id':'one-1','verdict':'confirmed'}]}
elif 'open_questions' in properties:
    stage='synthesis'; finding.pop('lenses'); finding['merged_ids']=['one-1','two-1']; output={'findings':[finding],'open_questions':[],'pre_existing':[]}
else:
    stage='find'; output={'findings':[finding]}
with (root/'stages.jsonl').open('a') as f: f.write(json.dumps({'stage':stage,'argv':sys.argv[1:]})+'\\n')
record=json.loads((root/'recorded-result.json').read_text())
record['structured_output']=output; record['result']=json.dumps(output); record['total_cost_usd']=0
for model in record.get('modelUsage',{}).values(): model['costUSD']=0
time.sleep(1)
print(json.dumps(record),flush=True)
'''
(root/'bin/claude').write_text(provider); (root/'bin/claude').chmod(0o755)
(root/'bin/codex').write_text('#!/bin/sh\necho "unexpected executor in free fixture" >&2\nexit 90\n');(root/'bin/codex').chmod(0o755)
(project/'sample.py').write_text('raise RuntimeError("controlled fixture")\n')
launcher=repo/'.claude-plugin/skills/revmux/scripts/launch-revmux.sh'
script='''#!/bin/bash
set -eu
root=ROOT
guild_bin=GUILDBIN
export PATH="$root/bin:$guild_bin:$PATH"
export REVMUX_HOST=guild
export REVMUX_AUTO_EXIT=2s
run=${1:-automatic}
mkdir -p "$root/project/tasks/fixture/$run/input"
printf 'Read sample.py. This is a controlled host fixture, not a real review.\\n' > "$root/project/tasks/fixture/$run/input/scope.md"
python3 -c 'import json,os,sys; json.dump({k:os.environ.get(k) for k in ("GUILD_SESSION","GUILD_SOCKET")},open(sys.argv[1],"w"))' "$root/identity.json"
code=0
bash LAUNCHER --config-dir "$root/project/config" --tasks-dir "$root/project/tasks" --task fixture --run "$run" --profile guild-fixture --stagger-delay=0s "${@:2}" > "$root/$run.report.json" 2> "$root/$run.stderr" || code=$?
printf '%s\\n' "$code" > "$root/$run.exit"
printf '\\nFixture %s finished with exit %s.\\n' "$run" "$code"
exit "$code"
'''.replace('ROOT',shlex.quote(str(root))).replace('GUILDBIN',shlex.quote(str(guild_bin))).replace('LAUNCHER',shlex.quote(str(launcher)))
(root/'run-free.sh').write_text(script);(root/'run-free.sh').chmod(0o755)
input_script = '''#!/bin/bash
set -eu
python3 - @ROOT@ @INPUT@ "$@" <<'PY'
import json, os, sys, uuid
from pathlib import Path
root = Path(sys.argv[1])
request = {
    'version': 1, 'request_id': str(uuid.uuid4()),
    'program': sys.executable,
    'args': [sys.argv[2], str(root / 'input-check'), *sys.argv[3:]],
    'cwd': str(root / 'project'),
    'target': {'kind': 'session', 'id': os.environ['GUILD_SESSION']},
    'title': 'Input fixture', 'foreground': False,
    'geometry': {'width_percent': 70, 'height_percent': 65},
}
(root / 'input-request.json').write_text(json.dumps(request))
PY
@GUILD@ terminal start --from-file @REQUEST@ --json > @RECEIPT@
'''
for key, value in {
    'ROOT': root,
    'INPUT': Path(__file__).with_name('terminal-input-fixture.py').resolve(),
    'GUILD': guild_bin / 'guild',
    'REQUEST': root / 'input-request.json',
    'RECEIPT': root / 'input-start.json',
}.items():
    input_script = input_script.replace('@' + key + '@', shlex.quote(str(value)))
(root / 'start-input.sh').write_text(input_script)
print(root)
