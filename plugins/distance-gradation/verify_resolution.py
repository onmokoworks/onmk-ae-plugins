import subprocess,json,numpy as np,hashlib
from datetime import datetime
from pathlib import Path
from PIL import Image
r=Path.cwd();out=r/('dist/resolution-'+datetime.now().strftime('%Y%m%d-%H%M%S'));out.mkdir(exist_ok=True)
h='D:/AEXCompatWorktrees/render-current/broker/target/release/aexcompat-harness.exe'
rows=[]
for den in [1,2,4]:
 w,hgt=64//den,48//den
 src=np.zeros((hgt,w,4),dtype='uint8');src[12//den:36//den,16//den:48//den]=[20,130,230,255]
 inp=out/f'input-{den}.png';Image.fromarray(src).save(inp)
 req={'schema_version':1,'assignments':[{'slot':2,'value':2},{'slot':4,'value':16},{'slot':13,'value':0}], 'host_context':{'mask_scene':{'masks':[]},'spatial':{'downsample_x':{'numerator':1,'denominator':den},'downsample_y':{'numerator':1,'denominator':den},'pixel_aspect_ratio':{'numerator':1,'denominator':1},'full_resolution_width':64,'full_resolution_height':48}}}
 rp=out/f'request-{den}.json';rp.write_text(json.dumps(req))
 pair=[]
 for mode in ['classic','smart']:
  route='--render-experimental-'+('smart-request-16' if mode=='smart' else 'request-16')
  png=out/f'{den}-{mode}.png'
  p=subprocess.run([h,'--headless',route,str(r/'dist/ONMK_DistanceGradation.aex'),str(inp),str(png),str(rp)],capture_output=True,text=True,timeout=30)
  assert p.returncode==0,p.stderr
  (out/f'{den}-{mode}-host.json').write_text(p.stdout)
  data=np.frombuffer(png.with_suffix('.rgba16le').read_bytes(),dtype='<u2').reshape(hgt,w,4)/32768
  x,y=56//den,24//den
  expected=max(0,1-(x-(48//den-1))*den/16)
  assert abs(data[y,x,3]-expected)<=1/32768,(den,mode,data[y,x,3],expected)
  assert np.max(abs(data[24//den,32//den]-np.array([20,130,230,255])/255))<=1/32768
  pair.append(data);rows.append({'denominator':den,'route':mode,'actual_outside_alpha':data[y,x,3],'expected':expected,'passed':True})
 assert np.array_equal(*pair)
(out/'summary.json').write_text(json.dumps({'artifact_sha256':hashlib.sha256((r/'dist/ONMK_DistanceGradation.aex').read_bytes()).hexdigest(),'cases':rows},indent=2))
print(rows)
