"""Headless AEXCompat parameter sensitivity audit; no AEXCompat source edits.

Observed image changes are evidence of sensitivity, not proof of correct meaning.
No-change results remain explicit; GUI buttons are never invoked by this test.
"""
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import struct
import subprocess
from PIL import Image


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--harness', type=Path, required=True)
    parser.add_argument('--plugin', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--smoke', action='store_true')
    parser.add_argument('--width', type=int, default=320)
    parser.add_argument('--height', type=int, default=180)
    parser.add_argument('--cpu-optics', action='store_true', help='CPU optical layers, GPU final composition')
    parser.add_argument('--resume', action='store_true', help='Reuse completed cases; only with the same plugin binary and fixtures')
    args = parser.parse_args()
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=args.resume)
    assert 320 <= args.width <= 1920 and 180 <= args.height <= 1080
    identity = dict(plugin_sha256=hashlib.sha256(args.plugin.read_bytes()).hexdigest(),
                    harness_sha256=hashlib.sha256(args.harness.read_bytes()).hexdigest(),
                    width=args.width,height=args.height,cpu_optics=args.cpu_optics)
    identity_path = root/'identity.json'
    if args.resume:
        assert identity_path.exists(), 'Resume requires identity.json from this runner'
        assert json.loads(identity_path.read_text(encoding='utf-8')) == identity, 'Binary changed; use a fresh output directory'
    else:
        identity_path.write_text(json.dumps(identity,indent=2),encoding='utf-8')
    env = dict(os.environ, ONMK_GPU_RESIDENT='0' if args.cpu_optics else '1', ONMK_GPU_GHOSTS='0' if args.cpu_optics else '1',
               ONMK_GPU_SOURCE_OPTICS='0', ONMK_GPU_BLOOM='0')

    def invoke(*argv):
        p = subprocess.run([str(args.harness), '--headless', *map(str, argv)],
                           capture_output=True, text=True, encoding='utf-8', env=env)
        if p.returncode:
            raise RuntimeError(f'{p.returncode}: {p.stdout}\n{p.stderr}')
        return json.loads(p.stdout)

    params = invoke('--inspect-experimental', args.plugin)
    (root/'discovery.json').write_text(json.dumps(params, indent=2), encoding='utf-8')
    by_name = {p['name']: p for p in params}
    assert len(by_name) == len(params)
    w,h = args.width,args.height
    Image.new('RGBA', (w,h), (0,0,0,255)).save(root/'black.png')
    image = Image.new('RGBA', (w,h))
    image.putdata([(x%95, y%80, (x+y)%70, 255) for y in range(h) for x in range(w)])
    for x,y,v in [(270,35,255),(240,120,200),(20,140,140)]:
        for dy in range(5):
            for dx in range(5): image.putpixel((x+dx,y+dy),(v,v,v,255))
    image.save(root/'pattern.png')

    def configured(values):
        result = copy.deepcopy(params)
        for p in result:
            if p['name'] not in values: continue
            value = values[p['name']]
            if p['kind'] == 'color': p['color'] = value
            elif p['kind'] == 'point': p['components'] = value
            else: p['value'] = value
        return result

    reports = []
    def render(label, values, source='pattern.png', depth='argb8'):
        document = dict(schema='aexcompat.render_fixture', schema_version=1,
            primary_layer=source, parameters=configured(values), pixel_format=depth,
            render_path='classic', premultiplication='straight',
            timing=dict(current_time=0,time_step=1,total_time=1,time_scale=1),
            final_artifact='raw',checkpoints=[dict(id='output',stage='classic-output')])
        fixture = root/(label+'.json')
        if args.resume and fixture.exists():
            assert json.loads(fixture.read_text(encoding='utf-8')) == document, f'Fixture changed: {label}'
        else:
            fixture.write_text(json.dumps(document,indent=2),encoding='utf-8')
        output = root/'renders'/label
        output.parent.mkdir(exist_ok=True)
        report_path = root/(label+'.report.json')
        if args.resume and report_path.exists():
            report = json.loads(report_path.read_text(encoding='utf-8'))
        else:
            report = invoke('--render-fixture', args.plugin, fixture, output)
        (root/(label+'.report.json')).write_text(json.dumps(report,indent=2),encoding='utf-8')
        paths = [p for p in output.rglob('output.bin') if 'checkpoints' not in p.relative_to(output).parts]
        assert len(paths)==1, (label, paths, report)
        raw = paths[0].read_bytes()
        expected_size = w*h*{'argb8':4,'argb16':8,'argb32f':16}[depth]
        assert len(raw)==expected_size, (label,len(raw))
        if depth=='argb32f':
            import math
            assert all(math.isfinite(p[0]) for p in struct.iter_unpack('<f',raw)), label
        reports.append(dict(case=label,sha256=hashlib.sha256(raw).hexdigest(),bytes=len(raw)))
        if len(reports)%10==0: print(f'rendered {len(reports)} cases: {label}',flush=True)
        return raw

    base = {'Character':6,'Light Source':[35,40,0], 'Bright Sources':3,
            'Human Eye Glare':1,'Squint / Eyelash':60,'Atmosphere Amount':20,
            'Flicker Amount':30,'Flicker Phase':70,'Eye Motion Phase':30,
            'Ray Grid':20,'Source Threshold':70,'Source Downsample':4}
    baseline = render('baseline-custom',base)
    solo = {'Character':14,'Light Source':[35,40,0]}
    solo_raw = render('cooke-only',solo,'black.png')
    manual = dict(solo, Character=6, **{'Physical Renderer':1,'Lens Model':1,
        'Bright Sources':1,'Source Opacity':0,'Hotspot Intensity':0,'Glow Intensity':0,
        'Streak Intensity':0,'Stripe Intensity':0,'Ring Intensity':0,'Starburst Intensity':0,
        'Chromatic Bloom':0,'Human Eye Glare':0,'Atmosphere Amount':0,'Chromatic Aberration':0,
        'Optical Gain':by_name['Optical Gain']['value']*40})
    assert solo_raw == render('cooke-manual-reference',manual,'black.png')
    assert any(solo_raw[i] for i in range(len(solo_raw)) if i%4), 'empty Cooke preset'
    rgba = bytes(v for a,r,g,b in struct.iter_unpack('4B', solo_raw) for v in (r,g,b,255))
    Image.frombytes('RGBA',(w,h),rgba).save(root/'cooke-only.png')
    isolated = {}
    intensity_images = [render(f'cooke-intensity-{v}',dict(solo, **{'Ghost Intensity':v}),'black.png') for v in (0,100,200)]
    isolated['ghost_intensity_changes_isolated_ghosts'] = len(set(intensity_images)) > 1
    assert len(set(intensity_images)) > 1, 'Ghost Intensity has no effect'
    assert not any(v for i,v in enumerate(intensity_images[0]) if i%4), 'Zero ghost intensity must be black'
    isolated['zero_ghost_intensity_is_black'] = True
    assert by_name['Ray Grid']['maximum'] == 128
    grid_images = [render(f'cooke-grid-{v}',dict(solo, **{'Ray Grid':v}),'black.png') for v in (8,20,32)]
    isolated['ray_grid_changes'] = len(set(grid_images)) > 1
    assert isolated['ray_grid_changes']
    for name in ('Ghost Count','Ghost Spread','Ghost Size','Ghost Color','Ghost Chromatic'):
        assert not by_name[name]['enabled'], f'{name} must be visibly disabled'
    results = []
    depth_results = {}
    if not args.smoke:
        for p in params:
            name,slot = p['name'],p['slot']
            if p['kind']=='button':
                results.append(dict(slot=slot,name=name,status='gui_not_executed'))
                continue
            context = dict(base)
            if name in ('Ghost Complexity','Diffraction','Flare Angle'):
                context['Character']=2
            if name.startswith('Edge Trigger'):
                context['Light Source']=[2,50,0]
            if p['kind']=='point': values=[[25,30,0],[75,65,0]]
            elif p['kind']=='color': values=[[255,255,20,20],[255,20,255,20]]
            elif p['choices']: values=list(range(int(p['minimum']),int(p['maximum'])+1))
            elif p['kind']=='integer': values=[p['minimum'],p['maximum']]
            else:
                low = p['minimum']
                high = min(p['maximum'], max(p['value']*1.7, 20))
                special={'Ray Grid':(8,28),'Flare Angle':(0,45), 'Streak Rotation':(0,45),
                    'Eye Motion Phase':(0,90),'Flicker Phase':(0,120),
                    'Stripe Length':(40,600),'Optical Gain':(0,3000),
                    'Ghost Count':(1,12),'Ghost Size':(5,60),'Global Scale':(50,150)}
                values=list(special.get(name,(low,high)))
            images = [render(f'param-{slot:02d}-{i}',dict(context,**{name:value})) for i,value in enumerate(values)]
            changed = max(sum(a!=b for a,b in zip(images[0],image)) for image in images)
            results.append(dict(slot=slot,name=name,status='changed' if changed else 'no_difference',
                changed_channels=changed,values=values,character=context['Character']))
        for depth in ['argb16','argb32f']:
            try:
                render('cooke-'+depth,solo,'black.png',depth)
                depth_results[depth] = 'rendered'
            except Exception as error:
                depth_results[depth] = str(error)
    summary=dict(parameters=results,renders=reports,preset_matches_manual=True,isolated=isolated,depths=depth_results,
                 plugin_sha256=hashlib.sha256(args.plugin.read_bytes()).hexdigest(),
                 scope=f'Single-variable sensitivity at {w}x{h}. Not a correctness proof or exhaustive combinations.',
                 cpu_optics=args.cpu_optics)
    (root/'summary.json').write_text(json.dumps(summary,indent=2),encoding='utf-8')
    print(json.dumps([p for p in results if p['status']!='changed'],indent=2),flush=True)
    print(f'Completed: {len(reports)} renders, {len(results)} parameters',flush=True)

if __name__=='__main__': main()
