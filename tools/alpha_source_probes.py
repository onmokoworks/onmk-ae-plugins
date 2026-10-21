"""Compile two exact production functions against independent straight-alpha oracles.

Audit only: does not edit plugins or load AEX/AE. A failing exit is a finding,
not a broken test harness. Generated files stay under tools/target/alpha-probes.
"""
from pathlib import Path
import hashlib
import json
import subprocess

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT/'tools/target/alpha-probes'


def extract(text, name):
    start = text.index('fn ' + name + '(')
    brace = text.index('{', start)
    depth = 0
    for end in range(brace, len(text)):
        depth += (text[end] == '{') - (text[end] == '}')
        if depth == 0:
            return text[start:end+1]
    raise ValueError('unterminated function')


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    cases = [
        ('luma-normal', 'normal_pixel', '''
#[test] fn partial_normal_keeps_encoded_rgb() {
 assert_eq!(normal_pixel([128,128,255],128), [128,128,128,255]);
}'''),
        ('edge-smith', 'output_rgb', '''
#[test] fn partial_output_keeps_straight_rgb() {
 for depth in [8,16,32] { assert_eq!(output_rgb(1.0,0.5,depth),1.0); }
}'''),
    ]
    reports=[]
    for plugin, name, oracle in cases:
        path=ROOT/'plugins'/plugin/'src/lib.rs'
        source=path.read_text(encoding='utf-8-sig')
        rust=OUT/(plugin+'.rs')
        rust.write_text(extract(source,name)+oracle, encoding='utf-8')
        binary=OUT/(plugin+'.exe')
        build=subprocess.run(['rustc','--edition=2021','--test',str(rust),'-o',str(binary)],capture_output=True,text=True)
        if build.returncode:
            raise RuntimeError(build.stderr)
        result=subprocess.run([str(binary)],capture_output=True,text=True)
        reports.append(dict(plugin=plugin, function=name, source_sha256=hashlib.sha256(path.read_bytes()).hexdigest(),
            contract_passed=result.returncode==0, output=result.stdout+result.stderr,
            scope='exact extracted function; not full plugin host coverage'))
    (OUT/'results.json').write_text(json.dumps(reports,indent=2),encoding='utf-8')
    print(json.dumps(reports,indent=2))
    return 0 if all(r['contract_passed'] for r in reports) else 1


if __name__ == '__main__':
    raise SystemExit(main())
