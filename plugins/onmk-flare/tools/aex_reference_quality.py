"""Bounded headless black-input audit of the three reference looks.

Does not control AE, deploy plugins, or modify AEXCompat. Motion metrics are
diagnostics, not a proof of temporal stability for arbitrary light trajectories.
"""
import argparse
import copy
import hashlib
import json
import math
import os
from pathlib import Path
import subprocess

import numpy as np
from PIL import Image


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--harness', type=Path, required=True)
    parser.add_argument('--plugin', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    records = []

    def invoke(*argv, cpu=False):
        env = dict(os.environ, ONMK_GPU_RESIDENT='0' if cpu else '1',
                   ONMK_GPU_GHOSTS='0' if cpu else '1', ONMK_GPU_SOURCE_OPTICS='0', ONMK_GPU_BLOOM='0')
        p = subprocess.run([str(args.harness), '--headless', *map(str, argv)],
                           capture_output=True, text=True, encoding='utf-8', env=env, timeout=180)
        if p.returncode:
            raise RuntimeError(p.stdout + p.stderr)
        return json.loads(p.stdout)

    params = invoke('--inspect-experimental', args.plugin)
    (root/'parameters.json').write_text(json.dumps(params, indent=2), encoding='utf-8')
    names = {p['name']: p for p in params}
    assert names['Character']['maximum'] == 17
    assert names['Ray Grid']['maximum'] == 128

    def render(label, style, w=321, h=181, frame=0, depth='argb16', cpu=False, zero=False):
        source = root/f'black-{w}x{h}.png'
        if not source.exists():
            Image.new('RGBA', (w,h), (0,0,0,255)).save(source)
        fov_h = math.radians(50)
        fov_v = 2*math.atan(h/w*math.tan(fov_h/2))
        values = {'Character':style, 'Ray Grid':64,
                  'Light Source':[100*(.5+(-.16+frame*.0004)/fov_h),100*(.5-.07/fov_v),0]}
        if zero:
            values['Ghost Intensity'] = 0
        configured = copy.deepcopy(params)
        for p in configured:
            if p['name'] in values:
                p['components' if p['kind']=='point' else 'value'] = values[p['name']]
        fixture = root/f'{label}.json'
        fixture.write_text(json.dumps(dict(schema='aexcompat.render_fixture',schema_version=1,
            primary_layer=source.name,parameters=configured,pixel_format=depth,render_path='classic',
            premultiplication='straight',timing=dict(current_time=0,time_step=1,total_time=1,time_scale=1),
            final_artifact='raw',checkpoints=[dict(id='output',stage='classic-output')]),indent=2),encoding='utf-8')
        dest=root/'renders'/label
        dest.parent.mkdir(exist_ok=True)
        report=invoke('--render-fixture',args.plugin,fixture,dest,cpu=cpu)
        (root/f'{label}.report.json').write_text(json.dumps(report,indent=2),encoding='utf-8')
        raw=(dest/'final'/'output.bin').read_bytes()
        data=np.frombuffer(raw,dtype='<u2' if depth=='argb16' else 'u1').reshape(h,w,4)
        rgb=data[:,:,1:4].astype(np.float64)/(32768 if depth=='argb16' else 255)
        assert np.isfinite(rgb).all()
        if zero:
            assert not rgb.any()
        else:
            assert rgb.max() > .01
            # Catch the previous half-frame failure on this known reference setup.
            assert rgb[:,:w//2].max() > .01 and rgb[:,w//2:].max() > .01
        Image.fromarray(np.rint(np.clip(rgb,0,1)*255).astype('uint8')).save(root/f'{label}.png')
        records.append(dict(case=label,sha256=hashlib.sha256(raw).hexdigest(),width=w,height=h,depth=depth))
        print(f'{len(records)}: {label}',flush=True)
        return rgb

    for style in (15,16,17):
        frames=[render(f'ref-{style}-motion-{f}',style,frame=f-2) for f in range(5)]
        cpu=render(f'ref-{style}-cpu',style,cpu=True)
        error=np.abs(cpu-frames[2])*32768
        assert error.max() <= 2 and error.mean() < .05, (style,error.max(),error.mean())
        repeat=render(f'ref-{style}-repeat',style)
        assert np.array_equal(repeat,frames[2])
        render(f'ref-{style}-zero',style,zero=True)
        render(f'ref-{style}-8bit',style,depth='argb8')
        render(f'ref-{style}-1080',style,w=1920,h=1080)
        curvature=float(np.abs(np.diff(np.stack(frames),n=2,axis=0)).mean())
        records.append(dict(style=style,cpu_gpu_max_units=float(error.max()),cpu_gpu_mean_units=float(error.mean()),
                            temporal_second_difference=curvature))
    (root/'summary.json').write_text(json.dumps(dict(plugin_sha256=hashlib.sha256(args.plugin.read_bytes()).hexdigest(),
        records=records,scope='Black input, 3 reference looks, 5-frame short trajectory, 8/16 bit, odd dimensions, 1080p. No AE GUI certification.'),indent=2),encoding='utf-8')


if __name__ == '__main__':
    main()
