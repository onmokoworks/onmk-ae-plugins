"""Bounded native AEX checks. Does not launch or operate Adobe After Effects."""
import hashlib
from datetime import datetime
import json
import subprocess
from pathlib import Path
import numpy as np
from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent
OUT = ROOT / 'dist' / ('validation-' + datetime.now().strftime('%Y%m%d-%H%M%S'))
HOST = Path('D:/AEXCompatWorktrees/render-current/broker/target/release/aexcompat-harness.exe')
AEX = ROOT / 'dist/ONMK_DistanceGradation.aex'
OUT.mkdir(parents=True, exist_ok=True)
records = []
processes = []

def invoke(args):
    with subprocess.Popen([str(HOST), '--headless', *map(str, args)], cwd=ROOT,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True) as p:
        try:
            stdout, stderr = p.communicate(timeout=45)
        except subprocess.TimeoutExpired:
            subprocess.run(['taskkill', '/PID', str(p.pid), '/T', '/F'], capture_output=True)
            p.wait(timeout=10)
            raise
        processes.append({'pid': p.pid, 'exit_code': p.returncode, 'exited': p.poll() is not None})
        assert p.returncode == 0, stderr
        return json.loads(stdout)

inspection = invoke(['--inspect-experimental', AEX])
(OUT/'inspection.json').write_text(json.dumps(inspection, indent=2))
assert len(inspection) == 15
assert {p['slot']: p['value'] for p in inspection}[13] == 1
assert {p['slot']: p['value'] for p in inspection}[14] == 1

w,h=161,113
yy,xx=np.indices((h,w))
coverage=np.clip(29.3-np.hypot(xx-79.2,yy-55.7)+0.5,0,1)
src=np.zeros((h,w,4),dtype=np.uint8)
src[...,:3]=[35,130,230]
src[...,3]=np.rint(coverage*255).astype(np.uint8)
# Low-alpha sentinel strips away from the main contour.
src[4,4:9,3]=[0,1,32,128,255]
Image.fromarray(src).save(OUT/'input.png')
Image.fromarray(np.zeros_like(src)).save(OUT/'empty.png')

def run(name,depth,mode,assignments,input_name='input'):
    request={'schema_version':1,'assignments':[{'slot':k,**({'color':v} if isinstance(v,list) else {'value':v})} for k,v in assignments.items()]}
    rp=OUT/(name+'-request.json');rp.write_text(json.dumps(request,indent=2))
    png=OUT/(name+'.png')
    route='--render-experimental-'+('smart-request' if mode=='smart' else 'request')
    if depth==16:route+='-16'
    if depth==32:route+='-32-cpu' if mode=='smart' else '-32'
    report=invoke([route,AEX,OUT/(input_name+'.png'),png,rp])
    (OUT/(name+'-host.json')).write_text(json.dumps(report,indent=2))
    if depth==8: data=np.array(Image.open(png).convert('RGBA'),dtype=float)/255
    else:
        raw=png.with_suffix('.rgba16le' if depth==16 else '.rgba32f-le')
        data=np.frombuffer(raw.read_bytes(),dtype='<u2' if depth==16 else '<f4').reshape(h,w,4).astype(float)/(32768 if depth==16 else 1)
    assert np.isfinite(data).all()
    records.append({'case':name,'depth':depth,'route':mode})
    return data

base={2:3,3:10,4:13,7:[255,255,221,51]}
frames={}
for depth in [8,16,32]:
    tol={8:1/255,16:1/32768,32:1e-6}[depth]+1e-7
    pair=[]
    for mode in ['classic','smart']:
        name=f'composite-{depth}-{mode}'
        data=run(name,depth,mode,base);pair.append(data)
        assert np.max(abs(data[56,79]-src[56,79]/255))<=tol, 'original center must remain'
        assert data[0,-1,3]==0, 'lower layers visible outside gradient'
        assert np.all(data[...,3]+tol>=src[...,3]/255), 'original alpha must not be removed'
        isolated=run(f'isolated-{depth}-{mode}',depth,mode,{**base,7:[255,255,255,255],14:0})
        mask=(isolated[...,3]>0)&(isolated[...,3]<1)
        assert mask.any()
        assert np.max(abs(isolated[...,:3][mask]-1))<=tol, 'straight white'
        bad=isolated[...,:3][mask]*isolated[...,3:4][mask]
        assert np.max(abs(bad-1))>0.1, 'mutation must be rejected'
        identity=run(f'identity-{depth}-{mode}',depth,mode,{15:0})
        assert np.max(abs(identity-src/255))<=tol, 'opacity zero identity'
        empty=run(f'empty-{depth}-{mode}',depth,mode,{**base,1:1,6:1},'empty')
        assert np.max(abs(empty))==0
    assert np.array_equal(pair[0],pair[1]), 'classic and smart differ'
    frames[depth]=pair[1]

smooth=run('aa-on',16,'smart',{**base,14:0})
hard=run('aa-off',16,'smart',{**base,14:0,13:0})
assert np.max(abs(smooth-hard))>0.01
for blur in [2,3]:
    result=run(f'blur-{blur}',16,'smart',{**base,11:blur,12:2})
    assert result[0,-1,3]==0
for side in [1,2]:
    result=run(f'side-{side}',16,'smart',{**base,2:side})
    assert result[0,-1,3]==0

# Representative native-host images over a checkerboard = a lower layer.
checker=np.where(((xx//10+yy//10)%2)[...,None],np.array([.16,.2,.25]),np.array([.33,.37,.42]))
def composite(data):
    return Image.fromarray(np.rint(np.clip(data[...,:3]*data[...,3:4]+checker*(1-data[...,3:4]),0,1)*255).astype('uint8'))
panels=[('Original',src/255),('Composite Original ON',frames[16]),('AA ON (effect only)',smooth),('AA OFF (effect only)',hard)]
preview=Image.new('RGB',(w*2*3,(h+22)*2*3),(24,27,32))
for n,(title,data) in enumerate(panels):
    panel=Image.new('RGB',(w,h+22),(24,27,32));panel.paste(composite(data),(0,22))
    ImageDraw.Draw(panel).text((4,5),title,fill='white')
    preview.paste(panel.resize((w*3,(h+22)*3)),((n%2)*w*3,(n//2)*(h+22)*3))
preview.save(OUT/'preview.png')
summary={'artifact_sha256':hashlib.sha256(AEX.read_bytes()).hexdigest(),
         'host_sha256':hashlib.sha256(HOST.read_bytes()).hexdigest(),
         'cases':records,'processes':processes,'all_passed':True,'ae_tested':False}
(OUT/'summary.json').write_text(json.dumps(summary,indent=2))
print(json.dumps({'cases':len(records),'all_passed':True,'all_harness_processes_exited':all(p['exited'] for p in processes)}))
