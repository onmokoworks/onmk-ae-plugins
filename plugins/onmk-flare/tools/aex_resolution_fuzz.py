"""Reproducible, bounded headless AEXCompat resolution/parameter fuzzing.

No GUI events. CPU-optics still uses GPU final composition. Blank random flares
are observations, not failures: off-screen rays may legitimately miss the sensor.
"""
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import random
import subprocess
import numpy as np
from PIL import Image


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--harness', required=True)
    ap.add_argument('--plugin', required=True)
    ap.add_argument('--output', type=Path, required=True)
    ap.add_argument('--seed', type=int, default=20261003)
    ap.add_argument('--random-depth', choices=['argb8','argb16'], default='argb8')
    args = ap.parse_args()
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    rng = random.Random(args.seed)
    records = []
    identity = dict(seed=args.seed, random_depth=args.random_depth,
                    plugin_sha256=hashlib.sha256(Path(args.plugin).read_bytes()).hexdigest())

    def invoke(argv, gpu=True):
        env = dict(os.environ, ONMK_GPU_RESIDENT=str(int(gpu)), ONMK_GPU_GHOSTS=str(int(gpu)),
                   ONMK_GPU_SOURCE_OPTICS='0', ONMK_GPU_BLOOM='0')
        run = subprocess.run([args.harness, '--headless', *map(str,argv)],
                             capture_output=True, text=True, encoding='utf-8', env=env, timeout=180)
        if run.returncode:
            raise RuntimeError(f'exit={run.returncode}: {run.stderr}\n{run.stdout}')
        return json.loads(run.stdout)

    params = invoke(['--inspect-experimental',args.plugin])
    (root/'discovery.json').write_text(json.dumps(params,indent=2),encoding='utf-8')
    dimensions = [(1,1),(1,257),(257,1),(17,31),(63,65),(319,181),(321,179),
                  (511,513),(1023,577),(1081,1919),(2560,1440),(3840,2160)]
    dimensions += [(rng.randint(32,1400),rng.randint(32,1000)) for _ in range(12)]
    (root/'plan.json').write_text(json.dumps(dict(identity,dimensions=dimensions),indent=2),encoding='utf-8')

    def save():
        (root/'summary.json').write_text(json.dumps(dict(identity,results=records),indent=2),encoding='utf-8')

    def render(label,w,h,values,source='black.png',depth='argb8',gpu=True):
        ps=copy.deepcopy(params)
        for p in ps:
            if p['name'] in values:
                key='components' if p['kind']=='point' else 'color' if p['kind']=='color' else 'value'
                p[key]=values[p['name']]
        fixture=dict(schema='aexcompat.render_fixture',schema_version=1,primary_layer=source,
                     parameters=ps,pixel_format=depth,render_path='classic',premultiplication='straight',
                     timing=dict(current_time=0,time_step=1,total_time=1,time_scale=1),final_artifact='raw',
                     checkpoints=[dict(id='input',stage='classic-input'),dict(id='output',stage='classic-output')])
        path=root/f'{label}.json'
        path.write_text(json.dumps(fixture,indent=2),encoding='utf-8')
        out=root/label
        entry=dict(case=label,width=w,height=h,depth=depth,gpu_optics=gpu)
        records.append(entry)
        try:
            report=invoke(['--render-fixture',args.plugin,path,out],gpu)
            (root/f'{label}.report.json').write_text(json.dumps(report,indent=2),encoding='utf-8')
            dtype=np.uint8 if depth=='argb8' else np.dtype('<u2')
            data=np.fromfile(out/'final'/'output.bin',dtype=dtype)
            assert data.size==w*h*4, f'Wrong output size: {data.size}'
            data=data.reshape(h,w,4)
            entry.update(status='rendered',sha256=hashlib.sha256(data.tobytes()).hexdigest(),
                         nonzero_rgb=int(np.count_nonzero(data[:,:,1:])),peak=int(data[:,:,1:].max()))
            save()
            return data,out,entry
        except Exception as error:
            entry.update(status='error',error=str(error))
            save()
            return None,out,entry

    def check(entry, condition, message):
        if not condition:
            entry.setdefault('failures',[]).append(message)
            entry['status']='failed'
        save()

    for case,(w,h) in enumerate(dimensions):
        Image.new('RGBA',(w,h),(0,0,0,255)).save(root/'black.png')
        yy,xx=np.indices((h,w),dtype=np.uint32)
        rgba=np.empty((h,w,4),np.uint8)
        rgba[:,:,0]=(xx*17+yy*3)%251+1
        rgba[:,:,1]=(xx*5+yy*19)%251+1
        rgba[:,:,2]=(xx*11+yy*7)%251+1
        rgba[:,:,3]=255
        Image.fromarray(rgba).save(root/'pattern.png')
        # Copy inputs per resolution so every saved fixture remains replayable.
        black=f'black-{case}.png'; pattern=f'pattern-{case}.png'
        Image.new('RGBA',(w,h),(0,0,0,255)).save(root/black)
        Image.fromarray(rgba).save(root/pattern)
        del rgba,xx,yy
        for depth in ('argb8','argb16'):
            data,out,entry=render(f'{case:02d}-coverage-{depth}',w,h,
                {'Character':6,'Global Brightness':0,'Source Opacity':100,'Transfer Mode':4,
                 'Physical Renderer':0,'Human Eye Glare':0},pattern,depth)
            if data is not None:
                original=np.fromfile(out/'checkpoints'/'input'/'output.bin',dtype=data.dtype).reshape(h,w,4)
                delta=np.abs(data.astype(np.int32)-original.astype(np.int32))
                entry['input_max_error']=int(delta.max())
                check(entry,int(delta.max()) == 0,'Input passthrough mismatch (including frame edges)')
        values={}
        for p in params:
            if not p['enabled'] or p['kind']=='button': continue
            if p['kind']=='color': values[p['name']]=[255]+[rng.randrange(256) for _ in range(3)]
            elif p['kind']=='float': values[p['name']]=max(p['minimum'],min(p['maximum'],p['value']*rng.uniform(0,2)))
        values.update({'Character':rng.choice([1,2,3,4,5,6,14]),'Lens Model':rng.randint(1,4),
            'Light Source':[rng.uniform(-20,120),rng.uniform(-20,120),0],
            'Ray Grid':rng.choice([4,8,12]) if w*h>2_000_000 else rng.choice([4,8,16,31,32]),
            'Bright Sources':1,'Source Opacity':0,'Flare Opacity':100,
            'Human Eye Glare':rng.randint(0,1),'Ghost Blur Passes':rng.randint(1,3),
            'Bloom Passes':rng.randint(1,3),'Bloom Octaves':rng.randint(1,4),
            'Transfer Mode':4})
        gpu,_,eg=render(f'{case:02d}-random-gpu',w,h,values,black,depth=args.random_depth)
        cpu,_,ec=render(f'{case:02d}-random-cpu-optics',w,h,values,black,depth=args.random_depth,gpu=False)
        if gpu is not None and cpu is not None:
            delta=np.abs(gpu.astype(np.int32)-cpu.astype(np.int32))
            eg['cpu_max_error']=int(delta.max())
            eg['cpu_mean_error']=float(delta.mean())
            # Native output units: do not hide 16-bit error behind an 8-bit
            # equivalent allowance after the floating-point pipeline fix.
            check(eg,delta.max()<=2 and delta.mean()<=0.05,'CPU/GPU optical output divergence')
        if case%6==0:
            repeat,_,er=render(f'{case:02d}-repeat',w,h,values,black,depth=args.random_depth)
            if gpu is not None and repeat is not None:
                check(er,np.array_equal(gpu,repeat),'Nondeterministic repeat')
        zero,_,ez=render(f'{case:02d}-zero',w,h,
            {'Character':14,'Ghost Intensity':0,'Light Source':[rng.uniform(0,100),rng.uniform(0,100),0]},black)
        if zero is not None: check(ez,not np.any(zero[:,:,1:]),'Ghost-only zero intensity not black')
        print(f'{case+1}/{len(dimensions)} resolutions; {len(records)} renders; failures='+str(sum(r['status'] in ('failed','error') for r in records)),flush=True)
    save()
    bad=[r for r in records if r['status'] in ('failed','error')]
    print(json.dumps(bad,indent=2),flush=True)
    if bad: raise SystemExit(1)


if __name__=='__main__': main()
