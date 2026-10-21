"""Native 16-bit checkpoint preservation and sub-8bit glow regression."""
import argparse, copy, hashlib, json, os, struct, subprocess, zlib
from pathlib import Path
import numpy as np
from PIL import Image

def main():
    ap=argparse.ArgumentParser()
    ap.add_argument('--harness',required=True)
    ap.add_argument('--plugin',required=True)
    ap.add_argument('--baseline',required=True)
    ap.add_argument('--output',type=Path,required=True)
    args=ap.parse_args(); root=args.output.resolve(); root.mkdir(parents=True,exist_ok=False)
    env=dict(os.environ,ONMK_GPU_RESIDENT='1',ONMK_GPU_GHOSTS='1',ONMK_GPU_SOURCE_OPTICS='0',ONMK_GPU_BLOOM='0')
    def invoke(*argv):
        p=subprocess.run([args.harness,'--headless',*map(str,argv)],env=env,capture_output=True,text=True,encoding='utf-8',timeout=180)
        if p.returncode: raise RuntimeError(p.stderr)
        return json.loads(p.stdout)
    params=invoke('--inspect-experimental',args.plugin)
    w,h=257,129
    ramp=np.empty((h,w,4),dtype='>u2')
    values=(np.arange(w*h,dtype=np.uint32)%32769)*2
    values=np.minimum(values,65535).reshape(h,w)
    ramp[:,:,0]=values; ramp[:,:,1]=values; ramp[:,:,2]=values; ramp[:,:,3]=65535
    def chunk(name,data):
        return struct.pack('>I',len(data))+name+data+struct.pack('>I',zlib.crc32(name+data)&0xffffffff)
    payload=b''.join(b'\0'+row.tobytes() for row in ramp)
    png=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',w,h,16,6,0,0,0))+chunk(b'IDAT',zlib.compress(payload))+chunk(b'IEND',b'')
    (root/'ramp16.png').write_bytes(png)
    Image.new('RGBA',(w,h),(0,0,0,255)).save(root/'black.png')
    findings={}
    for tag,plugin in [('before',args.baseline),('after',args.plugin)]:
        for kind in ['passthrough','glow']:
            ps=copy.deepcopy(params)
            values={'Character':6,'Physical Renderer':0,'Human Eye Glare':0,'Global Brightness':100,
                'Source Opacity':100 if kind=='passthrough' else 0,'Flare Opacity':100,'Transfer Mode':4,
                'Hotspot Intensity':0,'Glow Intensity':0 if kind=='passthrough' else 1,
                'Glow Radius':80,'Glow Falloff':2,'Glow Color':[255,255,255,255],
                'Streak Intensity':0,'Stripe Intensity':0,'Ring Intensity':0,'Starburst Intensity':0,
                'Chromatic Bloom':0,'Atmosphere Amount':0,'Flicker Amount':0,'Bright Sources':1,
                'Light Source':[120/w*100,60/h*100,0]}
            for p in ps:
                if p['name'] in values:
                    key='color' if p['kind']=='color' else 'components' if p['kind']=='point' else 'value'
                    p[key]=values[p['name']]
            label=f'{tag}-{kind}'
            fixture=dict(schema='aexcompat.render_fixture',schema_version=1,primary_layer='ramp16.png' if kind=='passthrough' else 'black.png',
                parameters=ps,pixel_format='argb16',render_path='classic',premultiplication='straight',
                timing=dict(current_time=0,time_step=1,total_time=1,time_scale=1),final_artifact='raw',
                checkpoints=[dict(id='input',stage='classic-input'),dict(id='output',stage='classic-output')])
            path=root/f'{label}.json'; path.write_text(json.dumps(fixture,indent=2),encoding='utf-8')
            report=invoke('--render-fixture',plugin,path,root/label)
            (root/f'{label}.report.json').write_text(json.dumps(report,indent=2),encoding='utf-8')
            original=np.fromfile(root/label/'checkpoints'/'input'/'output.bin',dtype='<u2').reshape(h,w,4)
            actual=np.fromfile(root/label/'final'/'output.bin',dtype='<u2').reshape(h,w,4)
            result=dict(input_levels=int(np.unique(original[:,:,1]).size),output_levels=int(np.unique(actual[:,:,1]).size),
                max_input_error=int(np.abs(actual.astype(np.int32)-original.astype(np.int32)).max()),
                plugin_sha256=hashlib.sha256(Path(plugin).read_bytes()).hexdigest())
            findings[label]=result
            print(label,result,flush=True)
    (root/'summary.json').write_text(json.dumps(findings,indent=2),encoding='utf-8')
    assert findings['after-passthrough']['max_input_error']==0
    assert findings['after-glow']['output_levels']>256
    assert findings['before-passthrough']['max_input_error']>0
    assert findings['before-glow']['output_levels']<findings['after-glow']['output_levels']

if __name__=='__main__': main()
