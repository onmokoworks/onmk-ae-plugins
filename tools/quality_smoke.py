"""Headless, contract-specific first-wave checks. Never deploys or opens GUI.

Independent worker processes do NOT establish same-instance resize/cache safety.
8-bit PNG input likewise does not establish complete 16-bit input precision.
"""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import numpy as np
from PIL import Image


def classify_render_error(oracle, message):
    if (oracle=='expected-model-error' and 'malformed_error_response' in message
            and 'error response carried output/resize fields' in message):
        return 'blocked_host_error_transport'
    return 'failed_unclassified'


def artifact_hash(path):
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(chunk)
    return digest.hexdigest()


def verify_fallback_pixels(input_metadata, output_metadata, input_bytes, output_bytes):
    for key in ['width', 'height', 'pixel_format', 'channel_order', 'origin', 'rowbytes']:
        if key not in input_metadata or key not in output_metadata or input_metadata[key] != output_metadata[key]:
            raise AssertionError(f'fallback layout mismatch: {key}')
    if not input_bytes or input_bytes != output_bytes:
        raise AssertionError('fallback pixels differ from input checkpoint')
    return 'blocked_error_notification_unverified'


def verify_artifact(path, expected):
    if artifact_hash(path) != expected:
        raise RuntimeError(f'Test artifact changed during run: {path}')


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--harness',type=Path,required=True)
    parser.add_argument('--plugin',type=Path,required=True)
    parser.add_argument('--profile',choices=['luma-normal','luma-grain','minimax-map','uv-project','mask-offset','mask-transform','scatter-map','prism-warp','adaptive-filter','median-pro','edge-smith','rio-grade','path-array','shape-plus','starglow','onmk-glow','tui-image','celglow','celglow2','depth-onnx-missing-model','onmk-film','particlekit','lattice-lab','refraction-dispersion','onmk-flare','frame-slice-static'],required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--only',help='Run only render labels with this prefix for focused reproduction')
    args=parser.parse_args()
    args.plugin=args.plugin.resolve()
    args.harness=args.harness.resolve()
    plugin_hash = artifact_hash(args.plugin)
    harness_hash = artifact_hash(args.harness)
    runner_hash = artifact_hash(Path(__file__))
    root=args.output.resolve(); root.mkdir(parents=True,exist_ok=False)
    results=[]
    def invoke(*argv):
        verify_artifact(args.plugin, plugin_hash)
        verify_artifact(args.harness, harness_hash)
        p=subprocess.run([str(args.harness),'--headless',*map(str,argv)],capture_output=True,text=True,encoding='utf-8',timeout=120)
        verify_artifact(args.plugin, plugin_hash)
        verify_artifact(args.harness, harness_hash)
        if p.returncode: raise RuntimeError(p.stdout+'\n'+p.stderr)
        return json.loads(p.stdout)
    params=invoke('--inspect-experimental',args.plugin)
    (root/'parameters.json').write_text(json.dumps(params,indent=2),encoding='utf-8')
    def save():
        (root/'summary.json').write_text(json.dumps(dict(profile=args.profile,
            plugin_sha256=plugin_hash,harness_sha256=harness_hash,runner_sha256=runner_hash,results=results,
            limitations=['No real AE GUI/MFR certification','Independent workers, not same-instance resize',
                          '8-bit PNG inputs do not establish full 16-bit input precision','No GPU execution proof']),indent=2),encoding='utf-8')
    default_path='smart' if args.profile in ['mask-offset','path-array','celglow','celglow2','depth-onnx-missing-model','onmk-film','particlekit','frame-slice-static'] else 'classic'
    def render(label,w,h,kind='pattern',values=None,depth='argb8',path=None,oracle=None,time=0,layers=None):
        path=path or default_path
        if args.only and not label.startswith(args.only): return None
        record=dict(case=label,width=w,height=h,depth=depth,path=path,status='running')
        results.append(record); save()
        try:
            pixels=np.zeros((h,w,4),dtype=np.uint8)
            pixels[:,:,3]=255
            if kind=='pattern':
                yy,xx=np.indices((h,w)); pixels[:,:,0]=(xx*17+yy*3)%256
                pixels[:,:,1]=(xx*7+yy*19)%256; pixels[:,:,2]=(xx*11+yy*5)%256
            elif kind=='transparent': pixels[:]=0
            elif kind=='alpha-stripes':
                pixels[:,:,:3]=[64,192,0]
                pixels[:,:,3]=np.array([0,64,128,255],dtype=np.uint8)[np.arange(w)%4]
            source=root/f'{label}-input.png'; Image.fromarray(pixels).save(source)
            configured=copy.deepcopy(params)
            for name,layer_path in (layers or {}).items():
                found=[p for p in configured if p['name']==name and p['kind']=='layer']
                assert len(found)==1, f'Unknown layer parameter {name}'
                found[0]['layer_path']=layer_path.resolve().relative_to(root).as_posix()
            values=values or {}
            for name,value in values.items():
                found=[p for p in configured if p['name']==name]
                assert len(found)==1, f'Unknown parameter {name}'
                found[0]['value']=value
            fixture=root/f'{label}.json'
            fixture.write_text(json.dumps(dict(schema='aexcompat.render_fixture',schema_version=1,
                primary_layer=source.name,parameters=configured,pixel_format=depth,render_path=path,
                premultiplication='premultiplied' if kind=='alpha-stripes' else 'straight',timing=dict(current_time=time,time_step=1,total_time=10,time_scale=1),
                final_artifact='raw',checkpoints=([dict(id='input',stage=f'{path}-input')] if kind=='alpha-stripes' or oracle in ['expected-model-error','glow-threshold'] else [])+[dict(id='output',stage=f'{path}-output')]),indent=2),encoding='utf-8')
            dest=root/'renders'/label; dest.parent.mkdir(exist_ok=True)
            report=invoke('--render-fixture',args.plugin,fixture,dest)
            (root/f'{label}.report.json').write_text(json.dumps(report,indent=2),encoding='utf-8')
            raw=(dest/'final'/'output.bin').read_bytes()
            dtype={'argb8':'u1','argb16':'<u2','argb32f':'<f4'}[depth]
            artifact=report['final_artifact']
            ow,oh=artifact['width'],artifact['height']
            record.update(output_width=ow,output_height=oh,output_origin=artifact['origin'])
            assert artifact['pixel_format']==depth, 'unexpected output pixel format'
            if args.profile!='particlekit':
                assert (ow,oh)==(w,h), 'unexpected output extent'
            else:
                origin=artifact['origin']
                assert origin['x']<=0 and origin['y']<=0, 'expanded output excludes input origin'
                assert origin['x']+ow>=w and origin['y']+oh>=h, 'expanded output excludes input bounds'
            out=np.frombuffer(raw,dtype=dtype).reshape(oh,ow,4)
            assert np.isfinite(out).all(), 'output contains non-finite channel values'
            if oracle=='glow-threshold':
                original=np.frombuffer((dest/'checkpoints'/'input'/'output.bin').read_bytes(),dtype=dtype).reshape(h,w,4)
                scale={'argb8':255,'argb16':32768,'argb32f':1}[depth]
                normalized=original.astype(np.float64)/scale
                rgb=normalized[:,:,1:]
                weight=values.get('Glow From Alpha',0)
                signal=(rgb@np.array([0.2126,0.7152,0.0722]))*(1-weight)+normalized[:,:,0]*weight
                thresholds=np.array([values['Threshold']+values.get('Threshold Add '+c,0) for c in ['Red','Green','Blue']])
                t=np.clip((signal[:,:,None]-(thresholds-0.05))/0.1,0,1)
                expected=rgb*t*t*(3-2*t)*scale
                if depth=='argb8': expected=np.floor(np.clip(expected,0,255))
                elif depth=='argb16': expected=np.floor(np.clip(expected,0,32768)+0.5)
                tolerance=2e-6 if depth=='argb32f' else 1
                error=float(np.max(np.abs(out[:,:,1:].astype(np.float64)-expected)))
                record['oracle_max_error']=error
                assert error<=tolerance, f'threshold preview differs: {error}'
                assert np.array_equal(out[:,:,0],original[:,:,0]), 'threshold alpha changed'
            if oracle=='expected-model-error':
                original=(dest/'checkpoints'/'input'/'output.bin').read_bytes()
                status=verify_fallback_pixels(report['checkpoints']['input'],artifact,original,raw)
                record.update(status=status, fallback_pixels_identical=True,
                    sha256=hashlib.sha256(raw).hexdigest(),
                    error='Input passthrough verified; error notification and model-failure branch execution are not observable. Not an inference or negative-path pass.')
                save()
                return None
            scale={'argb8':255,'argb16':32768,'argb32f':1}[depth]
            if oracle=='flat-normal':
                assert np.all(out==[255,128,128,255]), 'flat normal coverage/values differ'
            elif oracle=='alpha-zero': assert not out[:,:,0].any(), 'transparent alpha lost'
            elif oracle=='scatter-fractional-map':
                original=np.frombuffer((root/'renders'/f'fractional-source-{depth}-{path}'/'final'/'output.bin').read_bytes(),dtype=dtype).reshape(h,w,4)
                yy,xx=np.indices((h,w))
                gray=np.where((xx*3//w+yy*7//h)%2,192,64)
                if depth=='argb16': level=np.floor(gray*32768/255+0.5)/32768
                elif depth=='argb32f': level=(gray.astype(np.float32)/np.float32(255)).astype(np.float64)
                else: level=gray/255
                if values['Invert Map']: level=1-level
                amount=np.float32(values['Scatter Amount'])*level.astype(np.float32)
                def offsets(channel):
                    # Explicit 32-bit wrapping and Rust's round-away-from-zero.
                    v=(xx.astype(np.uint64)*374761393+yy.astype(np.uint64)*668265263+values['Random Seed']*2246822519+channel*3266489917)&0xffffffff
                    v=v^(v>>13); v=(v*274177)&0xffffffff
                    v=v^(v>>16); v=(v*1900813)&0xffffffff; v=v^(v>>13)
                    noise=(v.astype(np.float32)/np.float32(4294967295))*np.float32(2)-np.float32(1)
                    offset=noise*amount
                    return (np.sign(offset)*np.floor(np.abs(offset)+np.float32(0.5))).astype(np.int64)
                sx=xx+(offsets(0) if values['Direction'] in [1,3] else 0)
                sy=yy+(offsets(1) if values['Direction'] in [2,3] else 0)
                expected=original[np.clip(sy,0,h-1),np.clip(sx,0,w-1)].copy()
                if not values['Repeat Edge Pixels']: expected[(sx<0)|(sx>=w)|(sy<0)|(sy>=h)]=0
                assert np.any(expected!=original), 'fractional fixture is not discriminating'
                mix=np.float32(values.get('Mix with Original',100))/np.float32(100)
                if mix<1:
                    mixed=original.astype(np.float32)*(np.float32(1)-mix)+expected.astype(np.float32)*mix
                    expected=mixed if depth=='argb32f' else np.clip(mixed,0,scale).astype(dtype)
                assert np.array_equal(out,expected), f'fractional scatter oracle mismatch: {np.count_nonzero(out!=expected)} channels'
            elif oracle=='scatter-spatial-map':
                def reference(label):
                    return np.frombuffer((root/'renders'/f'{label}-{depth}-{path}'/'final'/'output.bin').read_bytes(),dtype=dtype).reshape(h,w,4)
                original=reference('spatial-source')
                scattered=reference('spatial-base')
                my,mx=np.indices((h,w))
                active=((mx*3//w+my*7//h)%2)==1
                if values.get('Invert Map',0): active=~active
                assert np.any(scattered[active]!=original[active]), 'fixture has no discriminating active pixels'
                expected=np.where(active[:,:,None],scattered,original)
                assert np.array_equal(out,expected), 'spatial map selection differs from independent source/scatter references'
            elif oracle=='lattice-native16':
                expanded8=(np.arange(256,dtype=np.uint32)*32768+127)//255
                native_count=int(np.count_nonzero(~np.isin(out,expanded8)))
                record['native16_values_outside_expanded8']=native_count
                assert native_count>0, 'Lattice16 output still restricted to expanded8 levels'
            elif oracle=='grain-premultiplied':
                assert np.array_equal(out[:,:,0],pixels[:,:,3]), 'grain changed input alpha'
                assert np.all(out[:,:,1:]<=out[:,:,0,None]), 'grain RGB exceeds premultiplied alpha'
            elif oracle=='alpha-opaque': assert np.all(out[:,:,0]==scale), 'generated frame alpha is not opaque'
            elif oracle=='flat-normal-alpha':
                alpha=pixels[:,:,3].astype(np.uint32) if values.get('Alpha')==2 else np.full((h,w),255,dtype=np.uint32)
                expected=np.stack([alpha,(128*alpha+127)//255,(128*alpha+127)//255,alpha],axis=2)
                assert np.array_equal(out,expected), 'flat normal not correctly premultiplied by selected alpha'
            elif oracle=='grain-seed-time':
                # Uniform luminance weights isolate the spatial/time hash and encoding.
                yy,xx=np.indices((h,w),dtype=np.uint64)
                seed=values['Seed'] ^ ((time*0x9e3779b9)&0xffffffff)
                base=((xx//values['Size'])*0x1f123bb5)^((yy//values['Size'])*0x5f356495)^seed
                expected=np.empty_like(out); expected[:,:,0]=255
                for channel in range(3):
                    hashed=(base^(0 if values['Monochrome'] else channel+1))&0xffffffff
                    hashed^=hashed>>16; hashed=(hashed*0x7feb352d)&0xffffffff
                    hashed^=hashed>>15; hashed=(hashed*0x846ca68b)&0xffffffff
                    hashed^=hashed>>16
                    noise=(hashed.astype(np.float32)/np.float32(0xffffffff))*np.float32(2)-np.float32(1)
                    encoded=np.clip(pixels[:,:,channel].astype(np.float32)+noise*np.float32(64),0,255)
                    expected[:,:,channel+1]=np.floor(encoded+np.float32(.5)).astype(np.uint8)
                error=np.abs(out.astype(np.int16)-expected.astype(np.int16)).max()
                assert error<=1, f'seed/time grain differs from hash reference by {error} codes'
                record['max_code_error']=int(error)
            elif oracle=='normal-field':
                height=pixels[:,:,:3].astype(float)@np.array([0.2126,0.7152,0.0722])/255
                radius=values['Pre-Blur']
                if radius:
                    horizontal=np.empty_like(height)
                    blurred=np.empty_like(height)
                    for x in range(w): horizontal[:,x]=height[:,max(0,x-radius):min(w,x+radius+1)].mean(axis=1)
                    for y in range(h): blurred[y,:]=horizontal[max(0,y-radius):min(h,y+radius+1),:].mean(axis=0)
                    height=blurred
                distance=values['Detail Scale']
                xs=np.arange(w); ys=np.arange(h)
                nx=-(height[:,np.minimum(xs+distance,w-1)]-height[:,np.maximum(xs-distance,0)])*values['Strength']
                ny=-(height[np.minimum(ys+distance,h-1),:]-height[np.maximum(ys-distance,0),:])*values['Strength']
                if values['Invert X']: nx=-nx
                if values['Invert Y']: ny=-ny
                norm=np.sqrt(nx*nx+ny*ny+1)
                rgb=np.floor((np.stack([nx/norm,ny/norm,1/norm],axis=2)*0.5+0.5)*255+0.5)
                assert np.all(out[:,:,0]==255), 'normal output alpha differs'
                error=np.abs(out[:,:,1:].astype(float)-rgb).max()
                assert error<=1, f'normal-field encoding differs by {error} codes'
                record['max_code_error']=float(error)
            elif oracle=='rio-premultiplied':
                expected_alpha=pixels[:,:,3]
                assert np.array_equal(out[:,:,0],expected_alpha), 'grade changed source alpha'
                assert np.all(out[:,:,1:]<=out[:,:,:1]), 'grade RGB exceeds premultiplied alpha'
                if values.get('Strength')==0:
                    source=np.frombuffer((dest/'checkpoints'/'input'/'output.bin').read_bytes(),dtype=dtype).reshape(h,w,4)
                    assert np.array_equal(out,source), 'zero-strength premultiplied input changed'
            elif oracle=='prism-ramp-lens':
                source=pixels[:,:,[3,0,1,2]].astype(float)/255
                if depth=='argb16': source=np.floor(source*32768+0.5)/32768
                yy,xx=np.indices((h,w))
                vertical=values['Direction']==90
                sx=xx+(0 if vertical else values['Strength (px)'])
                sy=yy+(values['Strength (px)'] if vertical else 0)
                def reflected(coordinates,length):
                    limit=length-1
                    wrapped=np.abs(coordinates)%(2*limit)
                    return np.minimum(wrapped,2*limit-wrapped)
                sx=reflected(sx,w); sy=reflected(sy,h)
                x0=sx.astype(int); y0=sy.astype(int)
                x1=np.minimum(x0+1,w-1); y1=np.minimum(y0+1,h-1)
                dx=(sx-x0)[:,:,None]; dy=(sy-y0)[:,:,None]
                expected=(source[y0,x0]*(1-dx)*(1-dy)+source[y0,x1]*dx*(1-dy)
                          +source[y1,x0]*(1-dx)*dy+source[y1,x1]*dx*dy)
                expected[:,:,0]=source[:,:,0]
                error=float(np.max(np.abs(out.astype(float)/scale-expected)))
                record['analytic_max_error']=error
                assert error<=(2e-6 if depth=='argb32f' else 1/scale), f'analytic ramp displacement mismatch: {error}'
            elif oracle=='prism-spatial-matte':
                reference=np.frombuffer((root/'renders'/f'aux-prism-base-{depth}-{path}'/'final'/'output.bin').read_bytes(),dtype=dtype).reshape(h,w,4)
                yy,xx=np.indices((h,w))
                active=((xx//3+yy//5)%2)!=0
                if values['Invert Matte']: active=~active
                source=pixels[:,:,[3,0,1,2]].astype(float)/255
                assert np.max(np.abs(reference.astype(float)[active]/scale-source[active]))>0.01, 'active matte region is not discriminating'
                assert np.array_equal(out[active],reference[active]), 'white matte region differs from full effect'
                tolerance=1e-6 if depth=='argb32f' else 1/scale
                assert np.max(np.abs(out.astype(float)[~active]/scale-source[~active]))<=tolerance, 'black matte region is not passthrough'
            elif oracle in ['passthrough','prism-flat-map','prism-active']:
                expected=pixels[:,:,[3,0,1,2]].astype(float)/255
                tolerance=1e-6 if depth=='argb32f' else 1/scale
                difference=np.max(np.abs(out.astype(float)/scale-expected))
                if oracle=='prism-active':
                    assert difference>0.01, 'Prism reference must visibly differ from input'
                else:
                    assert difference<=tolerance, 'passthrough mismatch'
                if oracle=='prism-flat-map':
                    reference=np.frombuffer((root/'renders'/f'aux-prism-base-{depth}-{path}'/'final'/'output.bin').read_bytes(),dtype=dtype).reshape(h,w,4)
                    assert np.max(np.abs(reference.astype(float)/scale-expected))>0.01, 'ignored-map fixture is not discriminating'
            elif oracle in ['constant-texture','masked-constant-texture']:
                expected=np.array([255,37,129,211],dtype=float)/255
                if oracle=='masked-constant-texture':
                    alpha=pixels[:,:,3].astype(float)/255
                    if depth=='argb16':
                        alpha=np.floor(alpha*32768+0.5)/32768
                        expected=np.floor(expected*32768+0.5)/32768
                    if not values.get('Use UV Alpha as Mask',1): alpha=np.ones_like(alpha)
                    expected=expected[None,None,:]*alpha[:,:,None]
                tolerance=1e-6 if depth=='argb32f' else 1/scale
                assert np.max(np.abs(out.astype(float)/scale-expected))<=tolerance, 'auxiliary texture was not projected correctly'
            elif oracle=='texture-role':
                uv=np.array(Image.open(layers['Other Layer (Texture / UV Map)']).convert('RGBA'))
                assert np.all(uv==uv[0,0]), 'texture-role oracle requires constant UV'
                u,v=uv[0,0,:2].astype(float)/255
                if values.get('V Origin',2)==2: v=1-v
                tx=int(np.clip(np.floor(u*w),0,w-1)); ty=int(np.clip(np.floor(v*h),0,h-1))
                expected=pixels[ty,tx,[3,0,1,2]].astype(float)/255
                if depth=='argb16': expected=np.floor(expected*32768+0.5)/32768
                expected*=values.get('Opacity',100)/100
                tolerance=1e-6 if depth=='argb32f' else 1/scale
                error=float(np.max(np.abs(out.astype(float)/scale-expected)))
                record['max_normalized_error']=error
                assert error<=tolerance, f'texture-input projection error {error}'
            elif oracle in ['nearest-texture','bilinear-texture']:
                texture=np.array(Image.open(layers['Other Layer (Texture / UV Map)']).convert('RGBA'))
                th,tw=texture.shape[:2]
                u=pixels[:,:,0].astype(float)/255
                v=pixels[:,:,1].astype(float)/255
                if kind=='alpha-stripes':
                    # Zero-alpha UV has no recoverable coordinates. Nonzero
                    # stripes use coordinates safely inside nearest texels.
                    u=np.where(pixels[:,:,3]>0,u,0)
                    v=np.where(pixels[:,:,3]>0,v,0)
                tex=texture[:,:,[3,0,1,2]].astype(float)/255
                if depth=='argb16':
                    u=np.floor(u*32768+0.5)/32768
                    v=np.floor(v*32768+0.5)/32768
                    tex=np.floor(tex*32768+0.5)/32768
                if values.get('V Origin',2)==2: v=1-v
                if oracle=='nearest-texture':
                    tx=np.clip(np.floor(u*tw).astype(int),0,tw-1)
                    ty=np.clip(np.floor(v*th).astype(int),0,th-1)
                    expected=tex[ty,tx]
                else:
                    fx=u*tw-0.5; fy=v*th-0.5
                    x0=np.floor(fx).astype(int); y0=np.floor(fy).astype(int)
                    wx=(fx-x0)[:,:,None]; wy=(fy-y0)[:,:,None]
                    def wrap_index(indices,size):
                        mode=values.get('Wrap',1)
                        if mode==2: return indices%size
                        if mode==3:
                            folded=indices%(2*size)
                            return np.where(folded<size,folded,2*size-1-folded)
                        return np.clip(indices,0,size-1)
                    x1=wrap_index(x0+1,tw); y1=wrap_index(y0+1,th)
                    x0=wrap_index(x0,tw); y0=wrap_index(y0,th)
                    expected=(tex[y0,x0]*(1-wx)+tex[y0,x1]*wx)*(1-wy)+(tex[y1,x0]*(1-wx)+tex[y1,x1]*wx)*wy
                if kind=='alpha-stripes' and values.get('Use UV Alpha as Mask',1):
                    alpha=pixels[:,:,3].astype(float)/255
                    if depth=='argb16': alpha=np.floor(alpha*32768+0.5)/32768
                    expected*=alpha[:,:,None]
                tolerance=1e-6 if depth=='argb32f' else 1/scale
                error=float(np.max(np.abs(out.astype(float)/scale-expected)))
                record['max_normalized_error']=error
                assert error<=tolerance, f'{oracle} UV projection error {error} exceeds {tolerance}'
            elif oracle=='minimax-alpha':
                alpha=pixels[:,:,3].astype(np.int64)
                premult=np.floor(pixels[:,:,:3].astype(float)*alpha[:,:,None]/255+0.5).astype(np.int64)
                straight=np.where(alpha[:,:,None]>0,
                    np.minimum((premult*255+alpha[:,:,None]//2)//np.maximum(alpha[:,:,None],1),255),0)
                channels=np.concatenate([alpha[:,:,None],straight],axis=2)
                processed=channels.copy()
                selected=[0] if values['Channel']==2 else [1,2,3] if values['Channel']==1 else [0,1,2,3]
                radius=int(abs(values['Amount']))
                for y in range(h):
                    for x in range(w):
                        neighborhood=channels[y,max(0,x-radius):min(w,x+radius+1)]
                        reduced=neighborhood.min(axis=0) if values['Amount']<0 else neighborhood.max(axis=0)
                        processed[y,x,selected]=reduced[selected]
                mix=values.get('Mix with Original',100)/100
                blended=np.floor(channels*(1-mix)+processed*mix+0.5).astype(np.int64)
                expected=blended.copy()
                expected[:,:,1:]=(blended[:,:,1:]*blended[:,:,0,None]+127)//255
                assert np.array_equal(out,expected), 'alpha/channel morphology or premultiplication mismatch'
                assert np.all(out[:,:,1:]<=out[:,:,0,None]), 'RGB exceeds alpha in bounded premultiplied fixture'
            elif oracle=='minimax-horizontal':
                expected=pixels[:,:,[3,0,1,2]].copy()
                mapped=np.array(Image.open(layers['Radius Map']).convert('RGBA'))
                mh,mw=mapped.shape[:2]
                yy,xx=np.indices((h,w))
                sampled=mapped[yy*mh//h,xx*mw//w,:3].astype(float)
                level=(sampled[:,:,0]*0.2126+sampled[:,:,1]*0.7152+sampled[:,:,2]*0.0722)/255
                if values.get('Invert Map',0): level=1-level
                radius=abs(values['Amount'])*level
                low=np.floor(radius).astype(int); high=np.ceil(radius).astype(int); fraction=(radius-low)[:,:,None]
                channels=pixels[:,:,:3].copy()
                direction=values.get('Direction',1)
                for axis in ([1,0] if direction==3 else [1 if direction==1 else 0]):
                    def reduce_window(r):
                        padding=[(0,0),(0,0),(0,0)]; padding[axis]=(r,r)
                        if values.get('Repeat Edge Pixels',1):
                            padded=np.pad(channels,padding,mode='edge')
                        else:
                            padded=np.pad(channels,padding,mode='constant',constant_values=255 if values['Amount']<0 else 0)
                        windows=np.stack([padded[offset:offset+h] if axis==0 else padded[:,offset:offset+w] for offset in range(2*r+1)])
                        return (windows.min(axis=0) if values['Amount']<0 else windows.max(axis=0)).astype(float)
                    filtered=np.stack([reduce_window(r) for r in range(int(high.max())+1)])
                    channels=np.floor(filtered[low,yy,xx]*(1-fraction)+filtered[high,yy,xx]*fraction+0.5).astype(np.uint8)
                expected[:,:,1:]=channels
                assert np.array_equal(out,expected), 'mapped morphology differs from independent window reduction'
            elif oracle=='starglow-dark-map':
                assert not out[:,:,1:].any(), 'black luminance map did not suppress glow RGB'
            elif oracle=='starglow-active-map':
                assert np.any(out[:,:,1:]>0), 'active map baseline produced no glow RGB'
            elif oracle in ['refraction-spatial-mask','refraction-spatial-background']:
                is_mask=oracle=='refraction-spatial-mask'
                aux=np.array(Image.open(layers['Mask (shape)' if is_mask else 'Background']).convert('RGBA'))
                ah,aw=aux.shape[:2]; rh,rw=min(h,ah),min(w,aw)
                expected=np.zeros((h,w,4),dtype=float); expected[:,:,0]=1
                if is_mask:
                    expected[:rh,:rw,1:]=aux[:rh,:rw,3,None]/255
                else:
                    expected[:rh,:rw,1:]=aux[:rh,:rw,:3]/255
                tolerance=1/scale if depth!='argb32f' else 2e-6
                assert np.max(np.abs(out.astype(float)/scale-expected))<=tolerance, 'auxiliary image placement differs from unscaled top-left clipping'
            elif oracle=='refraction-aux-mask':
                alpha=np.array(Image.open(layers['Mask (shape)']).convert('RGBA'))[:,:,3].astype(float)/255
                expected=np.stack([np.ones((h,w)),alpha,alpha,alpha],axis=2)
                tolerance=1/scale if depth!='argb32f' else 2e-6
                assert np.max(np.abs(out.astype(float)/scale-expected))<=tolerance, 'selected mask alpha not reflected in mask output'
            elif oracle=='refraction-aux-background':
                expected=np.array([1,64/255,192/255,0])
                tolerance=1/scale if depth!='argb32f' else 2e-6
                assert np.max(np.abs(out.astype(float)/scale-expected))<=tolerance, 'selected background not sampled under neutral full-coverage render'
            elif oracle=='zero': assert not out.any(), 'expected transparent black'
            elif oracle=='mapped-median':
                map_pixels=np.array(Image.open(layers['Luminance Map']).convert('RGBA'))
                mh,mw=map_pixels.shape[:2]
                yy,xx=np.indices((h,w))
                sampled=map_pixels[yy*mh//h,xx*mw//w,0].astype(float)/255
                if values.get('Invert Map'): sampled=1-sampled
                radii=np.floor(values['Radius']*sampled+0.5).astype(int)
                expected=pixels[:,:,[3,0,1,2]].copy()
                for iteration in range(values.get('Iterations',1)):
                    previous=expected.copy()
                    for y in range(h):
                        for x in range(w):
                            radius=radii[y,x]
                            ys=np.clip(np.arange(y-radius,y+radius+1),0,h-1)
                            xs=np.clip(np.arange(x-radius,x+radius+1),0,w-1)
                            neighborhood=previous[ys[:,None],xs[None,:],1:].reshape(-1,3)
                            if values.get('Filter Type',1)==1:
                                expected[y,x,1:]=np.sort(neighborhood,axis=0)[len(neighborhood)//2]
                            else:
                                offsets=np.arange(-radius,radius+1)
                                sigma=max(radius*(values['Weight Falloff']/100*1.5+0.3),0.5)
                                weights=np.exp(-(offsets[:,None]**2+offsets[None,:]**2)/(2*sigma*sigma)).ravel()
                                for channel in range(3):
                                    order=np.argsort(neighborhood[:,channel],kind='stable')
                                    index=np.searchsorted(np.cumsum(weights[order]),weights.sum()/2,side='left')
                                    expected[y,x,channel+1]=neighborhood[order[index],channel]
                mix=values.get('Mix with Original',100)/100
                expected[:,:,1:]=np.floor(pixels[:,:,:3]*(1-mix)+expected[:,:,1:]*mix+0.5).astype(np.uint8)
                assert np.array_equal(out,expected), 'mapped median differs from independent sorted-window oracle'
            elif oracle=='opaque-white': assert np.all(out==scale), 'generated shape did not fill every channel'
            elif oracle=='planar':
                yy,xx=np.indices((h,w)); expected=np.stack([np.ones((h,w)),(xx+.5)/w,1-(yy+.5)/h,np.zeros((h,w))],axis=2)
                error=np.abs(out.astype(float)/scale-expected)
                assert error.max()<=1/scale if depth!='argb32f' else error.max()<1e-6, f'UV coordinate error {error.max()}'
            record.update(status='passed',sha256=hashlib.sha256(raw).hexdigest(),oracle=oracle or 'finite_and_complete_shape_only')
            return raw
        except Exception as e:
            message=str(e)
            if classify_render_error(oracle,message)=='blocked_host_error_transport':
                record.update(status='blocked_host_error_transport',error=message,
                    oracle='expected-model-error',limitation='Host rejected its error response; model error text and safe output are not verified. Not a passing negative test or a plugin crash classification.')
            else:
                record.update(status='failed_unclassified',error=message)
            return None
        finally:
            save(); print(f"{args.profile}: {label}: {record['status']}",flush=True)
    if args.profile=='mask-transform':
        apply_control=[p for p in params if p['name']=='Apply Transform']
        results.append(dict(case='apply-control-supervised',status='passed' if len(apply_control)==1 and apply_control[0].get('supervised') else 'failed_unclassified',
                            oracle='host_parameter_metadata_not_actual_apply'))
    if args.profile=='depth-onnx-missing-model':
        for depth in ['argb8','argb16','argb32f']:
            render(f'missing-model-{depth}',17,31,depth=depth,oracle='expected-model-error')
        save()
        # An unverified negative test must not produce a green CI exit.
        raise SystemExit(1)
    for w,h in [(1,1),(1,31),(17,1),(17,31),(63,65),(321,181),(127,257)]:
        for kind in ['black','pattern']:
            oracle='flat-normal' if args.profile=='luma-normal' and kind=='black' else 'passthrough' if args.profile in ['minimax-map','mask-offset','mask-transform','path-array'] else 'zero' if args.profile=='uv-project' else None
            render(f'{kind}-{w}x{h}',w,h,kind,oracle=oracle)
    render('transparent',63,65,'transparent',{'Alpha':2} if args.profile=='luma-normal' else {},
           oracle=None if args.profile in ['shape-plus','particlekit','lattice-lab','onmk-flare'] else 'alpha-opaque' if args.profile=='tui-image' else 'alpha-zero' if args.profile in ['luma-normal','luma-grain','rio-grade','onmk-film'] else 'zero')
    basic=render('repeat-a',63,65)
    repeat=render('repeat-b',63,65)
    classic_only=args.profile in ['mask-transform','rio-grade','lattice-lab','onmk-flare']
    smart=render('smart',63,65,path='smart') if not classic_only else basic
    if not args.only:
        results.append(dict(case='repeat_agreement' if classic_only else 'repeat_and_smart_agreement',status='passed' if basic is not None and basic==repeat==smart else 'failed_unclassified'))
    if args.profile=='luma-normal':
        render('zero-strength',63,65,values={'Strength':0},oracle='flat-normal')
        for blur,detail in [(0,1),(3,4)]:
            for ix in [0,1]:
                for iy in [0,1]:
                    for path in ['classic','smart']:
                        render(f'normal-field-{blur}-{detail}-{ix}-{iy}-{path}',17,31,path=path,
                               values={'Strength':4,'Pre-Blur':blur,'Detail Scale':detail,'Invert X':ix,'Invert Y':iy,'Alpha':1},oracle='normal-field')
        for alpha_mode in [1,2]:
            for path in ['classic','smart']:
                render(f'normal-alpha-{alpha_mode}-{path}',17,31,kind='alpha-stripes',path=path,
                       values={'Strength':0,'Alpha':alpha_mode},oracle='flat-normal-alpha')
        for width,height in [(1,1),(1,31),(37,1),(7,9)]:
            for blur,detail in [(0,16),(3,4),(100,16)]:
                for path in ['classic','smart']:
                    render(f'normal-boundary-{width}x{height}-{blur}-{detail}-{path}',width,height,path=path,
                           values={'Strength':4,'Pre-Blur':blur,'Detail Scale':detail,
                                   'Invert X':1,'Invert Y':0,'Alpha':1},oracle='normal-field')
    if args.profile=='minimax-map':
        for channel in [1,2,3]:
            for amount in [-2,2]:
                for mix in [0,50,100]:
                    for path in ['classic','smart']:
                        render(f'alpha-channel-{channel}-{amount}-{mix}-{path}',17,7,kind='alpha-stripes',path=path,
                               values={'Amount':amount,'Direction':1,'Channel':channel,'Mix with Original':mix},
                               oracle='minimax-alpha')
        spatial_map=root/'spatial-radius-map.png'
        my,mx=np.indices((11,7)); levels=np.array([0,64,128,255],dtype=np.uint8)[(mx+2*my)%4]
        Image.fromarray(np.stack([levels,levels,levels,np.full_like(levels,255)],axis=2)).save(spatial_map)
        for invert in [0,1]:
            for amount in [-2,2]:
                for direction in [1,2,3]:
                    for path in ['classic','smart']:
                        render(f'aux-spatial-radius-{invert}-{amount}-{direction}-{path}',17,31,path=path,
                               values={'Amount':amount,'Direction':direction,'Channel':1,'Invert Map':invert},
                               layers={'Radius Map':spatial_map},oracle='minimax-horizontal')
        changed=render('positive-radius',63,65,values={'Amount':3})
        if not args.only:
            results.append(dict(case='amount_has_effect',status='passed' if changed is not None and changed!=basic else 'failed_unclassified'))
    if args.profile=='luma-grain':
        for moment in [0,1,9]:
            for seed,size in [(0,1),(10000,32)]:
                for mono in [0,1]:
                    for path in ['classic','smart']:
                        render(f'grain-hash-{moment}-{seed}-{size}-{mono}-{path}',17,31,path=path,time=moment,
                               values={'Seed':seed,'Size':size,'Monochrome':mono,'Amount':100,
                                       'Shadows':100,'Midtones':100,'Highlights':100},oracle='grain-seed-time')
        for path in ['classic','smart']:
            for mono in [0,1]:
                render(f'alpha-grain-{path}-{mono}',17,31,kind='alpha-stripes',path=path,
                       values={'Monochrome':mono,'Amount':100},oracle='grain-premultiplied')
        render('zero-amount',63,65,values={'Amount':0},oracle='passthrough')
        for label,values,time in [('seed',{'Seed':7},0),('size',{'Size':7},0),('color',{'Monochrome':0},0),('time',{},1)]:
            changed=render(label,63,65,values=values,time=time)
            if not args.only:
                results.append(dict(case=label+'_has_effect',status='passed' if changed is not None and changed!=basic else 'failed_unclassified'))
        temporal_classic=render('time-classic',63,65,time=1)
        temporal_smart=render('time-smart',63,65,time=1,path='smart')
        if not args.only:
            results.append(dict(case='temporal_path_agreement',status='passed' if temporal_classic is not None and temporal_classic==temporal_smart else 'failed_unclassified'))
    if args.profile=='uv-project':
        texture=root/'constant-texture.png'
        Image.fromarray(np.tile(np.array([37,129,211,255],dtype=np.uint8),(11,7,1))).save(texture)
        spatial_texture=root/'spatial-texture.png'
        ty,tx=np.indices((11,7))
        spatial=np.stack([tx*31,ty*23,(tx*13+ty*7)%256,np.full_like(tx,255)],axis=2).astype(np.uint8)
        Image.fromarray(spatial).save(spatial_texture)
        constant_uv=root/'constant-uv.png'
        Image.fromarray(np.tile(np.array([64,192,0,255],dtype=np.uint8),(11,7,1))).save(constant_uv)
        for depth in ['argb8','argb16','argb32f']:
            for path in ['classic','smart']:
                render(f'planar-{depth}-{path}',321,181,values={'Input Is':3},depth=depth,path=path,oracle='planar')
                render(f'aux-texture-{depth}-{path}',17,31,depth=depth,path=path,oracle='constant-texture',
                       layers={'Other Layer (Texture / UV Map)':texture})
                for mask in [0,1]:
                    render(f'aux-alpha-{mask}-{depth}-{path}',17,31,kind='alpha-stripes',depth=depth,path=path,
                           values={'Use UV Alpha as Mask':mask},oracle='masked-constant-texture',
                           layers={'Other Layer (Texture / UV Map)':texture})
                render(f'aux-alpha-spatial-{depth}-{path}',17,31,kind='alpha-stripes',depth=depth,path=path,
                       values={'Sampling':2},oracle='nearest-texture',
                       layers={'Other Layer (Texture / UV Map)':spatial_texture})
                for wrap in [2,3]:
                    render(f'aux-wrap-{wrap}-{depth}-{path}',17,31,depth=depth,path=path,
                           values={'Sampling':1,'Wrap':wrap},oracle='bilinear-texture',
                           layers={'Other Layer (Texture / UV Map)':spatial_texture})
                for origin in [1,2]:
                    for opacity in [0,50,100]:
                        render(f'aux-role-{origin}-{opacity}-{depth}-{path}',17,31,depth=depth,path=path,
                               values={'Input Is':2,'Sampling':2,'V Origin':origin,'Opacity':opacity},
                               oracle='texture-role',layers={'Other Layer (Texture / UV Map)':constant_uv})
                    render(f'aux-spatial-{origin}-{depth}-{path}',17,31,depth=depth,path=path,
                           values={'Sampling':2,'V Origin':origin},oracle='nearest-texture',
                           layers={'Other Layer (Texture / UV Map)':spatial_texture})
                    render(f'aux-bilinear-{origin}-{depth}-{path}',17,31,depth=depth,path=path,
                           values={'Sampling':1,'V Origin':origin},oracle='bilinear-texture',
                           layers={'Other Layer (Texture / UV Map)':spatial_texture})
    if args.profile=='minimax-map':
        for level in [0,128,255]:
            radius_map=root/f'radius-map-{level}.png'
            mapped=np.full((11,7,4),level,dtype=np.uint8); mapped[:,:,3]=255
            Image.fromarray(mapped).save(radius_map)
            for invert in [0,1]:
                active=(255-level if invert else level)>0
                for amount in [-2,2]:
                    for path in ['classic','smart']:
                        render(f'aux-radius-{level}-{invert}-{amount}-{path}',17,31,path=path,
                               values={'Amount':amount,'Direction':1,'Channel':1,'Invert Map':invert},
                               layers={'Radius Map':radius_map},oracle='minimax-horizontal' if active else 'passthrough')
                        if level==128 and invert==0:
                            for direction in [2,3]:
                                for repeat in [0,1]:
                                    render(f'aux-direction-{direction}-{repeat}-{amount}-{path}',17,31,path=path,
                                           values={'Amount':amount,'Direction':direction,'Channel':1,'Repeat Edge Pixels':repeat},
                                           layers={'Radius Map':radius_map},oracle='minimax-horizontal')
    if args.profile=='scatter-map':
        zero_map=root/'scatter-zero-map.png'
        Image.fromarray(np.tile(np.array([0,0,0,255],dtype=np.uint8),(7,3,1))).save(zero_map)
        checker_map=root/'scatter-checker-map.png'
        cy,cx=np.indices((7,3)); gray=((cx+cy)%2*255).astype(np.uint8)
        Image.fromarray(np.stack([gray,gray,gray,np.full_like(gray,255)],axis=2)).save(checker_map)
        fractional_map=root/'scatter-fractional-map.png'
        gray=np.where((cx+cy)%2,192,64).astype(np.uint8)
        Image.fromarray(np.stack([gray,gray,gray,np.full_like(gray,255)],axis=2)).save(fractional_map)
        for depth in ['argb8','argb16','argb32f']:
            for path in ['classic','smart']:
                render(f'fractional-source-{depth}-{path}',17,31,depth=depth,path=path,values={'Scatter Amount':0})
                for direction in [1,2,3]:
                    for repeat in [0,1]:
                        render(f'fractional-map-{direction}-{repeat}-{depth}-{path}',17,31,depth=depth,path=path,
                               values={'Scatter Amount':23,'Random Seed':37,'Direction':direction,'Repeat Edge Pixels':repeat,'Invert Map':repeat},
                               layers={'Scatter Map':fractional_map},oracle='scatter-fractional-map')
                        render(f'fractional-mix-{direction}-{repeat}-{depth}-{path}',17,31,depth=depth,path=path,
                               values={'Scatter Amount':23,'Random Seed':37,'Direction':direction,'Repeat Edge Pixels':repeat,'Invert Map':repeat,'Mix with Original':37},
                               layers={'Scatter Map':fractional_map},oracle='scatter-fractional-map')
                render(f'spatial-source-{depth}-{path}',17,31,depth=depth,path=path,values={'Scatter Amount':0})
                render(f'spatial-base-{depth}-{path}',17,31,depth=depth,path=path,values={'Scatter Amount':50})
                for invert in [0,1]:
                    render(f'spatial-map-{invert}-{depth}-{path}',17,31,depth=depth,path=path,
                           values={'Scatter Amount':50,'Invert Map':invert},layers={'Scatter Map':checker_map},oracle='scatter-spatial-map')
                render(f'aux-zero-{depth}-{path}',17,31,depth=depth,path=path,
                       values={'Scatter Amount':50},layers={'Scatter Map':zero_map},oracle='passthrough')
    if args.profile=='celglow':
        for depth in ['argb8','argb16','argb32f']:
            for radius in [3,64]:
                render(f'source-blur-{radius}-{depth}',17,31,depth=depth,path='smart',values={'Source Box Blur (px)':radius})
    if args.profile=='prism-warp':
        maps={}
        ramp_lens=root/'prism-ramp-lens.png'
        ramp=np.tile(np.arange(17,dtype=np.uint8)*15,(31,1))
        Image.fromarray(np.stack([ramp,ramp,ramp,np.full_like(ramp,255)],axis=2)).save(ramp_lens)
        spatial_matte=root/'prism-spatial-matte.png'
        my,mx=np.indices((31,17)); gray=((mx//3+my//5)%2*255).astype(np.uint8)
        Image.fromarray(np.stack([gray,gray,gray,np.full_like(gray,255)],axis=2)).save(spatial_matte)
        for level in [0,128,255]:
            maps[level]=root/f'prism-constant-{level}.png'
            Image.fromarray(np.tile(np.array([level,level,level,255],dtype=np.uint8),(7,3,1))).save(maps[level])
        for depth in ['argb8','argb16','argb32f']:
            for path in ['classic','smart']:
                settings={'Strength (px)':20,'Lens Blur':0,'Edge Threshold':0}
                for strength in [-2.5,2.5]:
                    for direction in [0,90]:
                        render(f'ramp-prism-{strength}-{direction}-{depth}-{path}',17,31,depth=depth,path=path,
                               values={'Strength (px)':strength,'Direction':direction,'Dispersion (px)':0,'Lens Blur':0,'Edge Threshold':0,'Edge Softness':0},
                               layers={'Lens':ramp_lens},oracle='prism-ramp-lens')
                render(f'aux-prism-base-{depth}-{path}',17,31,depth=depth,path=path,values=settings,oracle='prism-active')
                for invert in [0,1]:
                    render(f'aux-prism-spatial-{invert}-{depth}-{path}',17,31,depth=depth,path=path,
                           values={**settings,'Invert Matte':invert},layers={'Matte':spatial_matte},oracle='prism-spatial-matte')
                for level in [0,128,255]:
                    render(f'aux-prism-lens-{level}-{depth}-{path}',17,31,depth=depth,path=path,values=settings,
                           layers={'Lens':maps[level]},oracle='prism-flat-map')
                for level,invert in [(0,0),(255,1)]:
                    render(f'aux-prism-matte-{level}-{depth}-{path}',17,31,depth=depth,path=path,
                           values={**settings,'Invert Matte':invert},layers={'Matte':maps[level]},oracle='prism-flat-map')
    if args.profile=='onmk-glow' and (not args.only or args.only.startswith('channels-glow')):
        for depth in ['argb8','argb16','argb32f']:
            for path in ['classic','smart']:
                prefix=f'channels-glow-{depth}-{path}'
                for alpha in [0,0.5,1]:
                    render(prefix+f'-preview-{alpha}',17,31,depth=depth,path=path,oracle='glow-threshold',
                        values={'Show':2,'Threshold':0.5,'Threshold Add Red':-0.15,
                            'Threshold Add Green':0.1,'Threshold Add Blue':0.25,'Glow From Alpha':alpha})
                settings={'Brightness':0.25,'Glow Width':0.3,'Falloff':0,'Threshold':0,
                    'Scale Source':0,'After Glow Width':0,'Horizontal Streaks':0,'Vertical Streaks':0,
                    'Combine':2,'Mix with Original':100}
                base=render(prefix+'-base',17,31,depth=depth,path=path,values=settings)
                isolated=[render(prefix+'-'+channel,17,31,depth=depth,path=path,
                    values={**settings,**{'Color '+c:int(c==channel) for c in ['Red','Green','Blue']}})
                    for channel in ['Red','Green','Blue']]
                if not args.only or args.only=='channels-glow':
                    passed=False
                    if base is not None and all(raw is not None for raw in isolated):
                        dtype={'argb8':'u1','argb16':'<u2','argb32f':'<f4'}[depth]
                        reference=np.frombuffer(base,dtype=dtype).reshape(31,17,4)
                        passed=True
                        for c,raw in enumerate(isolated,1):
                            expected=np.zeros_like(reference);expected[:,:,0]=reference[:,:,0];expected[:,:,c]=reference[:,:,c]
                            passed=passed and np.any(reference[:,:,c]>0) and raw==expected.tobytes()
                    results.append(dict(case=prefix+'-isolation',status='passed' if passed else 'failed_unclassified',
                        oracle='isolated_core_color_channel_identity_and_other_channels_zero'))
                    save()
    if args.profile=='onmk-glow' and (not args.only or args.only.startswith('controls-glow')):
        for depth in ['argb8','argb16','argb32f']:
            for path in ['classic','smart']:
                settings={'Brightness':0.25,'Glow Width':0.3,'Falloff':0,'Threshold':0,
                    'Scale Source':0,'After Glow Width':0,'Horizontal Streaks':0,
                    'Vertical Streaks':0,'Combine':2,'Mix with Original':100}
                prefix=f'controls-glow-{depth}-{path}'
                outputs={label:render(prefix+'-'+label,17,31,depth=depth,path=path,values={**settings,**changes})
                    for label,changes in [('base',{}),('zero',{'Brightness':0}),('double',{'Brightness':0.5}),
                        ('threshold',{'Threshold':1}),('width',{'Glow Width':0})]}
                if not args.only or args.only=='controls-glow':
                    passed=False
                    if all(raw is not None for raw in outputs.values()):
                        dtype={'argb8':'u1','argb16':'<u2','argb32f':'<f4'}[depth]
                        arrays={key:np.frombuffer(raw,dtype=dtype).reshape(31,17,4).astype(np.float64) for key,raw in outputs.items()}
                        base=arrays['base'][:,:,1:]; high=arrays['threshold'][:,:,1:]
                        tol=2e-6 if depth=='argb32f' else 1
                        passed=(np.any(base>0) and np.all(arrays['zero'][:,:,1:]==0)
                            and np.max(np.abs(arrays['double'][:,:,1:]-2*base))<=tol
                            and np.all(high<=base+tol) and np.any(high<base-tol)
                            and outputs['width']!=outputs['base']
                            and all(np.array_equal(a[:,:,0],arrays['base'][:,:,0]) for a in arrays.values()))
                    results.append(dict(case=prefix+'-relationships',status='passed' if passed else 'failed_unclassified',
                        oracle='isolated_glow_zero_linear_brightness_threshold_monotonic_width_response_alpha_invariant'))
                    save()
    if args.profile=='adaptive-filter' and (not args.only or args.only.startswith('spatial-adaptive')):
        yy,xx=np.indices((11,7))
        levels=np.array([0,64,96,128,160,192,224,255],dtype=np.uint8)[(xx+3*yy)%8]
        map_path=root/'adaptive-spatial-map.png'
        Image.fromarray(np.stack([levels,levels,levels,np.full_like(levels,255)],axis=2)).save(map_path)
        yy,xx=np.indices((31,17))
        expanded=levels[yy*11//31,xx*7//17].astype(np.int32)
        for mode in [1,2,3]:
            for depth in ['argb8','argb16','argb32f']:
                for path in ['classic','smart']:
                    prefix=f'spatial-adaptive-{mode}-{depth}-{path}'
                    settings={'Filter Type':mode,'Radius':4,'Iterations':1,'Mix with Original':100}
                    bank=[render(prefix+f'-radius-{radius}',17,31,depth=depth,path=path,
                        values={**settings,'Radius':radius}) for radius in range(1,5)]
                    for invert in [0,1]:
                        actual=render(prefix+f'-map-{invert}',17,31,depth=depth,path=path,
                            values={**settings,'Invert Map':invert},layers={'Luminance Map':map_path})
                        if not args.only or args.only=='spatial-adaptive':
                            passed=False
                            if actual is not None and all(raw is not None for raw in bank):
                                dtype={'argb8':'u1','argb16':'<u2','argb32f':'<f4'}[depth]
                                references=np.stack([np.frombuffer(raw,dtype=dtype).reshape(31,17,4) for raw in bank])
                                v=255-expanded if invert else expanded
                                # Integer half-up rounding, independent of plugin luma/radius helpers.
                                radius=np.maximum(1,(8*v+255)//510)
                                expected=references[radius-1,yy,xx]
                                passed=actual==expected.tobytes() and len(set(bank))>1
                            results.append(dict(case=prefix+f'-agreement-{invert}',
                                status='passed' if passed else 'failed_unclassified',
                                oracle='spatial_nearest_map_integer_radius_bank_not_independent_filter_kernel'))
                            save()
    if args.profile=='adaptive-filter' and (not args.only or args.only.startswith('aux-adaptive')):
        maps={}
        for label,level in [('black',0),('white',255)]:
            pixels=np.empty((11,7,4),dtype=np.uint8); pixels[:]=[level,level,level,255]
            maps[label]=root/f'adaptive-{label}-map.png'; Image.fromarray(pixels).save(maps[label])
        for mode in [1,2,3]:
            for depth in ['argb8','argb16','argb32f']:
                for path in ['classic','smart']:
                    settings={'Filter Type':mode,'Radius':4,'Iterations':2,'Mix with Original':100}
                    prefix=f'aux-adaptive-{mode}-{depth}-{path}'
                    full=render(prefix+'-full',17,31,depth=depth,path=path,values=settings)
                    minimum=render(prefix+'-minimum',17,31,depth=depth,path=path,values={**settings,'Radius':1})
                    outputs={}
                    for label,invert in [('black',0),('white',0),('black',1),('white',1)]:
                        outputs[label,invert]=render(prefix+f'-{label}-{invert}',17,31,depth=depth,path=path,
                            values={**settings,'Invert Map':invert},layers={'Luminance Map':maps[label]})
                    if not args.only or args.only=='aux-adaptive':
                        results.append(dict(case=prefix+'-agreement',
                            status='passed' if full is not None and minimum is not None and full!=minimum
                                and full==outputs['white',0]==outputs['black',1]
                                and minimum==outputs['black',0]==outputs['white',1] else 'failed_unclassified',
                            oracle='constant_map_radius_equivalence_with_nonvacuous_radius_response'))
                        save()
    if args.profile in ['scatter-map','prism-warp','adaptive-filter','median-pro','onmk-glow','refraction-dispersion']:
        depths=['argb8'] if args.profile=='median-pro' else ['argb8','argb16','argb32f']
        for depth in depths:
            for path in ['classic','smart']:
                render(f'identity-{depth}-{path}',321,181,values={'Mix with Original':0},depth=depth,path=path,oracle='passthrough')
    if args.profile in ['adaptive-filter','median-pro']:
        for mode in range(1,4 if args.profile=='adaptive-filter' else 3):
            render(f'mode-{mode}-small',17,31,values={'Filter Type':mode,'Radius':7,'Iterations':2})
            if args.profile=='adaptive-filter':
                for depth in ['argb16','argb32f']:
                    render(f'mode-{mode}-{depth}',17,31,depth=depth,values={'Filter Type':mode,'Radius':3,'Iterations':2})
        if args.profile=='median-pro':
            render('zero-radius',63,65,values={'Radius':0},oracle='passthrough')
            my,mx=np.indices((11,7))
            for label,levels in [('black',np.zeros((11,7),dtype=np.uint8)),('white',np.full((11,7),255,dtype=np.uint8)),
                                 ('gray',np.full((11,7),128,dtype=np.uint8)),('spatial',np.array([0,64,128,255],dtype=np.uint8)[(mx+2*my)%4])]:
                radius_map=root/f'median-{label}-map.png'
                Image.fromarray(np.stack([levels,levels,levels,np.full_like(levels,255)],axis=2)).save(radius_map)
                for invert in [0,1]:
                    for path in ['classic','smart']:
                        render(f'aux-median-{label}-{invert}-{path}',17,31,path=path,
                               values={'Filter Type':1,'Radius':2,'Iterations':1,'Invert Map':invert,'Mix with Original':100},
                               layers={'Luminance Map':radius_map},oracle='mapped-median')
                if label=='spatial':
                    for mode in [1,2]:
                        for iterations in [2,3]:
                            for mix in [25,50]:
                                for path in ['classic','smart']:
                                    render(f'aux-iterations-{mode}-{iterations}-{mix}-{path}',17,31,path=path,
                                           values={'Filter Type':mode,'Radius':2,'Iterations':iterations,'Invert Map':0,'Weight Falloff':50,'Mix with Original':mix},
                                           layers={'Luminance Map':radius_map},oracle='mapped-median')
                    for falloff in [0,50,100]:
                        for invert in [0,1]:
                            for path in ['classic','smart']:
                                render(f'aux-weighted-{falloff}-{invert}-{path}',17,31,path=path,
                                       values={'Filter Type':2,'Radius':2,'Iterations':1,'Invert Map':invert,'Weight Falloff':falloff,'Mix with Original':100},
                                       layers={'Luminance Map':radius_map},oracle='mapped-median')
    if args.profile=='edge-smith':
        for depth in ['argb8','argb16','argb32f']:
            for path in ['classic','smart']:
                render(f'identity-{depth}-{path}',321,181,values={'Mix':0,'GPU Acceleration':0},depth=depth,path=path,oracle='passthrough')
    if args.profile=='rio-grade':
        control_default=render('controls-default',63,65)
        for index,(name,low,high) in enumerate([
                ('Strength',0,100),('Purple / Pink',-100,100),('Warm Bottom',0,100),
                ('Saturation',0,200),('Contrast',0,200),('Color Fade',0,100),
                ('Film Grain',0,100),('Vignette',0,100)]):
            low_image=render(f'controls-{index}-low',63,65,values={name:low})
            high_image=render(f'controls-{index}-high',63,65,values={name:high})
            if args.only is None or args.only=='controls':
                results.append(dict(case=f'controls-{index}-response',parameter=name,
                    status='passed' if low_image is not None and high_image is not None and low_image!=high_image else 'failed_unclassified',
                    oracle='endpoints_change_output_not_visual_correctness'))
        control_restored=render('controls-default-restored',63,65)
        if args.only is None or args.only=='controls':
            results.append(dict(case='controls-default-agreement',status='passed' if control_default is not None and control_default==control_restored else 'failed_unclassified',
                oracle='default_repeat_independent_workers_not_cache_invalidation'))
        render('zero-strength',63,65,values={'Strength':0},oracle='passthrough')
        for strength in [0,50,100]:
            render(f'alpha-strength-{strength}',17,31,kind='alpha-stripes',values={'Strength':strength},oracle='rio-premultiplied')
        render('transparent-rgb',17,31,kind='transparent',oracle='zero')
    if args.profile in ['shape-plus','starglow']:
        values={'Opacity':0,'Composite':1} if args.profile=='shape-plus' else {'Starglow Opacity':0,'Source Opacity':100}
        for depth in ['argb8','argb16','argb32f']:
            for path in ['classic','smart']:
                render(f'identity-{depth}-{path}',321,181,values=values,depth=depth,path=path,oracle='passthrough')
    if args.profile=='starglow' and (not args.only or args.only.startswith('aux-starglow')):
        maps={}
        for label,level in [('black',0),('white',255)]:
            pixels=np.empty((11,7,4),dtype=np.uint8); pixels[:]=[level,level,level,255]
            maps[label]=root/f'starglow-{label}-map.png'; Image.fromarray(pixels).save(maps[label])
        for depth in ['argb8','argb16','argb32f']:
            for path in ['classic','smart']:
                values={'Source Opacity':0,'Starglow Opacity':100,'Threshold':0}
                baseline=render(f'aux-starglow-base-{depth}-{path}',17,31,depth=depth,path=path,
                    values=values,oracle='starglow-active-map')
                outputs={}
                for label in ['black','white']:
                    for invert in [0,1]:
                        active=(label=='white') != bool(invert)
                        outputs[label,invert]=render(f'aux-starglow-{label}-{invert}-{depth}-{path}',17,31,depth=depth,path=path,
                            values={**values,'Invert Map':invert},layers={'Luminance Map':maps[label]},
                            oracle='starglow-active-map' if active else 'starglow-dark-map')
                results.append(dict(case=f'aux-starglow-role-agreement-{depth}-{path}',
                    status='passed' if baseline is not None and baseline==outputs['white',0]==outputs['black',1]
                        and outputs['black',0] is not None and outputs['black',0]==outputs['white',1] else 'failed_unclassified',
                    oracle='map_white_identity_black_suppression_and_inversion'))
                save()
    if args.profile=='starglow' and (not args.only or args.only.startswith('spatial-starglow')):
        yy,xx=np.indices((11,7))
        small=np.empty((11,7,4),dtype=np.uint8)
        small[:,:,0]=(xx*37+yy*13)%256; small[:,:,1]=(xx*19+yy*29)%256
        small[:,:,2]=(xx*11+yy*7)%256; small[:,:,3]=255
        yy,xx=np.indices((31,17)); expanded=small[yy*11//31,xx*7//17]
        small_path=root/'starglow-spatial-small.png'; Image.fromarray(small).save(small_path)
        expanded_path=root/'starglow-spatial-expanded.png'; Image.fromarray(expanded).save(expanded_path)
        for depth in ['argb8','argb16','argb32f']:
            for path in ['classic','smart']:
                for invert in [0,1]:
                    values={'Source Opacity':0,'Starglow Opacity':100,'Threshold':0,'Invert Map':invert}
                    a=render(f'spatial-starglow-small-{invert}-{depth}-{path}',17,31,depth=depth,path=path,
                        values=values,layers={'Luminance Map':small_path},oracle='starglow-active-map')
                    b=render(f'spatial-starglow-expanded-{invert}-{depth}-{path}',17,31,depth=depth,path=path,
                        values=values,layers={'Luminance Map':expanded_path},oracle='starglow-active-map')
                    if not args.only or args.only=='spatial-starglow':
                        results.append(dict(case=f'spatial-starglow-agreement-{invert}-{depth}-{path}',
                            status='passed' if a is not None and a==b else 'failed_unclassified',
                            oracle='nearest_expansion_equivalence_not_independent_glow_model'))
                        save()
    if args.profile=='refraction-dispersion' and (not args.only or args.only.startswith('aux-refraction')):
        mask=np.zeros((31,17,4),dtype=np.uint8)
        mask[:,:,:3]=[192,64,128]
        mask[:,:,3]=np.array([0,64,128,255],dtype=np.uint8)[np.arange(17)%4]
        mask_path=root/'refraction-mask.png'; Image.fromarray(mask).save(mask_path)
        background=np.empty((31,17,4),dtype=np.uint8); background[:]=[64,192,0,255]
        background_path=root/'refraction-background.png'; Image.fromarray(background).save(background_path)
        for depth in ['argb8','argb16','argb32f']:
            for path in ['classic','smart']:
                render(f'aux-refraction-mask-{depth}-{path}',17,31,depth=depth,path=path,
                    values={'Output Mode':3,'Coverage Source':1,'Map Blur':0,'Edge Blur':0},
                    layers={'Mask (shape)':mask_path},oracle='refraction-aux-mask')
                render(f'aux-refraction-background-{depth}-{path}',17,31,depth=depth,path=path,
                    values={'Output Mode':1,'Coverage Source':3,'Height Strength':0,
                            'Map Blur':0,'Edge Blur':0,'Background Blur':0,'Mix with Original':100,
                            'Diffuseness':0,'Refract Power':0},
                    layers={'Mask (shape)':mask_path,'Background':background_path},oracle='refraction-aux-background')
    if args.profile=='refraction-dispersion' and (not args.only or args.only.startswith('aux-refraction-spatial')):
        for aw,ah in [(7,11),(23,37)]:
            yy,xx=np.indices((ah,aw))
            mask=np.zeros((ah,aw,4),dtype=np.uint8); mask[:,:,:3]=[192,64,128]
            mask[:,:,3]=np.array([0,64,128,255],dtype=np.uint8)[(xx+2*yy)%4]
            mask_path=root/f'refraction-spatial-mask-{aw}x{ah}.png'; Image.fromarray(mask).save(mask_path)
            background=np.empty((ah,aw,4),dtype=np.uint8)
            background[:,:,0]=(xx*17+yy*3)%256; background[:,:,1]=(xx*7+yy*19)%256
            background[:,:,2]=(xx*11+yy*5)%256; background[:,:,3]=255
            background_path=root/f'refraction-spatial-background-{aw}x{ah}.png'; Image.fromarray(background).save(background_path)
            for depth in ['argb8','argb16','argb32f']:
                for path in ['classic','smart']:
                    render(f'aux-refraction-spatial-mask-{aw}x{ah}-{depth}-{path}',17,31,depth=depth,path=path,
                        values={'Output Mode':3,'Coverage Source':1,'Map Blur':0,'Edge Blur':0},
                        layers={'Mask (shape)':mask_path},oracle='refraction-spatial-mask')
                    render(f'aux-refraction-spatial-background-{aw}x{ah}-{depth}-{path}',17,31,depth=depth,path=path,
                        values={'Output Mode':1,'Coverage Source':3,'Height Strength':0,
                                'Map Blur':0,'Edge Blur':0,'Background Blur':0,'Mix with Original':100,
                                'Diffuseness':0,'Refract Power':0},
                        layers={'Mask (shape)':mask_path,'Background':background_path},oracle='refraction-spatial-background')
    if args.profile in ['starglow','onmk-glow','refraction-dispersion']:
        for depth in ['argb16','argb32f']:
            classic=render(f'active-{depth}-classic',63,65,depth=depth,path='classic')
            smart=render(f'active-{depth}-smart',63,65,depth=depth,path='smart')
            if not args.only:
                results.append(dict(case=f'active-{depth}-agreement',status='passed' if classic is not None and classic==smart else 'failed_unclassified'))
            if args.profile=='refraction-dispersion':
                render(f'gpu-requested-{depth}',63,65,depth=depth,values={'Use GPU':1})
    if args.profile=='shape-plus':
        for shape in [1,2,3]:
            for depth in ['argb8','argb16','argb32f']:
                render(f'filled-shape-{shape}-{depth}',17,31,'transparent',values={'Shape':shape,'Composite':2,'Size X':30000,'Size Y':30000,'Feather':0},depth=depth,oracle='opaque-white')
    if args.profile in ['celglow','celglow2']:
        for depth in ['argb8','argb16','argb32f']:
            values={'View':3} if args.profile=='celglow' else {'View':1,'Glow Opacity (%)':0}
            render(f'identity-{depth}-smart',321,181,values=values,depth=depth,oracle='passthrough')
    if args.profile=='onmk-film':
        if sys.platform=='win32':
            for depth in ['argb8','argb16','argb32f']:
                cpu=render(f'gpu-fallback-cpu-{depth}',65,65,values={'Use GPU':0},depth=depth)
                fallback=render(f'gpu-fallback-request-{depth}',65,65,values={'Use GPU':1},depth=depth)
                if args.only is None or args.only=='gpu-fallback':
                    results.append(dict(case=f'gpu-fallback-equality-{depth}',status='passed' if cpu is not None and cpu==fallback else 'failed_unclassified',oracle='windows_metal_unavailable_cpu_fallback_not_gpu_execution'))
        for depth in ['argb8','argb16','argb32f']:
            render(f'bypass-{depth}',63,65,values={'Bypass':1,'Use GPU':0},depth=depth,oracle='passthrough')
    if args.profile=='onmk-flare':
        for depth in ['argb8','argb16']:
            render(f'identity-{depth}',321,181,values={'Flare Opacity':0,'Source Opacity':100},depth=depth,oracle='passthrough')
    if args.profile=='frame-slice-static':
        for mode in [1,2,3,4]:
            render(f'static-mode-{mode}',63,65,values={'Slice Mode':mode},oracle='passthrough',time=1)
            render(f'classic-static-mode-{mode}',63,65,values={'Slice Mode':mode},oracle='passthrough',time=1,path='classic')
        render('static-mix-half',63,65,values={'Mix with Original':50},oracle='passthrough',time=1)
    if args.profile in ['particlekit','lattice-lab']:
        render('time-1',321,181,time=1)
        if args.profile=='lattice-lab':
            render('deep-16',321,181,depth='argb16',time=1,oracle='lattice-native16')
    save()
    raise SystemExit(1 if any(r['status'].startswith('failed') for r in results) else 0)


if __name__=='__main__': main()
